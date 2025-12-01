use crate::model::{CalendarListEntry, Task};
use crate::storage::LocalStorage;
use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

// Wrapper struct to store tasks + metadata
#[derive(Serialize, Deserialize)]
struct CalendarCache {
    sync_token: Option<String>,
    tasks: Vec<Task>,
}

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

    // Save now accepts an optional sync_token
    pub fn save(key: &str, tasks: &[Task], sync_token: Option<String>) -> Result<()> {
        if let Some(path) = Self::get_path(key) {
            let data = CalendarCache {
                sync_token,
                tasks: tasks.to_vec(),
            };
            let json = serde_json::to_string_pretty(&data)?;
            LocalStorage::atomic_write(path, json)?;
        }
        Ok(())
    }

    // Load now returns (Vec<Task>, Option<String>)
    pub fn load(key: &str) -> Result<(Vec<Task>, Option<String>)> {
        if let Some(path) = Self::get_path(key)
            && path.exists()
        {
            let json = fs::read_to_string(path)?;
            // Try loading new format
            if let Ok(cache) = serde_json::from_str::<CalendarCache>(&json) {
                return Ok((cache.tasks, cache.sync_token));
            }
            // Fallback: Try loading old format (raw Vec<Task>) for backward compatibility
            if let Ok(tasks) = serde_json::from_str::<Vec<Task>>(&json) {
                return Ok((tasks, None));
            }
        }
        Ok((vec![], None))
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
