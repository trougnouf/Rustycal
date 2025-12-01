// File: ./src/storage.rs
use crate::model::Task;
use anyhow::Result;
use directories::ProjectDirs;
use fs2::FileExt;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

// Constants for identification
pub const LOCAL_CALENDAR_HREF: &str = "local://default";
pub const LOCAL_CALENDAR_NAME: &str = "Local";

pub struct LocalStorage;

impl LocalStorage {
    pub fn get_path() -> Option<PathBuf> {
        if let Ok(test_dir) = env::var("CFAIT_TEST_DIR") {
            let path = PathBuf::from(test_dir);
            if !path.exists() {
                let _ = fs::create_dir_all(&path);
            }
            return Some(path.join("local.json"));
        }

        if let Some(proj) = ProjectDirs::from("com", "trougnouf", "cfait") {
            let data_dir = proj.data_dir();
            if !data_dir.exists() {
                let _ = fs::create_dir_all(data_dir);
            }
            return Some(data_dir.join("local.json"));
        }
        None
    }

    /// Helper to get a sidecar lock file path (e.g., "local.json.lock")
    fn get_lock_path(file_path: &Path) -> PathBuf {
        let mut lock_path = file_path.to_path_buf();
        if let Some(ext) = lock_path.extension() {
            let mut new_ext = ext.to_os_string();
            new_ext.push(".lock");
            lock_path.set_extension(new_ext);
        } else {
            lock_path.set_extension("lock");
        }
        lock_path
    }

    /// execute a closure while holding an exclusive lock on the sidecar file.
    pub fn with_lock<F, T>(file_path: &Path, f: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let lock_path = Self::get_lock_path(file_path);

        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        // Block until lock is acquired
        file.lock_exclusive()?;

        let result = f();

        file.unlock()?;

        result
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
            Self::with_lock(&path, || {
                let json = serde_json::to_string_pretty(tasks)?;
                Self::atomic_write(&path, json)?;
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn load() -> Result<Vec<Task>> {
        if let Some(path) = Self::get_path()
            && path.exists()
        {
            return Self::with_lock(&path, || {
                if let Ok(json) = fs::read_to_string(&path)
                    && let Ok(tasks) = serde_json::from_str::<Vec<Task>>(&json)
                {
                    Ok(tasks)
                } else {
                    Ok(vec![])
                }
            });
        }
        Ok(vec![])
    }
}
