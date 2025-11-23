use crate::model::{CalendarListEntry, Task};

#[derive(Debug)]
pub enum Action {
    SwitchCalendar(String),
    ToggleTask(usize),
    CreateTask(String),
    EditTask(usize, String),
    EditDescription(usize, String),
    DeleteTask(usize),
    ChangePriority(usize, i8),
    IndentTask(usize),
    OutdentTask(usize),
    Quit,
}

#[derive(Debug)]
pub enum AppEvent {
    CalendarsLoaded(Vec<CalendarListEntry>),
    TasksLoaded(Vec<Task>),
    #[allow(dead_code)]
    TaskUpdated(Task),
    Error(String),
    Status(String),
}
