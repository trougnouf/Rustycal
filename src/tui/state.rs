use crate::model::{CalendarListEntry, Task};
use ratatui::widgets::ListState;

#[derive(PartialEq, Clone, Copy)]
pub enum Focus {
    Sidebar,
    Main,
}

#[derive(PartialEq, Clone, Copy)]
pub enum InputMode {
    Normal,
    Creating,
    Searching,
    Editing,
    EditingDescription,
}

pub struct AppState {
    pub tasks: Vec<Task>,
    pub view_indices: Vec<usize>,
    pub calendars: Vec<CalendarListEntry>,
    pub list_state: ListState,
    pub cal_state: ListState,
    pub active_focus: Focus,
    pub message: String,
    pub loading: bool,
    pub mode: InputMode,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub editing_index: Option<usize>,
}

impl AppState {
    pub fn new() -> Self {
        let mut l_state = ListState::default();
        l_state.select(Some(0));
        let mut c_state = ListState::default();
        c_state.select(Some(0));
        Self {
            tasks: vec![],
            view_indices: vec![],
            calendars: vec![],
            list_state: l_state,
            cal_state: c_state,
            active_focus: Focus::Main,
            message: "Tab: View | /: Search | a: Add | e: Edit".to_string(),
            loading: true,
            mode: InputMode::Normal,
            input_buffer: String::new(),
            cursor_position: 0,
            editing_index: None,
        }
    }

    pub fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.cursor_position.saturating_sub(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_left);
    }
    pub fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.cursor_position.saturating_add(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_right);
    }
    pub fn enter_char(&mut self, new_char: char) {
        self.input_buffer.insert(self.cursor_position, new_char);
        self.move_cursor_right();
    }
    pub fn delete_char(&mut self) {
        if self.cursor_position != 0 {
            let current_index = self.cursor_position;
            let from_left_to_current_index = current_index - 1;
            let before_char_to_delete = self.input_buffer.chars().take(from_left_to_current_index);
            let after_char_to_delete = self.input_buffer.chars().skip(current_index);
            self.input_buffer = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }
    pub fn reset_input(&mut self) {
        self.input_buffer.clear();
        self.cursor_position = 0;
    }
    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input_buffer.chars().count())
    }
    pub fn recalculate_view(&mut self) {
        if self.mode == InputMode::Searching && !self.input_buffer.is_empty() {
            let query = self.input_buffer.to_lowercase();
            self.view_indices = self
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.summary.to_lowercase().contains(&query))
                .map(|(i, _)| i)
                .collect();
        } else {
            self.view_indices = (0..self.tasks.len()).collect();
        }
        let sel = self.list_state.selected().unwrap_or(0);
        if self.view_indices.is_empty() {
            self.list_state.select(Some(0));
        } else if sel >= self.view_indices.len() {
            self.list_state.select(Some(self.view_indices.len() - 1));
        }
    }
    pub fn get_selected_master_index(&self) -> Option<usize> {
        if let Some(view_idx) = self.list_state.selected() {
            if view_idx < self.view_indices.len() {
                return Some(self.view_indices[view_idx]);
            }
        }
        None
    }
    pub fn next(&mut self) {
        match self.active_focus {
            Focus::Main => {
                let len = self.view_indices.len();
                if len == 0 {
                    return;
                }
                let i = match self.list_state.selected() {
                    Some(i) => {
                        if i >= len - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            Focus::Sidebar => {
                let len = self.calendars.len();
                if len == 0 {
                    return;
                }
                let i = match self.cal_state.selected() {
                    Some(i) => {
                        if i >= len - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.cal_state.select(Some(i));
            }
        }
    }
    pub fn previous(&mut self) {
        match self.active_focus {
            Focus::Main => {
                let len = self.view_indices.len();
                if len == 0 {
                    return;
                }
                let i = match self.list_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            len - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            Focus::Sidebar => {
                let len = self.calendars.len();
                if len == 0 {
                    return;
                }
                let i = match self.cal_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            len - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.cal_state.select(Some(i));
            }
        }
    }
    pub fn jump_forward(&mut self, step: usize) {
        match self.active_focus {
            Focus::Main => {
                if self.view_indices.is_empty() {
                    return;
                }
                let current = self.list_state.selected().unwrap_or(0);
                let new_index = (current + step).min(self.view_indices.len() - 1);
                self.list_state.select(Some(new_index));
            }
            Focus::Sidebar => {
                if self.calendars.is_empty() {
                    return;
                }
                let current = self.cal_state.selected().unwrap_or(0);
                let new_index = (current + step).min(self.calendars.len() - 1);
                self.cal_state.select(Some(new_index));
            }
        }
    }
    pub fn jump_backward(&mut self, step: usize) {
        match self.active_focus {
            Focus::Main => {
                if self.view_indices.is_empty() {
                    return;
                }
                let current = self.list_state.selected().unwrap_or(0);
                let new_index = current.saturating_sub(step);
                self.list_state.select(Some(new_index));
            }
            Focus::Sidebar => {
                if self.calendars.is_empty() {
                    return;
                }
                let current = self.cal_state.selected().unwrap_or(0);
                let new_index = current.saturating_sub(step);
                self.cal_state.select(Some(new_index));
            }
        }
    }
    pub fn toggle_focus(&mut self) {
        self.active_focus = match self.active_focus {
            Focus::Main => Focus::Sidebar,
            Focus::Sidebar => Focus::Main,
        }
    }
}
