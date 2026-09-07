pub mod counterexample;

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub use counterexample::{CounterexampleArtifact, CounterexampleManifest};

pub struct StorageManager {
    base_dir: PathBuf,
}

impl StorageManager {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            base_dir: root.as_ref().join(".faultline"),
        }
    }

    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(self.sessions_dir())?;
        fs::create_dir_all(self.experiments_dir())?;
        fs::create_dir_all(self.counterexamples_dir())?;
        Ok(())
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.base_dir.join("sessions")
    }

    pub fn experiments_dir(&self) -> PathBuf {
        self.base_dir.join("experiments")
    }

    pub fn counterexamples_dir(&self) -> PathBuf {
        self.base_dir.join("counterexamples")
    }

    pub fn save_json<T: Serialize>(&self, path: &Path, data: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(data)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_json<T: for<'de> Deserialize<'de>>(&self, path: &Path) -> Result<T> {
        let content = fs::read_to_string(path)?;
        let data = serde_json::from_str(&content)?;
        Ok(data)
    }
}
