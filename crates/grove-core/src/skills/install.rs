//! Skill installation: download the folder, stage it beside the destination, then rename it into
//! place. Traversal, absolute paths and symlink escapes are refused before anything is written, and
//! every install is recorded so the UI can show what went where.

use super::catalog::{checked_relative, find_source, manifest_for, read_blob};
use super::targets::{home_dir, target_dirs};
use super::{SkillError, SkillService};
use crate::paths::display_path;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

/// What to install and where. `project_root` resolves the project-local `.agents/skills` target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRequest {
    pub source: String,
    pub path: String,
    pub target: String,
    pub project_root: Option<PathBuf>,
}

/// Result of one install. Pinned by `contract/fixtures/skill-install.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub skill: String,
    pub source: String,
    /// Target id the user chose: `claude` | `codex` | `pi` | `opencode` | `agents`.
    pub target: String,
    /// Forward-slash path of the installed folder.
    pub path: String,
    /// False when the skill was new, true when an existing folder was replaced.
    pub overwritten: bool,
    pub installed_at: String,
}

/// One row in the install store: the architecture's `installs` table in file form until the SQLite
/// store lands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub skill: String,
    pub source: String,
    pub target: String,
    pub path: String,
    pub installed_at: String,
}

/// The install log. A SQLite-backed implementation can replace this without touching install logic.
pub trait InstallStore: Send + Sync {
    fn record(&self, record: InstallRecord) -> Result<(), SkillError>;
    fn list(&self) -> Result<Vec<InstallRecord>, SkillError>;
    /// Forget one installation. The folder itself is removed by [`SkillService::remove_install`].
    fn remove(&self, path: &str) -> Result<(), SkillError>;
}

/// File-backed store under the app-data dir. Writes are atomic; the primary key is
/// `(skill, target, path)`.
pub struct JsonInstallStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl JsonInstallStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    fn read(&self) -> Vec<InstallRecord> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write(&self, records: &[InstallRecord]) -> Result<(), SkillError> {
        let bytes = serde_json::to_vec_pretty(records)
            .map_err(|err| SkillError::Unavailable(format!("serialize installs: {err}")))?;
        super::atomic_write(&self.path, &bytes)?;
        Ok(())
    }
}

impl InstallStore for JsonInstallStore {
    fn record(&self, record: InstallRecord) -> Result<(), SkillError> {
        let guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut records = self.read();
        records.retain(|existing| {
            !(existing.skill == record.skill
                && existing.target == record.target
                && existing.path == record.path)
        });
        records.push(record);
        self.write(&records)?;
        drop(guard);
        Ok(())
    }

    fn list(&self) -> Result<Vec<InstallRecord>, SkillError> {
        Ok(self.read())
    }

    fn remove(&self, path: &str) -> Result<(), SkillError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut records = self.read();
        records.retain(|existing| existing.path != path);
        self.write(&records)
    }
}

/// In-memory store for tests and headless use.
#[derive(Debug, Default)]
pub struct MemoryInstallStore {
    records: Mutex<Vec<InstallRecord>>,
}

impl MemoryInstallStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<InstallRecord>> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl InstallStore for MemoryInstallStore {
    fn record(&self, record: InstallRecord) -> Result<(), SkillError> {
        let mut records = self.lock();
        records.retain(|existing| {
            !(existing.skill == record.skill
                && existing.target == record.target
                && existing.path == record.path)
        });
        records.push(record);
        Ok(())
    }

    fn list(&self) -> Result<Vec<InstallRecord>, SkillError> {
        Ok(self.lock().clone())
    }

    fn remove(&self, path: &str) -> Result<(), SkillError> {
        self.lock().retain(|existing| existing.path != path);
        Ok(())
    }
}

