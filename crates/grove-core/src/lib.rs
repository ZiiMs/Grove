//! Grove's core.
//!
//! Everything durable and every external process lives here. This crate must never depend on
//! `tauri` (asserted by `tests/architecture.rs`) so that the whole product surface stays testable
//! headlessly, and so the UI shell can be replaced without touching behaviour.

pub mod adapters;
pub mod gitops;
pub mod ipc;
pub mod mcp;
pub mod paths;
pub mod plan;
pub mod secrets;
pub mod sessions;
pub mod skills;
pub mod specgraph;
pub mod store;

use paths::AppPaths;
use sessions::manager::{SessionError, SessionManager};
use std::path::PathBuf;
use std::sync::Arc;

pub const APP_NAME: &str = "Grove";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Root of the service graph the Tauri shell owns.
///
/// Feature modules add one field here (see `architecture.md` §Extension seams). Constructing it is
/// cheap and does no I/O; call [`GroveCore::init`] to create directories and open the store.
#[derive(Clone)]
pub struct GroveCore {
    pub paths: AppPaths,
    /// The project whose specs, git state and plan Grove is looking at. Tickets that need a project
    /// root read it here instead of guessing from the UI. Defaults to `$GROVE_PROJECT_ROOT`, else the
    /// process working directory; [`GroveCore::with_project_root`] lets the shell or a test pin it.
    pub project_root: PathBuf,
    pub skills: Arc<skills::SkillService>,
    pub git: gitops::Git,
    /// One versioned TODO plan per session, mirrored into the session's project for the agent.
    pub plan: Arc<plan::PlanService>,
    /// The session runtime: adapters, live sessions, and the SQLite transcript.
    pub sessions: Arc<SessionManager>,
    pub mcp: Arc<mcp::McpService>,
}

impl GroveCore {
    /// Cheap construction that does no I/O: the session manager keeps an in-memory store. The app
    /// calls [`GroveCore::init`]; tests and previews that never persist use this.
    pub fn new(paths: AppPaths) -> Self {
        let sessions = SessionManager::in_memory(&paths).expect("open the in-memory Grove store");
        Self::assemble(paths, Arc::new(sessions))
    }

    /// Create the directories the app writes to and open the durable store at `paths.database`.
    pub fn init(paths: AppPaths) -> Result<Self, SessionError> {
        paths.ensure()?;
        let sessions = SessionManager::open(paths.clone())?;
        Ok(Self::assemble_durable(paths, Arc::new(sessions)))
    }

    pub fn with_project_root(mut self, project_root: impl Into<PathBuf>) -> Self {
        self.project_root = project_root.into();
        self.mcp = Arc::new(mcp::McpService::new(mcp::McpConfig {
            project_root: Some(self.project_root.clone()),
            ..Default::default()
        }));
        self
    }

    fn assemble(paths: AppPaths, sessions: Arc<SessionManager>) -> Self {
        Self::build(paths, sessions, Arc::new(secrets::MemorySecretStore::new()))
    }

    fn assemble_durable(paths: AppPaths, sessions: Arc<SessionManager>) -> Self {
        // Ticket 07 swaps the OS keychain in here; until then the durable app still uses the
        // in-memory store so no host keychain is required to start.
        Self::build(paths, sessions, Arc::new(secrets::MemorySecretStore::new()))
    }

    fn build(
        paths: AppPaths,
        sessions: Arc<SessionManager>,
        secrets: Arc<dyn secrets::SecretStore>,
    ) -> Self {
        let project_root = std::env::var_os("GROVE_PROJECT_ROOT")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            skills: Arc::new(skills::SkillService::new(paths.clone(), secrets)),
            git: gitops::Git::system(),
            mcp: Arc::new(mcp::McpService::new(mcp::McpConfig {
                project_root: Some(project_root.clone()),
                ..Default::default()
            })),
            plan: Arc::new(plan::PlanService::new(
                sessions.store(),
                project_root.clone(),
            )),
            paths,
            project_root,
            sessions,
        }
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
