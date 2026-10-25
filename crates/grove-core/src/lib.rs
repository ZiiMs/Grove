//! Grove's core.
//!
//! Everything durable and every external process lives here. This crate must never depend on
//! `tauri` (asserted by `tests/architecture.rs`) so that the whole product surface stays testable
//! headlessly, and so the UI shell can be replaced without touching behaviour.

pub mod adapters;
pub mod gitops;
pub mod harness_doctor;
pub mod ipc;
pub mod mcp;
pub mod paths;
pub mod plan;
pub mod secrets;
pub mod sessions;
pub mod settings;
pub mod skills;
pub mod specgraph;
pub mod store;

use ipc::IpcResult;
use paths::{AppPaths, ProjectRoot};
use sessions::manager::{SessionError, SessionManager};
use specgraph::SpecGraph;
use std::path::{Path, PathBuf};
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
    /// The project whose specs, plan, MCP configs and project-local skills Grove is looking at.
    ///
    /// Shared (`Arc<RwLock<_>>`) with the services that consume it, so moving it moves all of them.
    /// It starts at `$GROVE_PROJECT_ROOT`, else the process working directory, and the shell then
    /// follows the active session with [`GroveCore::set_project_root`]. Nothing else may cache a
    /// project path: two notions of "the project" is exactly the bug this replaced.
    project_root: ProjectRoot,
    pub skills: Arc<skills::SkillService>,
    pub git: gitops::Git,
    /// One versioned TODO plan per session, mirrored into the session's project for the agent.
    pub plan: Arc<plan::PlanService>,
    /// Pinned specs plus the plan, assembled into the documents a harness receives at start
    /// (ticket 14). Also the durable side of the pin commands.
    pub context: Arc<sessions::context::SessionContextService>,
    /// The session runtime: adapters, live sessions, and the SQLite transcript.
    pub sessions: Arc<SessionManager>,
    pub mcp: Arc<mcp::McpService>,
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

    /// The project Grove is looking at right now: the spec graph's root, the plan mirror's fallback,
    /// MCP discovery's project files, and the project-local skills target all read this.
    pub fn project_root(&self) -> PathBuf {
        self.project_root.path()
    }

    /// Point every project-scoped subsystem at `root` and return the path now in force.
    ///
    /// The shell calls this when a session is created, resumed or its tab is activated, so a session
    /// on `C:/work/grove` indexes that repository instead of the app's own directory. Because each
    /// service holds a clone of the same [`ProjectRoot`], nothing has to be rebuilt.
    pub fn set_project_root(&self, root: impl Into<PathBuf>) -> PathBuf {
        self.project_root.set(root);
        self.project_root()
    }

    /// Pin the initial project root (tests and previews). Same effect as
    /// [`GroveCore::set_project_root`]; kept as a builder so a fixture reads in one expression.
    pub fn with_project_root(self, project_root: impl Into<PathBuf>) -> Self {
        self.set_project_root(project_root);
        self
    }

    /// Index the project Grove is looking at. The graph carries its root, so the Specs panel can show
    /// which project it is reading and re-index when that changes.
    pub fn spec_index(&self) -> IpcResult<SpecGraph> {
        Ok(specgraph::SpecIndex::load(self.project_root())?.graph())
    }

    /// The skill destinations: each harness's own directory plus the project-local `.agents/skills`.
    ///
    /// An explicit `project_root` wins (the UI may pass one); otherwise the local target follows the
    /// active session like every other project-scoped thing, instead of resolving against the
    /// executable's directory.
    pub fn skill_targets(&self, project_root: Option<&Path>) -> Vec<skills::SkillTarget> {
        let explicit = project_root.map(Path::to_path_buf);
        let root = explicit.unwrap_or_else(|| self.project_root());
        self.skills.targets(Some(root.as_path()))
    }

    fn assemble(
        paths: AppPaths,
        sessions: Arc<SessionManager>,
        secrets: Arc<dyn secrets::SecretStore>,
    ) -> Self {
        // The starting point only: `GROVE_PROJECT_ROOT` keeps headless runs and tests pinnable, and
        // the shell moves the root to the active session from here on.
        let project_root = ProjectRoot::new(
            std::env::var_os("GROVE_PROJECT_ROOT")
                .map(PathBuf::from)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from(".")),
        );
        let settings = Arc::new(settings::SettingsService::new(sessions.store()));
        let doctor = Arc::new(harness_doctor::HarnessDoctor::new(
            Arc::clone(&settings),
            Arc::clone(&sessions),
        ));
        let skills = skills::SkillService::new(paths.clone(), Arc::clone(&secrets));
        let plan = Arc::new(plan::PlanService::new(
            sessions.store(),
            project_root.clone(),
        ));
        // The session runtime asks this service for its knowledge; the port keeps `sessions` free of
        // the spec graph and the plan. Wiring it here is the one line that makes pinned specs real.
        let context = Arc::new(sessions::context::SessionContextService::new(
            sessions.store(),
            Arc::clone(&plan),
            project_root.clone(),
        ));
        sessions
            .set_context_source(Arc::clone(&context) as Arc<dyn sessions::context::ContextSource>);
        // `StartContext.config` is "adapter-specific, from settings": a harness path configured in
        // the Settings panel is the command the next session of that harness launches. The manager
        // sees the port, not the settings store.
        sessions.set_config_source(Arc::new(settings::HarnessPathSource::new(Arc::clone(
            &settings,
        ))));
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
            mcp: Arc::new(mcp::McpService::with_project_root(
                mcp::McpConfig {
                    project_root: Some(project_root.path()),
                    ..Default::default()
                },
                project_root.clone(),
            )),
            plan,
            context,
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