pub(crate) async fn install(
    service: &SkillService,
    request: InstallRequest,
) -> Result<InstallResult, SkillError> {
    let relative = checked_relative(&request.path)?;
    let config = service.config_snapshot();
    let source = find_source(&config, &request.source)?.clone();
    let target = target_dirs(
        &home_dir(config.home.as_deref()),
        request.project_root.as_deref(),
    )
    .into_iter()
    .find(|target| target.id == request.target)
    .ok_or_else(|| SkillError::Invalid(format!("unknown install target: {}", request.target)))?;

    let skill = relative
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| SkillError::Invalid(format!("invalid skill path: {}", request.path)))?
        .to_string();
    let prefix = format!("{}/", display_path(&relative));
    let skill_md = format!("{prefix}SKILL.md");

    let token = service.github_token();
    let (manifest, _) = manifest_for(service, &config, &source, token.as_deref(), false).await?;

    // Validate the whole tree before downloading anything: a symlink or a traversal in one file
    // rejects the install, it does not get skipped silently.
    let mut planned = Vec::new();
    for file in manifest
        .files
        .iter()
        .filter(|file| file.kind != "tree" && file.path.starts_with(&prefix))
    {
        if file.mode == "120000" {
            return Err(SkillError::Invalid(format!(
                "skill contains a symlink: {}",
                file.path
            )));
        }
        let inside = file
            .path
            .strip_prefix(&prefix)
            .ok_or_else(|| SkillError::Invalid(format!("invalid skill file: {}", file.path)))?;
        let inside = checked_relative(inside)?;
        planned.push((inside, file.path.clone(), file.sha.clone()));
    }

    if !planned
        .iter()
        .any(|(inside, _, _)| display_path(inside) == "SKILL.md")
    {
        return Err(SkillError::NotFound(format!(
            "{skill_md} in {}",
            source.repo
        )));
    }

    let mut contents = Vec::with_capacity(planned.len());
    for (inside, repo_path, sha) in planned {
        let body = read_blob(
            service,
            &source.repo,
            &manifest.branch,
            &repo_path,
            &sha,
            token.as_deref(),
        )
        .await?;
        contents.push((inside, body));
    }

    let destination = target.path.join(&skill);
    let overwritten = write_atomically(&target.path, &destination, &contents, &skill)?;

    let installed_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let path = display_path(&destination);
    let result = InstallResult {
        skill: skill.clone(),
        source: source.repo.clone(),
        target: target.id.to_string(),
        path: path.clone(),
        overwritten,
        installed_at: installed_at.clone(),
    };
    service.installs_store().record(InstallRecord {
        skill,
        source: source.repo,
        target: target.id.to_string(),
        path,
        installed_at,
    })?;
    Ok(result)
}

/// Stage into `target_root/.grove-install-<uuid>`, then rename over the destination. The rename is
/// the only visible step, so a partial download never leaves a half-installed skill behind.
fn write_atomically(
    target_root: &Path,
    destination: &Path,
    contents: &[(PathBuf, String)],
    skill: &str,
) -> Result<bool, SkillError> {
    std::fs::create_dir_all(target_root)?;
    let canonical_root = std::fs::canonicalize(target_root)?;

    // Refuse to write through a symlink or junction: it could point outside the target directory.
    let existing = std::fs::symlink_metadata(destination).ok();
    if let Some(metadata) = &existing {
        if metadata.file_type().is_symlink() {
            return Err(SkillError::Invalid(format!(
                "destination is a symlink: {}",
                display_path(destination)
            )));
        }
        match std::fs::canonicalize(destination) {
            Ok(canonical) if canonical.starts_with(&canonical_root) => {}
            Ok(_) => {
                return Err(SkillError::Invalid(format!(
                    "destination escapes the target directory: {}",
                    display_path(destination)
                )))
            }
            Err(_) => {
                return Err(SkillError::Invalid(format!(
                    "destination is a broken link: {}",
                    display_path(destination)
                )))
            }
        }
        if !metadata.is_dir() {
            return Err(SkillError::Invalid(format!(
                "destination exists and is not a directory: {}",
                display_path(destination)
            )));
        }
    }

    let id = Uuid::new_v4().simple();
    let staging = target_root.join(format!(".grove-install-{id}"));
    let backup = target_root.join(format!(".grove-backup-{id}"));

    let staged = (|| -> Result<(), SkillError> {
        std::fs::create_dir_all(&staging)?;
        for (inside, body) in contents {
            let path = staging.join(inside);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, body)?;
        }
        Ok(())
    })();

    if let Err(err) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(err);
    }

    let had_existing = existing.is_some();
    if had_existing {
        std::fs::rename(destination, &backup)?;
    }
    if let Err(err) = std::fs::rename(&staging, destination) {
        let _ = std::fs::remove_dir_all(&staging);
        if had_existing {
            let _ = std::fs::rename(&backup, destination);
        }
        return Err(SkillError::Io(err));
    }
    if had_existing {
        let _ = std::fs::remove_dir_all(&backup);
    }

    tracing::info!(target: "grove::skills", skill, path = %display_path(destination), "installed skill");
    Ok(had_existing)
}
