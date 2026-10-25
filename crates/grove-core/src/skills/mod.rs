//! The skill catalog and installer.
//!
//! Sources are public GitHub repositories (defaults in [`SkillCatalogConfig`]); the manifest is a
//! recursive git tree fetched through the GitHub API and cached on disk with a TTL + ETag. The
//! catalog, preview and install all fall back to that cache when the network is unavailable, so a
//! skill that has been seen once can still be installed offline.
//!
//! The optional GitHub token is read through [`crate::secrets::SecretStore`] on each request. It is
//! never stored in this module, never logged, and never written to the cache, settings or the DB.

pub mod catalog;
pub mod install;
pub mod targets;

pub use catalog::{CatalogEntry, CatalogSnapshot, SkillCatalogConfig, SkillPreview, SkillSource};
pub use install::{
    InstallRecord, InstallRequest, InstallResult, InstallStore, JsonInstallStore,
    MemoryInstallStore,
};
pub use targets::SkillTarget;

use crate::ipc::{ErrorCode, IpcError};
use crate::paths::AppPaths;
use crate::secrets::{SecretStore, GITHUB_TOKEN};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Every failure the skill subsystem reports. Converted to `IpcError` at the command boundary.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("invalid skill request: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("skill catalog unavailable: {0}")]
    Unavailable(String),
    #[error("GitHub returned {status}: {message}")]
    Http { status: u16, message: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl SkillError {
    pub fn code(&self) -> ErrorCode {
        match self {
            SkillError::Invalid(_) => ErrorCode::Invalid,
            SkillError::NotFound(_) => ErrorCode::NotFound,
            SkillError::Unavailable(_) | SkillError::Http { .. } => ErrorCode::Unavailable,
            SkillError::Io(_) => ErrorCode::Io,
        }
    }
}

impl From<SkillError> for IpcError {
    fn from(err: SkillError) -> Self {
        IpcError::new(err.code(), err.to_string())
    }
}

/// The skill subsystem as the rest of the app sees it: catalog, preview, targets, install.
pub struct SkillService {
    paths: AppPaths,
    http: reqwest::Client,
    secrets: Arc<dyn SecretStore>,
    config: RwLock<SkillCatalogConfig>,
    installs: Arc<dyn InstallStore>,
}

impl SkillService {
    pub fn new(paths: AppPaths, secrets: Arc<dyn SecretStore>) -> Self {
        Self::with_config(paths, secrets, SkillCatalogConfig::default())
    }

    pub fn with_config(
        paths: AppPaths,
        secrets: Arc<dyn SecretStore>,
        config: SkillCatalogConfig,
    ) -> Self {
        let installs = Arc::new(JsonInstallStore::new(
            paths.app_data.join("skill-installs.json"),
        ));
        Self {
            paths,
            http: reqwest::Client::builder()
                .user_agent(format!("Grove/{}", crate::VERSION))
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap_or_default(),
            secrets,
            config: RwLock::new(config),
            installs,
        }
    }

    /// Replace the catalog sources at runtime (Settings). Sources are `owner/name` GitHub repos.
    pub fn set_sources(&self, sources: Vec<SkillSource>) {
        self.config_ref().sources = sources;
    }

    pub fn config(&self) -> SkillCatalogConfig {
        self.config_snapshot()
    }

    /// Fetch the catalog. `refresh` bypasses a fresh disk cache; a failed fetch still falls back to it.
    pub async fn catalog(&self, refresh: bool) -> Result<CatalogSnapshot, SkillError> {
        catalog::catalog(self, refresh).await
    }

    /// Read one skill's `SKILL.md`, frontmatter and body.
    pub async fn preview(&self, source: &str, path: &str) -> Result<SkillPreview, SkillError> {
        catalog::preview(self, source, path).await
    }

    /// The install destinations: each provider's skills directory plus project-local `.agents/skills`.
    pub fn targets(&self, project_root: Option<&Path>) -> Vec<SkillTarget> {
        let config = self.config_snapshot();
        targets::project_targets(&targets::home_dir(config.home.as_deref()), project_root)
    }

    /// Copy a skill folder into the chosen target, atomically. Reports new vs overwritten.
    pub async fn install(&self, request: InstallRequest) -> Result<InstallResult, SkillError> {
        install::install(self, request).await
    }

    /// The recorded installs, newest last.
    pub fn installs(&self) -> Result<Vec<InstallRecord>, SkillError> {
        self.installs.list()
    }

    /// Delete one installed skill folder and forget its record.
    ///
    /// The path comes from the install log, not from the caller's imagination, and must still look
    /// like `<target>/skills/<name>`: a record that was edited by hand cannot turn this into an
    /// arbitrary recursive delete.
    pub fn remove_install(&self, path: &str) -> Result<(), SkillError> {
        let records = self.installs.list()?;
        let record = records
            .into_iter()
            .find(|record| record.path == path)
            .ok_or_else(|| SkillError::NotFound(format!("installed skill at {path}")))?;
        let folder = PathBuf::from(&record.path);
        let safe = folder
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "skills")
            && folder.file_name().is_some();
        if !safe {
            return Err(SkillError::Invalid(format!(
                "refusing to remove a non-skill path: {path}"
            )));
        }
        if folder.exists() {
            std::fs::remove_dir_all(&folder)?;
        }
        self.installs.remove(&record.path)
    }

    pub(crate) fn github_token(&self) -> Option<String> {
        match self.secrets.get(GITHUB_TOKEN) {
            Ok(token) => token.filter(|token| !token.is_empty()),
            Err(err) => {
                // The error describes the store, never the value.
                tracing::warn!(target: "grove::skills", "secret store unavailable: {err}");
                None
            }
        }
    }

    pub(crate) fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub(crate) fn config_snapshot(&self) -> SkillCatalogConfig {
        self.config_snapshot_inner()
    }

    pub(crate) fn installs_store(&self) -> &Arc<dyn InstallStore> {
        &self.installs
    }

    fn config_ref(&self) -> std::sync::RwLockWriteGuard<'_, SkillCatalogConfig> {
        self.config
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn config_snapshot_inner(&self) -> SkillCatalogConfig {
        self.config
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl fmt::Debug for SkillService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Deliberately no token: the service never holds one, but keep the capability fields only.
        f.debug_struct("SkillService")
            .field("paths", &self.paths)
            .field("config", &self.config_snapshot())
            .finish_non_exhaustive()
    }
}

/// Write `bytes` to `path` through a temporary sibling, so readers never observe a partial file.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary: PathBuf = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
    std::fs::write(&temporary, bytes)?;
    if let Err(err) = std::fs::rename(&temporary, path) {
        // Windows cannot rename over an existing file; replace it explicitly.
        if path.exists() {
            std::fs::remove_file(path)?;
            std::fs::rename(&temporary, path)?;
        } else {
            let _ = std::fs::remove_file(&temporary);
            return Err(err);
        }
    }
    Ok(())
}
