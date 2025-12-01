use crate::model::Task;
use anyhow::Result;
use directories::ProjectDirs;
use std::fs;
use std::path::{Path, PathBuf};

// Constants for identification
pub const LOCAL_CALENDAR_HREF: &str = "local://default";
pub const LOCAL_CALENDAR_NAME: &str = "Local";

pub struct LocalStorage;

impl LocalStorage {
    fn get_path() -> Option<PathBuf> {
        if let Some(proj) = ProjectDirs::from("com", "trougnouf", "cfait") {
            let data_dir = proj.data_dir();
            if !data_dir.exists() {
                let _ = fs::create_dir_all(data_dir);
            }
            return Some(data_dir.join("local.json"));
        }
        None
    }

    /// Atomic write: Write to .tmp file then rename
    pub fn atomic_write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> Result<()> {
        let path = path.as_ref();
        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, contents)?;
        fs::rename(tmp_path, path)?;
        Ok(())
    }

    pub fn save(tasks: &[Task]) -> Result<()> {
        if let Some(path) = Self::get_path() {
            let json = serde_json::to_string_pretty(tasks)?;
            Self::atomic_write(path, json)?;
        }
        Ok(())
    }

    pub fn load() -> Result<Vec<Task>> {
        if let Some(path) = Self::get_path()
            && path.exists()
        {
            // If the file exists but is empty/corrupt, ignore error and return empty vec
            if let Ok(json) = fs::read_to_string(path)
                && let Ok(tasks) = serde_json::from_str::<Vec<Task>>(&json)
            {
                return Ok(tasks);
            }
        }
        Ok(vec![])
    }
}
