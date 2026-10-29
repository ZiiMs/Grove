//! Workspace projects: adding a folder, detecting its git facts, and browsing folders for the picker.
//!
//! A project is a durable row; its worktrees are read from git on demand because git owns checkout
//! state. This module is the only place that turns a folder path into a `Project`, which keeps the
//! validation (exists, is a folder, unique name and path) in one place.

use crate::gitops::{Git, GitWorktree};
use crate::ipc::{
    DirEntry, DirListing, FsRoot, FsRootKind, FsRoots, IpcError, Project, ProjectDetection,
};
use crate::paths::display_path;
use crate::sessions::event::now;
use crate::store::{Store, StoreError};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Arguments to `project_add`. Every optional field falls back to detection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AddProjectRequest {
    pub path: String,
    pub name: Option<String>,
    pub default_branch: Option<String>,
    pub remote: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("project not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Git(#[from] IpcError),
}

impl From<ProjectError> for IpcError {
    fn from(error: ProjectError) -> Self {
        match error {
            ProjectError::Invalid(message) => IpcError::invalid(message),
            ProjectError::NotFound(message) => IpcError::not_found(message),
            ProjectError::Store(StoreError::Invalid(message)) => IpcError::invalid(message),
            ProjectError::Store(StoreError::Sqlite(error)) => IpcError::io(error.to_string()),
            ProjectError::Store(StoreError::Io(error)) => IpcError::io(error.to_string()),
            ProjectError::Store(StoreError::Json(error)) => IpcError::io(error.to_string()),
            ProjectError::Git(error) => error,
        }
    }
}

pub type ProjectResult<T> = Result<T, ProjectError>;

/// Projects as the rest of the app sees them: durable rows plus live git detection.
#[derive(Clone)]
pub struct ProjectService {
    store: Arc<Store>,
    git: Git,
}

impl ProjectService {
    pub fn new(store: Arc<Store>, git: Git) -> Self {
        Self { store, git }
    }

    pub fn list(&self) -> ProjectResult<Vec<Project>> {
        Ok(self.store.list_projects()?)
    }

    pub fn get(&self, id: &str) -> ProjectResult<Project> {
        self.store
            .project(id)?
            .ok_or_else(|| ProjectError::NotFound(id.to_string()))
    }

    /// Detect git facts for a folder the user is about to add.
    pub fn detect(&self, path: &str) -> ProjectResult<ProjectDetection> {
        let folder = resolve_folder(Some(path))?;
        let detection = self.git.detect(&folder)?;
        Ok(ProjectDetection {
            path: display_path(&folder),
            is_git: detection.is_git,
            current_branch: detection.current_branch,
            branches: detection.branches,
            default_branch: detection.default_branch,
            remote: detection.remote,
        })
    }

    /// Add a folder as a project, then attach existing sessions that ran in it.
    pub fn add(&self, request: AddProjectRequest) -> ProjectResult<Project> {
        let folder = resolve_folder(Some(&request.path))?;
        let path = display_path(&folder);
        let detection = self.git.detect(&folder)?;

        let name = request
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .or_else(|| {
                folder
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| ProjectError::Invalid("a folder name is required".into()))?;
        validate_label("name", &name)?;

        if self.store.project_name_taken(&name)? {
            return Err(ProjectError::Invalid(format!(
                "a project named '{name}' already exists"
            )));
        }
        if self.store.project_path_taken(&path)? {
            return Err(ProjectError::Invalid(format!(
                "'{path}' is already a project"
            )));
        }

        let remote = request
            .remote
            .as_deref()
            .map(str::trim)
            .filter(|remote| !remote.is_empty())
            .map(str::to_string)
            .or(detection.remote.clone());
        let default_branch = request
            .default_branch
            .as_deref()
            .map(str::trim)
            .filter(|branch| !branch.is_empty())
            .map(str::to_string)
            .or(detection.default_branch.clone())
            .filter(|_| detection.is_git);

        let at = now();
        let project = Project {
            id: Uuid::new_v4().to_string(),
            name,
            path: path.clone(),
            remote,
            default_branch,
            is_git: detection.is_git,
            created_at: at.clone(),
            updated_at: at,
        };
        self.store.insert_project(&project)?;

        // Attach conversations that already ran here. A failure to list worktrees must not undo the
        // project: the project path alone still links the common case.
        let mut paths = vec![path];
        if detection.is_git {
            if let Ok(worktrees) = self.git.worktrees(Some(&folder)) {
                paths.extend(worktrees.into_iter().map(|worktree| worktree.path));
            }
        }
        let _ = self.store.link_sessions_to_project(&project.id, &paths);
        Ok(project)
    }

    pub fn create_worktree(
        &self,
        repo_path: &str,
        branch: &str,
        base: &str,
        path: &str,
    ) -> ProjectResult<GitWorktree> {
        let repo = resolve_folder(Some(repo_path))?;
        let worktree = self.git.create_worktree(Some(&repo), branch, base, path)?;
        Ok(worktree)
    }
}

/// Resolve a user-typed folder to an existing canonical directory. `None` or an empty string means
/// the home folder, which is where the picker opens.
pub fn resolve_folder(requested: Option<&str>) -> ProjectResult<PathBuf> {
    let raw = normalize_path(requested.unwrap_or_default());
    let expanded = if raw.is_empty() || raw == "~" {
        home_dir()
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(raw)
    };
    if expanded.as_os_str().is_empty() {
        return Err(ProjectError::Invalid("a folder path is required".into()));
    }
    let canonical = std::fs::canonicalize(&expanded).map_err(|error| {
        ProjectError::Invalid(format!("cannot open {}: {error}", expanded.display()))
    })?;
    // Windows canonicalization returns a verbatim `\\?\C:\…` path; store the normal form so
    // it compares with the paths git and the session rows carry.
    #[cfg(windows)]
    let canonical = {
        let raw = canonical.to_string_lossy().to_string();
        if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{rest}"))
        } else {
            PathBuf::from(raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_string())
        }
    };
    let metadata = std::fs::metadata(&canonical).map_err(|error| {
        ProjectError::Invalid(format!("cannot read {}: {error}", canonical.display()))
    })?;
    if !metadata.is_dir() {
        return Err(ProjectError::Invalid(format!(
            "{} is not a folder",
            canonical.display()
        )));
    }
    Ok(canonical)
}

/// One level of folders for the add-project picker. Hidden folders are skipped: they are almost never
/// a project root and would bury the useful rows.
pub fn list_dirs(requested: Option<&str>) -> ProjectResult<DirListing> {
    let folder = resolve_folder(requested)?;
    let path = display_path(&folder);

    let mut dirs = Vec::new();
    let entries = std::fs::read_dir(&folder).map_err(|error| {
        ProjectError::Invalid(format!("cannot list {}: {error}", folder.display()))
    })?;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let name = entry.file_name();
        let name = match name.to_str() {
            Some(name) if !name.starts_with('.') => name.to_string(),
            _ => continue,
        };
        let candidate = entry.path();
        if !candidate.is_dir() {
            continue;
        }
        dirs.push(DirEntry {
            name,
            path: display_path(&candidate),
        });
    }
    dirs.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));

    let parent = folder
        .parent()
        .filter(|parent| *parent != folder)
        .map(display_path);

    Ok(DirListing { path, parent, dirs })
}

