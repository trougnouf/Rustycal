// File: ./src/cache.rs
use crate::model::{CalendarListEntry, Task};
use crate::storage::LocalStorage;
use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct CalendarCache {
    sync_token: Option<String>,
    tasks: Vec<Task>,
}

pub struct Cache;

impl Cache {
    fn get_base_dir() -> Option<PathBuf> {
        if let Ok(test_dir) = env::var("CFAIT_TEST_DIR") {
            let path = PathBuf::from(test_dir);
            if !path.exists() {
                let _ = fs::create_dir_all(&path);
            }
            return Some(path);
        }

        if let Some(proj) = ProjectDirs::from("com", "cfait", "cfait") {
            let cache_dir = proj.cache_dir();
            if !cache_dir.exists() {
                let _ = fs::create_dir_all(cache_dir);
            }
            return Some(cache_dir.to_path_buf());
        }
        None
    }

    fn get_calendars_path() -> Option<PathBuf> {
        Self::get_base_dir().map(|p| p.join("calendars.json"))
    }

    fn get_path(key: &str) -> Option<PathBuf> {
        Self::get_base_dir().map(|dir| {
            let mut hasher = DefaultHasher::new();
            key.hash(&mut hasher);
            let filename = format!("tasks_{:x}.json", hasher.finish());
            dir.join(filename)
        })
    }

    pub fn save(key: &str, tasks: &[Task], sync_token: Option<String>) -> Result<()> {
        if let Some(path) = Self::get_path(key) {
            LocalStorage::with_lock(&path, || {
                let data = CalendarCache {
                    sync_token: sync_token.clone(),
                    tasks: tasks.to_vec(),
                };
                let json = serde_json::to_string_pretty(&data)?;
                LocalStorage::atomic_write(&path, json)?;
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn load(key: &str) -> Result<(Vec<Task>, Option<String>)> {
        if let Some(path) = Self::get_path(key)
            && path.exists()
        {
            return LocalStorage::with_lock(&path, || {
                let json = fs::read_to_string(&path)?;
                if let Ok(cache) = serde_json::from_str::<CalendarCache>(&json) {
                    return Ok((cache.tasks, cache.sync_token));
                }
                if let Ok(tasks) = serde_json::from_str::<Vec<Task>>(&json) {
                    return Ok((tasks, None));
                }
                Ok((vec![], None))
            });
        }
        Ok((vec![], None))
    }

    pub fn save_calendars(cals: &[CalendarListEntry]) -> Result<()> {
        if let Some(path) = Self::get_calendars_path() {
            LocalStorage::with_lock(&path, || {
                let json = serde_json::to_string_pretty(cals)?;
                LocalStorage::atomic_write(&path, json)?;
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn load_calendars() -> Result<Vec<CalendarListEntry>> {
        if let Some(path) = Self::get_calendars_path()
            && path.exists()
        {
            return LocalStorage::with_lock(&path, || {
                let json = fs::read_to_string(&path)?;
                let cals: Vec<CalendarListEntry> = serde_json::from_str(&json)?;
                Ok(cals)
            });
        }
        Ok(vec![])
    }
}
