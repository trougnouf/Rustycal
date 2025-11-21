mod client;
mod config;
mod model;
mod ui; // <--- Module added

use crate::client::RustyClient;
use crate::model::Task;
use crate::ui::{AppState, draw};
use anyhow::Result;
use crossterm::{
    // Add MouseEventKind to the list
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{env, io, time::Duration};
use tokio::sync::mpsc;

enum Action {
    ToggleTask(usize),
    CreateTask(String),
    DeleteTask(usize),
    ChangePriority(usize, i8),
    Quit,
}

enum AppEvent {
    TasksLoaded(Vec<Task>),
    #[allow(dead_code)]
    TaskUpdated(Task),
    Error(String),
    Status(String),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Panic Hook
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("rustycal_panic.log")
        {
            let _ = writeln!(file, "PANIC: {:?}", info);
        }
        default_hook(info);
    }));

    // --- CONFIGURATION LOGIC ---
    // Try to load from file, fallback to args
    let (url, user, pass) = match config::Config::load() {
        Ok(cfg) => (cfg.url, cfg.username, cfg.password),
        Err(_) => {
            let args: Vec<String> = env::args().collect();
            if args.len() < 4 {
                eprintln!("Usage: rustycal <URL> <USER> <PASS>");
                eprintln!("Or create config at ~/.config/rustycal/config.toml");
                return Ok(());
            }
            (args[1].clone(), args[2].clone(), args[3].clone())
        }
    };
    // ---------------------------

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = AppState::new();
    let (action_tx, mut action_rx) = mpsc::channel(10);
    let (event_tx, mut event_rx) = mpsc::channel(10);

    // SPAWN ACTOR
    // Variables url, user, pass are moved into this block
    tokio::spawn(async move {
        let mut client = match RustyClient::new(&url, &user, &pass) {
            Ok(c) => c,
            Err(e) => {
                let _ = event_tx.send(AppEvent::Error(e)).await;
                return;
            }
        };

        let _ = event_tx
            .send(AppEvent::Status("Connecting...".to_string()))
            .await;

        if let Err(e) = client.discover_calendar().await {
            let _ = event_tx.send(AppEvent::Error(e)).await;
            return;
        }

        let _ = event_tx
            .send(AppEvent::Status("Fetching tasks...".to_string()))
            .await;

        let mut local_tasks: Vec<Task> = match client.get_tasks().await {
            Ok(t) => t,
            Err(e) => {
                let _ = event_tx.send(AppEvent::Error(e)).await;
                return;
            }
        };

        local_tasks.sort();
        let _ = event_tx
            .send(AppEvent::TasksLoaded(local_tasks.clone()))
            .await;

        while let Some(action) = action_rx.recv().await {
            match action {
                Action::Quit => break,

                Action::CreateTask(summary) => {
                    let _ = event_tx
                        .send(AppEvent::Status("Creating...".to_string()))
                        .await;

                    // Task::new() now parses !1 and @tomorrow
                    let mut new_task = Task::new(&summary);

                    match client.create_task(&mut new_task).await {
                        Ok(_) => {
                            local_tasks.push(new_task);
                            local_tasks.sort();
                            let _ = event_tx
                                .send(AppEvent::TasksLoaded(local_tasks.clone()))
                                .await;
                            let _ = event_tx
                                .send(AppEvent::Status("Created.".to_string()))
                                .await;
                        }
                        Err(e) => {
                            let _ = event_tx.send(AppEvent::Error(e)).await;
                        }
                    }
                }

                Action::ToggleTask(index) => {
                    if index < local_tasks.len() {
                        let task = &mut local_tasks[index];
                        task.completed = !task.completed;
                        let _ = event_tx
                            .send(AppEvent::Status("Syncing...".to_string()))
                            .await;

                        let mut task_copy = task.clone();
                        match client.update_task(&mut task_copy).await {
                            Ok(_) => {
                                local_tasks[index] = task_copy.clone();
                                local_tasks.sort();
                                let _ = event_tx
                                    .send(AppEvent::TasksLoaded(local_tasks.clone()))
                                    .await;
                                let _ =
                                    event_tx.send(AppEvent::Status("Synced.".to_string())).await;
                            }
                            Err(e) => {
                                local_tasks[index].completed = !local_tasks[index].completed;
                                let _ = event_tx
                                    .send(AppEvent::Error(format!("Sync Failed: {}", e)))
                                    .await;
                            }
                        }
                    }
                }

                Action::DeleteTask(index) => {
                    if index < local_tasks.len() {
                        let task = local_tasks[index].clone();
                        let _ = event_tx
                            .send(AppEvent::Status("Deleting...".to_string()))
                            .await;

                        match client.delete_task(&task).await {
                            Ok(_) => {
                                local_tasks.remove(index);
                                let _ = event_tx
                                    .send(AppEvent::TasksLoaded(local_tasks.clone()))
                                    .await;
                                let _ = event_tx
                                    .send(AppEvent::Status("Deleted.".to_string()))
                                    .await;
                            }
                            Err(e) => {
                                let _ = event_tx.send(AppEvent::Error(e)).await;
                            }
                        }
                    }
                }

                Action::ChangePriority(index, delta) => {
                    if index < local_tasks.len() {
                        let task = &mut local_tasks[index];

                        let new_prio = if delta > 0 {
                            match task.priority {
                                0 => 9,
                                9 => 5,
                                5 => 1,
                                1 => 1,
                                _ => 5,
                            }
                        } else {
                            match task.priority {
                                1 => 5,
                                5 => 9,
                                9 => 0,
                                0 => 0,
                                _ => 0,
                            }
                        };

                        if new_prio != task.priority {
                            task.priority = new_prio;
                            let _ = event_tx
                                .send(AppEvent::Status("Updating Prio...".to_string()))
                                .await;

                            let mut task_copy = task.clone();
                            match client.update_task(&mut task_copy).await {
                                Ok(_) => {
                                    local_tasks[index] = task_copy;
                                    local_tasks.sort();
                                    let _ = event_tx
                                        .send(AppEvent::TasksLoaded(local_tasks.clone()))
                                        .await;
                                    let _ = event_tx
                                        .send(AppEvent::Status("Updated.".to_string()))
                                        .await;
                                }
                                Err(e) => {
                                    let _ = event_tx.send(AppEvent::Error(e)).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // UI Loop
    loop {
        terminal.draw(|f| draw(f, &mut app_state))?;

        if let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::TasksLoaded(tasks) => {
                    app_state.tasks = tasks;
                    app_state.loading = false;
                    app_state.message = format!("Tasks: {}", app_state.tasks.len());
                }
                AppEvent::TaskUpdated(_) => {}
                AppEvent::Error(msg) => {
                    app_state.message = format!("Error: {}", msg);
                    app_state.loading = false;
                }
                AppEvent::Status(msg) => {
                    app_state.message = msg;
                }
            }
        }

        // 2. Process User Input
        if crossterm::event::poll(Duration::from_millis(50))? {
            let event = event::read()?; // Read event once

            match event {
                // --- MOUSE HANDLING ---
                Event::Mouse(mouse_event) => {
                    match mouse_event.kind {
                        MouseEventKind::ScrollDown => app_state.next(), // Scroll down = Next Item
                        MouseEventKind::ScrollUp => app_state.previous(), // Scroll up = Prev Item
                        _ => {}
                    }
                }

                // --- KEYBOARD HANDLING ---
                Event::Key(key) => {
                    if app_state.show_input {
                        // --- INPUT MODE ---
                        match key.code {
                            KeyCode::Enter => {
                                if !app_state.input_buffer.is_empty() {
                                    let summary = app_state.input_buffer.clone();
                                    let _ = action_tx.send(Action::CreateTask(summary)).await;
                                    app_state.input_buffer.clear();
                                    app_state.show_input = false;
                                }
                            }
                            KeyCode::Esc => {
                                app_state.show_input = false;
                                app_state.input_buffer.clear();
                            }
                            KeyCode::Char(c) => {
                                app_state.input_buffer.push(c);
                            }
                            KeyCode::Backspace => {
                                app_state.input_buffer.pop();
                            }
                            _ => {}
                        }
                    } else {
                        // --- NORMAL MODE ---
                        match key.code {
                            KeyCode::Char('q') => {
                                let _ = action_tx.send(Action::Quit).await;
                                break;
                            }
                            KeyCode::Char('a') => {
                                app_state.show_input = true;
                                app_state.message = "Example: Buy Milk @tomorrow !1".to_string();
                            }
                            // Navigation
                            KeyCode::Down | KeyCode::Char('j') => app_state.next(),
                            KeyCode::Up | KeyCode::Char('k') => app_state.previous(),

                            // NEW: Page Up / Down
                            KeyCode::PageDown => app_state.jump_forward(10), // Jump 10 items
                            KeyCode::PageUp => app_state.jump_backward(10),

                            // Actions
                            KeyCode::Char(' ') => {
                                if let Some(idx) = app_state.list_state.selected() {
                                    if idx < app_state.tasks.len() {
                                        app_state.tasks[idx].completed =
                                            !app_state.tasks[idx].completed;
                                        let _ = action_tx.send(Action::ToggleTask(idx)).await;
                                    }
                                }
                            }
                            KeyCode::Char('d') => {
                                if let Some(idx) = app_state.list_state.selected() {
                                    if idx < app_state.tasks.len() {
                                        let _ = action_tx.send(Action::DeleteTask(idx)).await;
                                    }
                                }
                            }
                            KeyCode::Char('+') => {
                                if let Some(idx) = app_state.list_state.selected() {
                                    if idx < app_state.tasks.len() {
                                        let _ =
                                            action_tx.send(Action::ChangePriority(idx, 1)).await;
                                    }
                                }
                            }
                            KeyCode::Char('-') => {
                                if let Some(idx) = app_state.list_state.selected() {
                                    if idx < app_state.tasks.len() {
                                        let _ =
                                            action_tx.send(Action::ChangePriority(idx, -1)).await;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {} // Handle Resize events etc if needed
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
