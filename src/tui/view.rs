use crate::tui::action::SidebarMode;
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
    let sidebar_style = if state.active_focus == Focus::Sidebar {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let (sidebar_title, sidebar_items) = match state.sidebar_mode {
        SidebarMode::Calendars => {
            let items: Vec<ListItem> = state
                .calendars
                .iter()
                .map(|c| {
                    let prefix = if Some(&c.href) == state.active_cal_href.as_ref() {
                        "* "
                    } else {
                        "  "
                    };
                    ListItem::new(Line::from(format!("{}{}", prefix, c.name)))
                })
                .collect();
            (" Calendars [1] ".to_string(), items) // Return String
        }
        SidebarMode::Categories => {
            let should_hide = state.hide_completed || state.hide_completed_in_tags;
            let all_cats = state.store.get_all_categories(should_hide);

            let items: Vec<ListItem> = all_cats
                .iter()
                .map(|c| {
                    let selected = if state.selected_categories.contains(c) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    ListItem::new(Line::from(format!("{} {}", selected, c)))
                })
                .collect();
            let logic = if state.match_all_categories {
                "AND"
            } else {
                "OR"
            };
            (format!(" Tags [2] ({}) ", logic), items) // Return String
        }
    };

    let sidebar = List::new(sidebar_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(sidebar_title)
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
        .tasks
        .iter()
        .map(|t| {
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

            // Logic: Only indent if in Calendar Mode (and not searching)
            // Consistent with GUI
            let show_indent = state.active_cal_href.is_some() && state.mode != InputMode::Searching;
            let indent = if show_indent {
                "  ".repeat(t.depth)
            } else {
                "".to_string()
            };

            let recur_str = if t.rrule.is_some() { " (R)" } else { "" };

            let mut cat_str = String::new();
            if !t.categories.is_empty() {
                cat_str = format!(" [{}]", t.categories.join(", "));
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
        format!(" Tasks ({}) ", state.tasks.len())
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

    // --- Details ---
    let details_text = if let Some(task) = state.get_selected_task() {
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

    // --- Footer ---
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

            // Dynamic Help Text
            let help_str = match state.active_focus {
                Focus::Sidebar => match state.sidebar_mode {
                    SidebarMode::Calendars => "Enter:Select | 2:Tags",
                    SidebarMode::Categories => "Enter:Toggle | m:Match(AND/OR) | 1:Cals",
                },
                Focus::Main => "/:Find | a:Add | e:Title | E:Desc | d:Del | H:Hide",
            };

            let help = Paragraph::new(help_str)
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
