//! Grove's core.
//!
//! Everything durable and every external process lives here. This crate must never depend on
//! `tauri` (asserted by `tests/architecture.rs`) so that the whole product surface stays testable
//! headlessly, and so the UI shell can be replaced without touching behaviour.

pub mod adapters;
pub mod ipc;
pub mod paths;
pub mod secrets;
pub mod sessions;
pub mod store;

use paths::AppPaths;
use sessions::manager::{SessionError, SessionManager};
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
    /// The session runtime: adapters, live sessions, and the SQLite transcript.
    pub sessions: Arc<SessionManager>,
}

impl GroveCore {
    /// Cheap construction that does no I/O: the session manager keeps an in-memory store. The app
    /// calls [`GroveCore::init`]; tests and previews that never persist use this.
    pub fn new(paths: AppPaths) -> Self {
        let sessions = SessionManager::in_memory(&paths).expect("open the in-memory Grove store");
        Self {
            paths,
            sessions: Arc::new(sessions),
        }
    }

    /// Create the directories the app writes to and open the durable store at `paths.database`.
    pub fn init(paths: AppPaths) -> Result<Self, SessionError> {
        paths.ensure()?;
        let sessions = SessionManager::open(paths.clone())?;
        Ok(Self {
            paths,
            sessions: Arc::new(sessions),
        })
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
