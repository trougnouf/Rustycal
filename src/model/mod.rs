// File: ./src/model/mod.rs
// Aggregates the split model files
pub mod adapter;
pub mod item;
pub mod parser;

// Re-export types so existing code using `crate::model::Task` still works
pub use item::{CalendarListEntry, Task, TaskStatus};
