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
pub use targets::{DetectedSkill, SkillTarget};

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

    /// The install destinations: the user's global and the project-local `.agents/skills`.
    pub fn targets(&self, project_root: Option<&Path>) -> Vec<SkillTarget> {
        let config = self.config_snapshot();
        targets::project_targets(&targets::home_dir(config.home.as_deref()), project_root)
    }

    /// Skill folders already present under either target, including ones Grove did not install.
    pub fn detected(&self, project_root: Option<&Path>) -> Vec<DetectedSkill> {
        let config = self.config_snapshot();
        targets::detect_installed(&targets::home_dir(config.home.as_deref()), project_root)
    }

    /// Copy a skill folder into the chosen target, atomically. Reports new vs overwritten.
    pub async fn install(&self, request: InstallRequest) -> Result<InstallResult, SkillError> {
        install::install(self, request).await
    }

    /// The recorded installs, newest last.
    pub fn installs(&self) -> Result<Vec<InstallRecord>, SkillError> {
        self.installs.list()
    }

    /// Delete one skill folder, whether Grove installed it or it was already on disk.
    ///
    /// The folder must be a direct child of a known `.agents/skills` target and must contain a
    /// `SKILL.md`; symlinked folders are refused. A recorded path from an older target keeps the
    /// narrower `<...>/skills/<name>` rule, so legacy installs remain removable without allowing an
    /// arbitrary recursive delete. If a recorded folder is already gone, its record is cleared.
    pub fn remove_skill(&self, path: &str, project_root: Option<&Path>) -> Result<(), SkillError> {
        let folder = PathBuf::from(path);
        let parent = folder.parent().ok_or_else(|| {
            SkillError::Invalid(format!("refusing to remove a non-skill path: {path}"))
        })?;
        if folder.file_name().is_none_or(|name| name.is_empty()) {
            return Err(SkillError::Invalid(format!(
                "refusing to remove a non-skill path: {path}"
            )));
        }

        let records = self.installs.list()?;
        let recorded = records.iter().any(|record| record.path == path);

        let metadata = match std::fs::symlink_metadata(&folder) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // An external delete or a removed worktree leaves only the record. Forgetting it is
                // safe because there is nothing on disk to remove.
                if recorded {
                    self.installs.remove(path)?;
                    return Ok(());
                }
                return Err(SkillError::NotFound(format!("skill folder at {path}")));
            }
            Err(error) => return Err(SkillError::Io(error)),
        };
        if metadata.file_type().is_symlink() {
            return Err(SkillError::Invalid(format!(
                "refusing to remove a symlinked skill: {path}"
            )));
        }
        if !metadata.is_dir() || !folder.join("SKILL.md").is_file() {
            return Err(SkillError::Invalid(format!(
                "refusing to remove a folder that is not a skill: {path}"
            )));
        }

        let config = self.config_snapshot();
        let targets =
            targets::target_dirs(&targets::home_dir(config.home.as_deref()), project_root);
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|_| SkillError::NotFound(format!("skill folder at {path}")))?;
        let in_target = targets.iter().any(|target| {
            std::fs::canonicalize(&target.path)
                .map(|root| root == canonical_parent)
                .unwrap_or(false)
        });

        if !in_target {
            let legacy = recorded
                && parent
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == "skills");
            if !legacy {
                return Err(SkillError::Invalid(format!(
                    "refusing to remove a skill outside a known target: {path}"
                )));
            }
        }

        remove_tree(&folder).map_err(|error| {
            SkillError::Io(std::io::Error::new(
                error.kind(),
                format!("remove {}: {error}", folder.display()),
            ))
        })?;
        if recorded {
            self.installs.remove(path)?;
        }
        Ok(())
    }

    pub(crate) fn github_token(&self) -> Option<String> {
        match self.secrets.get(GITHUB_TOKEN) {
            Ok(Some(token)) if !token.trim().is_empty() => return Some(token),
            Ok(_) => {}
            Err(err) => {
                // The error describes the store, never the value.
                tracing::warn!(target: "grove::skills", "secret store unavailable: {err}");
            }
        }

        // The user may already be signed in through the environment or the GitHub CLI. Both are
        // read-only fallbacks; Grove never writes a discovered token back to disk.
        env_github_token().or_else(gh_auth_token)
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

/// Delete a directory tree, clearing Windows read-only attributes if the first attempt is denied.
fn remove_tree(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                clear_readonly(path)?;
                std::fs::remove_dir_all(path)
            }
            Err(error) => Err(error),
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_dir_all(path)
    }
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn clear_readonly(path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            clear_readonly(&entry?.path())?;
        }
    }
    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

/// A GitHub token already present in the environment, without writing it anywhere.
fn env_github_token() -> Option<String> {
    for key in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Ask the GitHub CLI for the token it already holds. `gh` is an official client and its token is
/// already scoped to whatever the user signed in for; a missing CLI is not an error.
fn gh_auth_token() -> Option<String> {
    let mut command = std::process::Command::new("gh");
    command
        .args(["auth", "token"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::process::hide_console(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
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
