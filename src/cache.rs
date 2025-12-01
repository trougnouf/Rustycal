use crate::model::{CalendarListEntry, Task};
use crate::storage::LocalStorage; // Import helper
use anyhow::Result;
use directories::ProjectDirs;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

pub struct Cache;

impl Cache {
    fn get_calendars_path() -> Option<PathBuf> {
        if let Some(proj) = ProjectDirs::from("com", "cfait", "cfait") {
            let cache_dir = proj.cache_dir();
            if !cache_dir.exists() {
                let _ = fs::create_dir_all(cache_dir);
            }
            return Some(cache_dir.join("calendars.json"));
        }
        None
    }

    fn get_path(key: &str) -> Option<PathBuf> {
        if let Some(proj) = ProjectDirs::from("com", "cfait", "cfait") {
            let cache_dir = proj.cache_dir();
            if !cache_dir.exists() {
                let _ = fs::create_dir_all(cache_dir);
            }

            let mut hasher = DefaultHasher::new();
            key.hash(&mut hasher);
            let filename = format!("tasks_{:x}.json", hasher.finish());

            return Some(cache_dir.join(filename));
        }
        None
    }

    pub fn save(key: &str, tasks: &[Task]) -> Result<()> {
        if let Some(path) = Self::get_path(key) {
            let json = serde_json::to_string_pretty(tasks)?;
            LocalStorage::atomic_write(path, json)?;
        }
        Ok(())
    }

    pub fn load(key: &str) -> Result<Vec<Task>> {
        if let Some(path) = Self::get_path(key)
            && path.exists()
        {
            let json = fs::read_to_string(path)?;
            let tasks: Vec<Task> = serde_json::from_str(&json)?;
            return Ok(tasks);
        }
        Ok(vec![])
    }

    pub fn save_calendars(cals: &[CalendarListEntry]) -> Result<()> {
        if let Some(path) = Self::get_calendars_path() {
            let json = serde_json::to_string_pretty(cals)?;
            LocalStorage::atomic_write(path, json)?;
        }
        Ok(())
    }

    pub fn load_calendars() -> Result<Vec<CalendarListEntry>> {
        if let Some(path) = Self::get_calendars_path()
            && path.exists()
        {
            let json = fs::read_to_string(path)?;
            let cals: Vec<CalendarListEntry> = serde_json::from_str(&json)?;
            return Ok(cals);
        }
        Ok(vec![])
    }
}