/// Every root the add-project picker offers: the home folder first, then mounted drives on Windows
/// (and readable `/Volumes` entries on macOS) or `/` on Unix. Paths are forward-slash display paths
/// so the UI can compare them with `DirListing.path`.
pub fn list_roots() -> FsRoots {
    let mut roots = Vec::new();
    let home = home_dir();
    if !home.as_os_str().is_empty() && home.is_dir() {
        roots.push(FsRoot {
            name: "Home".into(),
            path: normalize_path(&home.to_string_lossy()),
            kind: FsRootKind::Home,
        });
    }

    #[cfg(windows)]
    {
        for letter in b'A'..=b'Z' {
            let path = format!("{}:/", letter as char);
            if std::path::Path::new(&path).is_dir() {
                roots.push(FsRoot {
                    name: format!("{}:", letter as char),
                    path,
                    kind: FsRootKind::Drive,
                });
            }
        }
    }
    #[cfg(unix)]
    {
        roots.push(FsRoot {
            name: "/".into(),
            path: "/".into(),
            kind: FsRootKind::Filesystem,
        });
        #[cfg(target_os = "macos")]
        if let Ok(entries) = std::fs::read_dir("/Volumes") {
            let mut volumes: Vec<FsRoot> = entries
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') || !entry.path().is_dir() {
                        return None;
                    }
                    Some(FsRoot {
                        name,
                        path: display_path(&entry.path()),
                        kind: FsRootKind::Drive,
                    })
                })
                .collect();
            volumes.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
            roots.extend(volumes);
        }
    }

    let home = roots
        .iter()
        .find(|root| root.kind == FsRootKind::Home)
        .map(|root| root.path.clone());
    FsRoots { home, roots }
}

