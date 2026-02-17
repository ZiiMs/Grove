use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::agent::Agent;
use crate::app::Config;

/// Persisted session data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub repo_path: String,
    pub agents: Vec<Agent>,
    pub selected_index: usize,
}

impl SessionData {
    pub fn new(repo_path: String) -> Self {
        Self {
            repo_path,
            agents: Vec::new(),
            selected_index: 0,
        }
    }
}

/// Session storage manager.
pub struct SessionStorage {
    session_path: PathBuf,
}

impl SessionStorage {
    pub fn new(repo_path: &str) -> Result<Self> {
        let config_dir = Config::ensure_config_dir()?;

        // Create a unique session file based on repo path hash
        let hash = Self::hash_path(repo_path);
        let session_path = config_dir.join(format!("session-{}.json", hash));

        Ok(Self { session_path })
    }

    fn hash_path(path: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Load session data from disk.
    pub fn load(&self) -> Result<Option<SessionData>> {
        if !self.session_path.exists() {
            return Ok(None);
        }

        let content =
            std::fs::read_to_string(&self.session_path).context("Failed to read session file")?;

        let session: SessionData =
            serde_json::from_str(&content).context("Failed to parse session file")?;

        Ok(Some(session))
    }

    /// Save session data to disk.
    pub fn save(&self, session: &SessionData) -> Result<()> {
        let content =
            serde_json::to_string_pretty(session).context("Failed to serialize session")?;

        std::fs::write(&self.session_path, content).context("Failed to write session file")?;

        Ok(())
    }

    /// Delete session file.
    pub fn delete(&self) -> Result<()> {
        if self.session_path.exists() {
            std::fs::remove_file(&self.session_path).context("Failed to delete session file")?;
        }
        Ok(())
    }

    /// Get the path to the session file.
    pub fn path(&self) -> &PathBuf {
        &self.session_path
    }
}

/// Save current app state to session.
pub fn save_session(
    storage: &SessionStorage,
    repo_path: &str,
    agents: &[Agent],
    selected_index: usize,
) -> Result<()> {
    let session = SessionData {
        repo_path: repo_path.to_string(),
        agents: agents.to_vec(),
        selected_index,
    };

    storage.save(&session)
}

/// Load session and restore app state.
pub fn load_session(storage: &SessionStorage) -> Result<Option<SessionData>> {
    storage.load()
}
