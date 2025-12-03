// File: ./src/tui/mod.rs
pub mod action;
pub mod handlers;
pub mod network;
pub mod state;
pub mod view;

use crate::config;
use crate::tui::state::{AppState, InputMode};
use crate::tui::view::draw;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, MouseEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{env, io, time::Duration};
use tokio::sync::mpsc;

pub async fn run() -> Result<()> {
    // --- 1. PREAMBLE & CONFIG ---
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        println!("Usage: cfait [OPTIONS]");
        return Ok(());
    }

    // Panic Hook
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("cfait_panic.log")
        {
            let _ = writeln!(file, "PANIC: {:?}", info);
        }
        default_hook(info);
    }));

    let config_result = config::Config::load();
    let (
        url,
        user,
        pass,
        default_cal,
        hide_completed,
        hide_fully_completed_tags,
        tag_aliases,
        sort_cutoff,
        allow_insecure,
        hidden_calendars,
        disabled_calendars,
    ) = match config_result {
        Ok(cfg) => (
            cfg.url,
            cfg.username,
            cfg.password,
            cfg.default_calendar,
            cfg.hide_completed,
            cfg.hide_fully_completed_tags,
            cfg.tag_aliases,
            cfg.sort_cutoff_months,
            cfg.allow_insecure_certs,
            cfg.hidden_calendars,
            cfg.disabled_calendars,
        ),
        Err(_) => {
            let path_str =
                config::Config::get_path_string().unwrap_or("[path unknown]".to_string());
            eprintln!("Config file not found: {}", path_str);
            return Ok(());
        }
    };

    // --- 2. TERMINAL SETUP ---
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // --- 3. STATE INIT ---
    let mut app_state = AppState::new();
    app_state.hide_completed = hide_completed;
    app_state.hide_fully_completed_tags = hide_fully_completed_tags;
    app_state.tag_aliases = tag_aliases;
    app_state.sort_cutoff_months = sort_cutoff;
    app_state.hidden_calendars = hidden_calendars.into_iter().collect();
    app_state.disabled_calendars = disabled_calendars.into_iter().collect();

    let (action_tx, action_rx) = mpsc::channel(10);
    let (event_tx, mut event_rx) = mpsc::channel(10);

    // --- 4. NETWORK THREAD ---
    tokio::spawn(network::run_network_actor(
        url,
        user,
        pass,
        allow_insecure,
        default_cal.clone(), // Clone for the thread
        action_rx,
        event_tx,
    ));

    // --- 5. UI LOOP ---
    loop {
        terminal.draw(|f| draw(f, &mut app_state))?;

        // A. Network Events
        if let Ok(event) = event_rx.try_recv() {
            handlers::handle_app_event(&mut app_state, event, &default_cal);
        }

        // B. Input Events
        if crossterm::event::poll(Duration::from_millis(50))? {
            let event = event::read()?;
            match event {
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollDown => app_state.next(),
                    MouseEventKind::ScrollUp => app_state.previous(),
                    _ => {}
                },
                Event::Key(key) => {
                    if let Some(action) =
                        handlers::handle_key_event(key, &mut app_state, &action_tx).await
                    {
                        if matches!(action, action::Action::Quit) {
                            break;
                        }
                        let _ = action_tx.send(action).await;
                    }
                    // Handle mode-specific transient state updates that don't produce Actions
                    // (e.g. typing characters into buffer)
                    // These are handled inside handle_key_event via direct state mutation,
                    // but Quit needs explicit break.
                    if matches!(app_state.mode, InputMode::Normal)
                        && key.code == crossterm::event::KeyCode::Char('q')
                    {
                        // Double check redundant safety break if handler returned None
                        // (Handler returns Action::Quit, dealt with above. This block is safe to skip)
                    }
                }
                _ => {}
            }
        }
    }

    // --- 6. CLEANUP ---
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
