//! Grove's core.
//!
//! Everything durable and every external process lives here. This crate must never depend on
//! `tauri` (asserted by `tests/architecture.rs`) so that the whole product surface stays testable
//! headlessly, and so the UI shell can be replaced without touching behaviour.

pub mod gitops;
pub mod ipc;
pub mod mcp;
pub mod paths;
pub mod plan;
pub mod projects;
pub mod providers;
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
    /// Workspace projects: durable folders plus live git detection.
    pub projects: Arc<projects::ProjectService>,
    /// Pinned specs plus the plan, assembled into the documents a provider receives at start
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
    /// Provider API keys: the only way a session authenticates. Values live in the keychain; this
    /// service is the only reader.
    pub credentials: Arc<providers::credentials::Credentials>,
    /// Shared HTTP client for provider probe requests. Sessions build their own through the manager.
    pub http: reqwest::Client,
}

impl GroveCore {
    /// Cheap construction that does no I/O: the session manager keeps an in-memory store and secrets
    /// stay in memory. The app calls [`GroveCore::init`]; tests and previews that never persist use
    /// this.
    pub fn new(paths: AppPaths) -> Self {
        // Previews and tests run the offline provider: no key, no network, real session runtime.
        let sessions =
            SessionManager::in_memory_offline(&paths).expect("open the in-memory Grove store");
        let secrets: Arc<dyn secrets::SecretStore> = Arc::new(secrets::MemorySecretStore::new());
        Self::assemble(paths, Arc::new(sessions), secrets)
    }

    /// Create the directories the app writes to and open the durable store at `paths.database`.
    /// Secrets go to the OS keychain through [`secrets::KeychainSecretStore`].
    pub fn init(paths: AppPaths) -> Result<Self, SessionError> {
        paths.ensure()?;
        // One keychain, handed to both the session runtime and the settings pane: a key saved in
        // Settings is the key a session authenticates with.
        let secrets: Arc<dyn secrets::SecretStore> =
            Arc::new(secrets::KeychainSecretStore::grove());
        let sessions = SessionManager::open_with_secrets(paths.clone(), Arc::clone(&secrets))?;
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

    /// The skill destinations: the user's global `.agents/skills` plus the project-local one.
    ///
    /// An explicit `project_root` wins (the UI may pass one); otherwise the local target follows the
    /// active session like every other project-scoped thing, instead of resolving against the
    /// executable's directory.
    pub fn skill_targets(&self, project_root: Option<&Path>) -> Vec<skills::SkillTarget> {
        let explicit = project_root.map(Path::to_path_buf);
        let root = explicit.unwrap_or_else(|| self.project_root());
        self.skills.targets(Some(root.as_path()))
    }

    /// Skill folders already present under either target, including ones Grove did not install.
    ///
    /// The root follows the same rule as [`Self::skill_targets`].
    pub fn skill_inventory(&self, project_root: Option<&Path>) -> Vec<skills::DetectedSkill> {
        let explicit = project_root.map(Path::to_path_buf);
        let root = explicit.unwrap_or_else(|| self.project_root());
        self.skills.detected(Some(root.as_path()))
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
        let credentials = Arc::new(providers::credentials::Credentials::new(Arc::clone(
            &secrets,
        )));
        let skills = skills::SkillService::new(paths.clone(), Arc::clone(&secrets));
        let plan = Arc::new(plan::PlanService::new(
            sessions.store(),
            project_root.clone(),
        ));
        let git = gitops::Git::system();
        let projects = Arc::new(projects::ProjectService::new(sessions.store(), git.clone()));
        // The session runtime asks this service for its knowledge; the port keeps `sessions` free of
        // the spec graph and the plan. Wiring it here is the one line that makes pinned specs real.
        let context = Arc::new(sessions::context::SessionContextService::new(
            sessions.store(),
            Arc::clone(&plan),
            project_root.clone(),
        ));
        sessions
            .set_context_source(Arc::clone(&context) as Arc<dyn sessions::context::ContextSource>);
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
            git,
            mcp: Arc::new(mcp::McpService::with_project_root(
                mcp::McpConfig {
                    project_root: Some(project_root.path()),
                    ..Default::default()
                },
                project_root.clone(),
            )),
            plan,
            projects,
            context,
            paths,
            project_root,
            sessions,
            secrets,
            settings,
            credentials,
            http: providers::http_client(),
        }
    }

    /// The models the credential for `provider` can reach. One small request, used by the settings
    /// pane's "test key" action. The catalog's `models` remains the picker's list: a gateway can
    /// expose more models than this build has a client for.
    pub async fn provider_models(
        &self,
        provider: &str,
    ) -> Result<Vec<String>, providers::ProviderError> {
        let spec = providers::catalog::find(provider).ok_or_else(|| {
            providers::ProviderError::Unavailable(format!("unknown provider: {provider}"))
        })?;
        let (key, _) = self.credentials.key(provider)?;
        let transport = providers::catalog::transport(provider);
        providers::probe::list_models(&self.http, &spec, &key, transport.models_url).await
    }

    /// Each provider model's reasoning levels, from the cached models.dev catalogue.
    ///
    /// The model list itself comes from the provider; this only supplies the capability layer the
    /// composer needs to show the tiers the selected model actually accepts. A catalogue failure is
    /// intentionally not an error: the UI has a static fallback.
    pub async fn provider_thinking(&self) -> providers::model_catalog::ThinkingMap {
        let cache = self.paths.cache.join("model-thinking.json");
        providers::model_catalog::thinking_options(&self.http, &cache).await
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
