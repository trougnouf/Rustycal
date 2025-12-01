use crate::model::Task;
use crate::storage::LocalStorage; // Import helper
use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Action {
    Create(Task),
    Update(Task),
    Delete(Task),
    // Add Move action for atomic moves on server
    Move(Task, String), // Task, New Calendar Href
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Journal {
    pub queue: Vec<Action>,
}

impl Journal {
    pub fn get_path() -> Option<PathBuf> {
        if let Some(proj) = ProjectDirs::from("com", "cfait", "cfait") {
            let data_dir = proj.data_dir();
            if !data_dir.exists() {
                let _ = fs::create_dir_all(data_dir);
            }
            return Some(data_dir.join("journal.json"));
        }
        None
    }

    pub fn load() -> Self {
        if let Some(path) = Self::get_path()
            && path.exists()
            && let Ok(content) = fs::read_to_string(path)
            && let Ok(journal) = serde_json::from_str(&content)
        {
            return journal;
        }
        Self::default()
    }

    pub fn save(&self) -> Result<()> {
        if let Some(path) = Self::get_path() {
            let json = serde_json::to_string_pretty(self)?;
            LocalStorage::atomic_write(path, json)?;
        }
        Ok(())
    }

    pub fn push(action: Action) -> Result<()> {
        let mut journal = Self::load();
        journal.queue.push(action);
        journal.save()
    }

    pub fn pop_front(&mut self) -> Result<()> {
        if !self.queue.is_empty() {
            self.queue.remove(0);
            self.save()?;
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    // Insert at front (used for conflict resolution retries)
    pub fn push_front(&mut self, action: Action) -> Result<()> {
        self.queue.insert(0, action);
        self.save()
    }
}
