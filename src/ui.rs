use crate::model::Task;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub struct AppState {
    pub tasks: Vec<Task>,
    pub list_state: ListState,
    pub message: String,
    pub loading: bool,
    pub show_input: bool,
    pub input_buffer: String,
}

impl AppState {
    pub fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            tasks: vec![],
            list_state: state,
            message: "Ready.".to_string(), // Initial status
            loading: true,
            show_input: false,
            input_buffer: String::new(),
        }
    }

    pub fn next(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.tasks.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.tasks.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn jump_forward(&mut self, step: usize) {
        if self.tasks.is_empty() {
            return;
        }

        let current = self.list_state.selected().unwrap_or(0);
        // Clamp to the last item (don't wrap around like next())
        let new_index = (current + step).min(self.tasks.len() - 1);

        self.list_state.select(Some(new_index));
    }

    pub fn jump_backward(&mut self, step: usize) {
        if self.tasks.is_empty() {
            return;
        }

        let current = self.list_state.selected().unwrap_or(0);
        // Clamp to 0 (don't wrap around)
        let new_index = current.saturating_sub(step);

        self.list_state.select(Some(new_index));
    }
}

pub fn draw(f: &mut Frame, state: &mut AppState) {
    // 1. Layout: Main Body (Top) vs Footer (Bottom 3 lines)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
        .split(f.area());

    // 2. Render Task List
    let items: Vec<ListItem> = state
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

            let summary = format!("{} {}{}", checkbox, t.summary, due_str);
            ListItem::new(Line::from(vec![Span::styled(summary, style)]))
        })
        .collect();

    let title = if state.loading {
        " Tasks (Loading...) "
    } else {
        " Tasks "
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        );

    f.render_stateful_widget(list, chunks[0], &mut state.list_state);

    // 3. Render Footer
    if state.show_input {
        // MODE: INPUT
        // Takes up the full footer width
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Create New Task ");
        let input_text = Paragraph::new(format!("> {}_", state.input_buffer))
            .style(Style::default().fg(Color::Yellow))
            .block(input_block);
        f.render_widget(input_text, chunks[1]);
    } else {
        // MODE: VIEW
        // Split footer into Left (Status) and Right (Shortcuts)
        let footer_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        // Left: Status Message
        let status_color = if state.message.contains("Error") {
            Color::Red
        } else {
            Color::Cyan
        };
        let status = Paragraph::new(state.message.clone())
            .style(Style::default().fg(status_color))
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::TOP | Borders::BOTTOM)
                    .title(" Status "),
            );

        // inside draw(...)
        let shortcuts = "a: Add | d: Del | +/-: Prio | Space: Done | q: Quit";
        let help = Paragraph::new(shortcuts)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Right)
            .block(
                Block::default()
                    .borders(Borders::RIGHT | Borders::TOP | Borders::BOTTOM)
                    .title(" Actions "),
            );

        f.render_widget(status, footer_chunks[0]);
        f.render_widget(help, footer_chunks[1]);
    }
}
