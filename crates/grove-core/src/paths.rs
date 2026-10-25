//! Filesystem locations. All paths the app owns come from here — never string-concatenated ad hoc,
//! never with a hard-coded separator, so Windows stays first-class.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// The project Grove is looking at, shared by every subsystem that has to agree on it.
///
/// There is exactly **one** notion of "the project": the spec graph indexes it, the plan mirror falls
/// back to it, MCP discovery reads `.mcp.json` under it, and the project-local skills target writes
/// into it. It is not the app's working directory — it follows the active session, which is what a
/// user means by "this repo". The shell points it at a session's `cwd` when the session is created,
/// resumed or its tab is activated; `GROVE_PROJECT_ROOT` (else the process cwd) is only where it
/// starts, so headless runs and tests can still pin one.
///
/// Cloning a `ProjectRoot` shares it: every holder reads the current path, so nothing caches a stale
/// project and no service has to be rebuilt when the user switches sessions.
#[derive(Debug, Clone)]
pub struct ProjectRoot(Arc<RwLock<PathBuf>>);

impl ProjectRoot {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(Arc::new(RwLock::new(path.into())))
    }

    /// The current project. Cloned, so a caller can keep using it while the root moves.
    pub fn path(&self) -> PathBuf {
        self.0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Point every holder at another project.
    pub fn set(&self, path: impl Into<PathBuf>) {
        *self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = path.into();
    }
}

impl From<PathBuf> for ProjectRoot {
    fn from(path: PathBuf) -> Self {
        Self::new(path)
    }
}

impl From<&Path> for ProjectRoot {
    fn from(path: &Path) -> Self {
        Self::new(path.to_path_buf())
    }
}

impl From<&PathBuf> for ProjectRoot {
    fn from(path: &PathBuf) -> Self {
        Self::new(path.clone())
    }
}

impl PartialEq for ProjectRoot {
    fn eq(&self, other: &Self) -> bool {
        self.path() == other.path()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// Tauri app-data dir: `%APPDATA%/Grove` on Windows.
    pub app_data: PathBuf,
    /// Where the SQLite database lives.
    pub database: PathBuf,
    /// Where per-session plan mirrors live, under a project root.
    pub cache: PathBuf,
}

impl AppPaths {
    pub fn new(app_data: impl Into<PathBuf>) -> Self {
        let app_data = app_data.into();
        Self {
            database: app_data.join("grove.db"),
            cache: app_data.join("cache"),
            app_data,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.app_data)?;
        std::fs::create_dir_all(&self.cache)?;
        Ok(())
    }

    /// Stable, forward-slash form shown in the UI and written into contract fixtures.
    pub fn app_data_display(&self) -> String {
        display_path(&self.app_data)
    }
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_sits_under_app_data() {
        let paths = AppPaths::new(PathBuf::from("/tmp/grove"));
        assert_eq!(paths.database, PathBuf::from("/tmp/grove/grove.db"));
        assert_eq!(paths.cache, PathBuf::from("/tmp/grove/cache"));
    }

    #[test]
    fn a_project_root_is_shared_by_every_clone() {
        let root = ProjectRoot::new(PathBuf::from("C:/one"));
        let shared = root.clone();
        assert_eq!(shared.path(), PathBuf::from("C:/one"));
        root.set(Path::new("C:/two"));
        assert_eq!(
            shared.path(),
            PathBuf::from("C:/two"),
            "a clone sees the move: services never cache the project"
        );
        assert_eq!(ProjectRoot::from(Path::new("C:/two")), root);
    }

    #[test]
    fn display_path_normalises_separators() {
        assert_eq!(
            display_path(Path::new("C:\\Users\\demo\\Grove")),
            "C:/Users/demo/Grove"
        );
    }
}
