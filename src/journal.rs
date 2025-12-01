// File: ./src/journal.rs
use crate::model::Task;
use crate::storage::LocalStorage;
use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Action {
    Create(Task),
    Update(Task),
    Delete(Task),
    Move(Task, String),
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Journal {
    pub queue: Vec<Action>,
}

impl Journal {
    pub fn get_path() -> Option<PathBuf> {
        // ISOLATION: Check env var first
        if let Ok(test_dir) = env::var("CFAIT_TEST_DIR") {
            let path = PathBuf::from(test_dir);
            if !path.exists() {
                let _ = fs::create_dir_all(&path);
            }
            return Some(path.join("journal.json"));
        }

        if let Some(proj) = ProjectDirs::from("com", "cfait", "cfait") {
            let data_dir = proj.data_dir();
            if !data_dir.exists() {
                let _ = fs::create_dir_all(data_dir);
            }
            return Some(data_dir.join("journal.json"));
        }
        None
    }

    /// Internal load helper (no locking)
    fn load_internal(path: &PathBuf) -> Self {
        if path.exists()
            && let Ok(content) = fs::read_to_string(path)
            && let Ok(journal) = serde_json::from_str(&content)
        {
            return journal;
        }
        Self::default()
    }

    /// Public load with locking
    pub fn load() -> Self {
        if let Some(path) = Self::get_path() {
            if !path.exists() {
                return Self::default();
            }
            return LocalStorage::with_lock(&path, || Ok(Self::load_internal(&path)))
                .unwrap_or_default();
        }
        Self::default()
    }

    /// Public save with locking
    pub fn save(&self) -> Result<()> {
        if let Some(path) = Self::get_path() {
            LocalStorage::with_lock(&path, || {
                let json = serde_json::to_string_pretty(self)?;
                LocalStorage::atomic_write(&path, json)?;
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Atomic Push using modify transaction
    pub fn push(action: Action) -> Result<()> {
        Self::modify(|queue| queue.push(action))
    }

    /// Atomic Push Front using modify transaction
    pub fn push_front(&mut self, action: Action) -> Result<()> {
        let res = Self::modify(|queue| queue.insert(0, action));
        // Reload self to keep in sync if needed by legacy code, though sync_journal
        // now reloads explicitly.
        if res.is_ok() {
            *self = Self::load();
        }
        res
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Transactional modification of the journal queue.
    /// Locks -> Loads -> Applies Closure -> Saves -> Unlocks.
    pub fn modify<F>(f: F) -> Result<()>
    where
        F: FnOnce(&mut Vec<Action>),
    {
        if let Some(path) = Self::get_path() {
            LocalStorage::with_lock(&path, || {
                let mut journal = Self::load_internal(&path);
                f(&mut journal.queue);
                let json = serde_json::to_string_pretty(&journal)?;
                LocalStorage::atomic_write(&path, json)?;
                Ok(())
            })?;
        }
        Ok(())
    }
}
