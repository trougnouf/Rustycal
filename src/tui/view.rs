use crate::tui::state::{AppState, Focus, InputMode};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
        .split(f.area());

    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(v_chunks[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(h_chunks[1]);

    // --- Sidebar ---
    let cal_items: Vec<ListItem> = state
        .calendars
        .iter()
        .map(|c| ListItem::new(Line::from(c.name.as_str())))
        .collect();
    let sidebar_style = if state.active_focus == Focus::Sidebar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let sidebar = List::new(cal_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Calendars ")
                .border_style(sidebar_style),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Blue),
        );
    f.render_stateful_widget(sidebar, h_chunks[0], &mut state.cal_state);

    // --- Task List ---
    let task_items: Vec<ListItem> = state
        .view_indices
        .iter()
        .map(|&idx| {
            let t = &state.tasks[idx];
            let style = match t.priority {
                1..=4 => Style::default().fg(Color::Red),
                5 => Style::default().fg(Color::Yellow),
                _ => Style::default().fg(Color::White),
            };
            let checkbox = if t.completed { "[x]" } else { "[ ]" };
            let due_str = match t.due {
                Some(d) => format!(" ({})", d.format("%d/%m")),
                None => "".to_string(),
            };
            let indent = "  ".repeat(t.depth);
            let recur_str = if t.rrule.is_some() { " (R)" } else { "" };

            // Show categories in TUI
            let mut cat_str = String::new();
            for cat in &t.categories {
                cat_str.push_str(&format!(" #{}", cat));
            }

            let summary = format!(
                "{}{}{} {}{}{}",
                indent, checkbox, t.summary, due_str, recur_str, cat_str
            );
            ListItem::new(Line::from(vec![Span::styled(summary, style)]))
        })
        .collect();

    let main_style = if state.active_focus == Focus::Main {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let title = if state.loading {
        " Tasks (Loading...) ".to_string()
    } else {
        format!(" Tasks ({}) ", state.view_indices.len())
    };
    let task_list = List::new(task_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(main_style),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        );
    f.render_stateful_widget(task_list, main_chunks[0], &mut state.list_state);

    // --- Details Pane ---
    let details_text = if let Some(idx) = state.get_selected_master_index() {
        let task = &state.tasks[idx];
        if task.description.is_empty() {
            "No description.".to_string()
        } else {
            task.description.clone()
        }
    } else {
        "".to_string()
    };

    let details = Paragraph::new(details_text)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title(" Details "));
    f.render_widget(details, main_chunks[1]);

    // --- Footer / Input ---
    let footer_area = v_chunks[1];
    match state.mode {
        InputMode::Creating
        | InputMode::Editing
        | InputMode::Searching
        | InputMode::EditingDescription => {
            let (title, prefix, color) = match state.mode {
                InputMode::Searching => (" Search ", "/ ", Color::Green),
                InputMode::Editing => (" Edit Title ", "> ", Color::Magenta),
                InputMode::EditingDescription => (" Edit Description ", "📝 ", Color::Blue),
                _ => (" Create Task ", "> ", Color::Yellow),
            };
            let input = Paragraph::new(format!("{}{}", prefix, state.input_buffer))
                .style(Style::default().fg(color))
                .block(Block::default().borders(Borders::ALL).title(title));
            f.render_widget(input, footer_area);
            let cursor_x =
                footer_area.x + 1 + prefix.chars().count() as u16 + state.cursor_position as u16;
            let cursor_y = footer_area.y + 1;
            f.set_cursor_position((cursor_x, cursor_y));
        }
        InputMode::Normal => {
            let f_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(v_chunks[1]);
            let status = Paragraph::new(state.message.clone())
                .style(Style::default().fg(Color::Cyan))
                .block(
                    Block::default()
                        .borders(Borders::LEFT | Borders::TOP | Borders::BOTTOM)
                        .title(" Status "),
                );
            let help_text = "Tab:View | /:Find | a:Add | e:Title | E:Desc | d:Del";
            let help = Paragraph::new(help_text)
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Right)
                .block(
                    Block::default()
                        .borders(Borders::RIGHT | Borders::TOP | Borders::BOTTOM)
                        .title(" Actions "),
                );
            f.render_widget(status, f_chunks[0]);
            f.render_widget(help, f_chunks[1]);
        }
    }
}