/// Create one folder under `parent` for the add-project picker. The name must be a single path
/// segment; nothing is created recursively and an existing path is never reused.
pub fn create_dir(parent: &str, name: &str) -> ProjectResult<DirEntry> {
    let folder = resolve_folder(Some(parent))?;
    let name = name.trim();
    validate_folder_name(name)?;
    let path = folder.join(name);
    if path.exists() {
        return Err(ProjectError::Invalid(format!("'{name}' already exists")));
    }
    std::fs::create_dir(&path).map_err(|error| {
        ProjectError::Invalid(format!("cannot create {}: {error}", path.display()))
    })?;
    Ok(DirEntry {
        name: name.to_string(),
        path: display_path(&path),
    })
}

fn validate_folder_name(name: &str) -> ProjectResult<()> {
    if name.is_empty() {
        return Err(ProjectError::Invalid("a folder name is required".into()));
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(ProjectError::Invalid(
            "a folder name must be a single path segment".into(),
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(ProjectError::Invalid(
            "a folder name cannot contain control characters".into(),
        ));
    }
    Ok(())
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Normalize a folder path the user typed or pasted: trim whitespace and a matching pair of
/// surrounding quotes, use forward slashes, collapse repeated separators, and keep a root's trailing
/// separator. `~` is left for [`resolve_folder`] to expand. This is not URL parsing: a `file://`
/// string is treated as a literal path, as the plan requires.
pub fn normalize_path(raw: &str) -> String {
    let mut value = raw.trim();
    if let Some(unquoted) = strip_quotes(value) {
        value = unquoted.trim();
    }
    if value.is_empty() {
        return String::new();
    }

    let unified = value.replace('\\', "/");
    let unc = unified.starts_with("//");
    let absolute = unified.starts_with('/');
    let parts: Vec<&str> = unified.split('/').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return if unc {
            "//".to_string()
        } else if absolute {
            "/".to_string()
        } else {
            String::new()
        };
    }

    // A drive path always uses forward slashes; a bare drive is its root (`C:/`).
    if let Some((drive, rest)) = parts.split_first() {
        if is_drive(drive) && rest.is_empty() {
            return format!("{drive}/");
        }
        if is_drive(drive) {
            return format!("{drive}/{}", rest.join("/"));
        }
    }

    let prefix = if unc {
        "//"
    } else if absolute {
        "/"
    } else {
        ""
    };
    format!("{prefix}{}", parts.join("/"))
}

fn is_drive(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn strip_quotes(value: &str) -> Option<&str> {
    let first = value.chars().next()?;
    let last = value.chars().next_back()?;
    if value.len() >= 2 && first == last && (first == '"' || first == '\'') {
        Some(&value[1..value.len() - 1])
    } else {
        None
    }
}

fn validate_label(field: &str, value: &str) -> ProjectResult<()> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(ProjectError::Invalid(format!(
            "project {field} must be a non-empty single line"
        )));
    }
    Ok(())
}

/// The project a path belongs to, for a session whose `project_id` predates this build.
pub fn project_for_path(store: &Store, path: &str) -> Option<Project> {
    store.project_for_path(path).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_handles_pasted_windows_paths() {
        for raw in [
            r"D:\Dev\grove\",
            "D:/Dev/grove",
            "  D:/Dev/grove  ",
            "\"D:\\Dev\\grove\"",
        ] {
            assert_eq!(normalize_path(raw), "D:/Dev/grove", "raw: {raw}");
        }
    }

    #[test]
    fn normalize_path_collapses_separators_but_keeps_roots() {
        assert_eq!(normalize_path("D:///Dev//grove"), "D:/Dev/grove");
        assert_eq!(normalize_path("D:/"), "D:/");
        assert_eq!(normalize_path("D:\\"), "D:/");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("//server/share/"), "//server/share");
    }

    #[test]
    fn normalize_path_keeps_home_shorthand() {
        assert_eq!(normalize_path("~"), "~");
        assert_eq!(normalize_path("~\\dev\\grove"), "~/dev/grove");
    }

    #[test]
    fn normalize_path_trims_quotes_and_whitespace() {
        assert_eq!(normalize_path("  \"/tmp/grove\"  "), "/tmp/grove");
    }
}
