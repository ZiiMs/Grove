//! Grove's core.
//!
//! Everything durable and every external process lives here. This crate must never depend on
//! `tauri` (asserted by `tests/architecture.rs`) so that the whole product surface stays testable
//! headlessly, and so the UI shell can be replaced without touching behaviour.

pub mod adapters;
pub mod gitops;
pub mod harness_doctor;
pub mod ipc;
pub mod paths;
pub mod secrets;
pub mod sessions;
pub mod settings;
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
    /// The session runtime: adapters, live sessions, and the SQLite transcript.
    pub sessions: Arc<SessionManager>,
    /// The only way secrets are read or written. `init` hands over the OS keychain; `new` keeps an
    /// in-memory store so tests and previews never touch it.
    pub secrets: Arc<dyn secrets::SecretStore>,
    /// Typed settings; the durable store lives behind [`SessionManager::store`].
    pub settings: Arc<settings::SettingsService>,
    /// Resolves harness and tool binaries for the Settings panel.
    pub doctor: Arc<harness_doctor::HarnessDoctor>,
}

impl GroveCore {
    /// Cheap construction that does no I/O: the session manager keeps an in-memory store and secrets
    /// stay in memory. The app calls [`GroveCore::init`]; tests and previews that never persist use
    /// this.
    pub fn new(paths: AppPaths) -> Self {
        let sessions = SessionManager::in_memory(&paths).expect("open the in-memory Grove store");
        let secrets: Arc<dyn secrets::SecretStore> = Arc::new(secrets::MemorySecretStore::new());
        Self::assemble(paths, Arc::new(sessions), secrets)
    }

    /// Create the directories the app writes to and open the durable store at `paths.database`.
    /// Secrets go to the OS keychain through [`secrets::KeychainSecretStore`].
    pub fn init(paths: AppPaths) -> Result<Self, SessionError> {
        paths.ensure()?;
        let sessions = SessionManager::open(paths.clone())?;
        let secrets: Arc<dyn secrets::SecretStore> =
            Arc::new(secrets::KeychainSecretStore::grove());
        Ok(Self::assemble(paths, Arc::new(sessions), secrets))
    }

    pub fn with_project_root(mut self, project_root: impl Into<PathBuf>) -> Self {
        self.project_root = project_root.into();
        self
    }

    fn assemble(
        paths: AppPaths,
        sessions: Arc<SessionManager>,
        secrets: Arc<dyn secrets::SecretStore>,
    ) -> Self {
        let project_root = std::env::var_os("GROVE_PROJECT_ROOT")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let settings = Arc::new(settings::SettingsService::new(sessions.store()));
        let doctor = Arc::new(harness_doctor::HarnessDoctor::new(
            Arc::clone(&settings),
            Arc::clone(&sessions),
        ));
        let skills = skills::SkillService::new(paths.clone(), Arc::clone(&secrets));
        // Settings own the catalog sources; apply them at startup so the Skills panel agrees. A bad
        // settings row keeps the built-in defaults instead of blocking the app.
        if let Ok(current) = settings.get() {
            skills.set_sources(
                current
                    .catalog_sources
                    .into_iter()
                    .map(skills::SkillSource::new)
                    .collect(),
            );
        }
        Self {
            skills: Arc::new(skills),
            git: gitops::Git::system(),
            paths,
            project_root,
            sessions,
            secrets,
            settings,
            doctor,
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
