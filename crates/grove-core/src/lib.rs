//! Grove's core.
//!
//! Everything durable and every external process lives here. This crate must never depend on
//! `tauri` (asserted by `tests/architecture.rs`) so that the whole product surface stays testable
//! headlessly, and so the UI shell can be replaced without touching behaviour.

pub mod gitops;
pub mod ipc;
pub mod paths;
pub mod secrets;
pub mod skills;
pub mod specgraph;

use paths::AppPaths;
use std::path::PathBuf;
use std::sync::Arc;

pub const APP_NAME: &str = "Grove";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Root of the service graph the Tauri shell owns.
///
/// Feature modules add one field here (see `architecture.md` §Extension seams). Constructing it is
/// cheap and does no I/O; call [`GroveCore::init`] to create directories and open the store.
#[derive(Debug, Clone)]
pub struct GroveCore {
    pub paths: AppPaths,
    /// The project whose specs, git state and plan Grove is looking at. Tickets that need a project
    /// root read it here instead of guessing from the UI. Defaults to `$GROVE_PROJECT_ROOT`, else the
    /// process working directory; [`GroveCore::with_project_root`] lets the shell or a test pin it.
    pub project_root: PathBuf,
    pub skills: Arc<skills::SkillService>,
    pub git: gitops::Git,
}

impl GroveCore {
    pub fn new(paths: AppPaths) -> Self {
        let secrets: Arc<dyn secrets::SecretStore> = Arc::new(secrets::MemorySecretStore::new());
        let project_root = std::env::var_os("GROVE_PROJECT_ROOT")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            skills: Arc::new(skills::SkillService::new(paths.clone(), secrets)),
            paths,
            project_root,
            git: gitops::Git::system(),
        }
    }

    pub fn with_project_root(mut self, project_root: impl Into<PathBuf>) -> Self {
        self.project_root = project_root.into();
        self
    }

    /// Create the directories the app writes to.
    pub fn init(&self) -> std::io::Result<()> {
        self.paths.ensure()
    }

    /// What the webview shows in its header. Shape is pinned by `contract/fixtures/app-info.json`.
    pub fn info(&self) -> ipc::AppInfo {
        ipc::AppInfo {
            name: APP_NAME.to_string(),
            version: VERSION.to_string(),
            app_data_dir: self.paths.app_data_display(),
            platform: std::env::consts::OS.to_string(),
        }
    }
}
