//! The session runtime: live sessions, their transcript, and the commands that drive them.
//!
//! A session is a row plus, while it is running, one in-process agent and one pump task. The manager
//! owns both. Commands (`send`, `cancel`, `respond_permission`) go through a per-session mutex, so a
//! session never has two turns in flight; events come back the other way through the pump.
//!
//! Nothing here imports `tauri`: the shell forwards events and exposes commands, and the whole runtime
//! is testable headlessly.

use crate::ipc::{
    CreateSessionRequest, EventEnvelope, IpcError, PlanUsage, ProviderStatus, Session,
    SessionDetail, SessionSummary,
};
use crate::paths::{display_path, AppPaths};
use crate::providers::agent::registry as provider_registry;
use crate::providers::credentials::Credentials;
use crate::providers::wire::{Message, MsgBlock, MsgRole};
use crate::providers::{
    AgentProvider, AgentSession, CompletionRequest, Detection, PermissionMode, ProviderDeps,
    ProviderError, ProviderSpec, StartContext, ThinkingLevel, UserInput,
};
use crate::secrets::MemorySecretStore;
use crate::sessions::context::{render_documents, ContextAssembly, ContextSource};
use crate::sessions::event::{
    now, render_tail, Block, ContextReason, Decision, EventKind, QuestionAnswer, Role,
    SessionEvent, SessionState, TitleSource,
};
use crate::sessions::pump::{self, InjectedEvent, SessionBus};
use crate::sessions::title::{
    first_message_title, sanitize_title, title_prompt, TITLE_MAX_TOKENS, TITLE_SYSTEM_PROMPT,
};
use crate::settings::{suggest_title_model, SettingsService, TitleModel};
use crate::store::{Store, StoreError};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

/// A running session: the agent handle plus the task draining its events.
struct LiveSession {
    /// Serializes every command to this session.
    commands: Mutex<Box<dyn AgentSession>>,
    /// The pump is the single writer for this session, so manager-authored events go through it.
    inject: tokio::sync::mpsc::UnboundedSender<InjectedEvent>,
    /// Taken and joined by [`SessionManager::stop`] before it writes the final `exited`.
    pump: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("session is not running: {0} (resume it first)")]
    NotRunning(String),
    #[error("{0}")]
    Conflict(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The only mapping from runtime errors to the wire error codes the UI switches on.
impl From<SessionError> for IpcError {
    fn from(error: SessionError) -> Self {
        let message = error.to_string();
        match error {
            SessionError::NotFound(_) => IpcError::not_found(message),
            SessionError::UnknownProvider(_) | SessionError::Invalid(_) => {
                IpcError::invalid(message)
            }
            SessionError::Conflict(_) => IpcError::conflict(message),
            SessionError::NotRunning(_) => IpcError::unavailable(message),
            SessionError::Provider(ProviderError::Unavailable(_)) => IpcError::unavailable(message),
            SessionError::Provider(ProviderError::Unsupported(_)) => IpcError::unavailable(message),
            SessionError::Provider(_) => IpcError::invalid(message),
            SessionError::Store(_) | SessionError::Io(_) => IpcError::io(message),
        }
    }
}

/// How many transcript lines a stopped session replays as the new provider's opening prompt. A live
/// switch hands over the normalized history instead; this is the fallback when there is none.
pub const TRANSCRIPT_SUMMARY_LINES: usize = 24;

/// The longest a session title may be: long enough for a sentence, short enough for a tree row.
pub const SESSION_TITLE_MAX_CHARS: usize = 80;

/// A session created with no title keeps this until its first message arrives. It is deliberately a
/// real title rather than an empty string: the tree, tabs and window need something to draw.
pub const TEMPORARY_SESSION_TITLE: &str = "New session";

/// Owns every live session and the store they persist into. One instance per application.
pub struct SessionManager {
    store: Arc<Store>,
    deps: ProviderDeps,
    providers: Vec<Arc<dyn AgentProvider>>,
    /// Pinned specs and the plan. Absent in tests and previews that never assemble knowledge: a
    /// session then starts with an empty context rather than failing.
    context: std::sync::Mutex<Option<Arc<dyn ContextSource>>>,
    live: Mutex<HashMap<String, Arc<LiveSession>>>,
    /// Sessions with a title completion in flight. A second request is refused instead of spending a
    /// second provider call on the same conversation.
    title_jobs: Mutex<HashSet<String>>,
    bus: SessionBus,
}

impl SessionManager {
    pub fn new(
        store: Arc<Store>,
        deps: ProviderDeps,
        providers: Vec<Arc<dyn AgentProvider>>,
    ) -> Self {
        let (bus, _) = broadcast::channel(512);
        Self {
            store,
            deps,
            providers,
            context: std::sync::Mutex::new(None),
            live: Mutex::new(HashMap::new()),
            title_jobs: Mutex::new(HashSet::new()),
            bus,
        }
    }

    /// Attach the knowledge source. `GroveCore` wires the real one; tests attach a stub or a service
    /// pointed at a tempdir project.
    pub fn set_context_source(&self, source: Arc<dyn ContextSource>) {
        let mut slot = self
            .context
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(source);
    }

    /// Open the durable store at `paths.database` and wire the built-in providers. Sessions left live
    /// by a previous run are marked exited: after a restart they are conversations, not processes.
    ///
    /// Keyless: a preview or a test wants no keychain prompt. The app calls
    /// [`Self::open_with_secrets`] with the OS keychain, because a session reads its provider key from
    /// the same port the settings pane writes.
    pub fn open(paths: AppPaths) -> Result<Self, SessionError> {
        let store = Arc::new(Store::open(&paths.database)?);
        store.mark_orphaned_sessions_exited()?;
        Ok(Self::new(store, Self::deps(paths), provider_registry()))
    }

    /// Open the durable store with a specific secret store, so a session authenticates with the key
    /// the user saved. `GroveCore::init` passes the OS keychain here; nothing else in the runtime
    /// knows which implementation it got.
    pub fn open_with_secrets(
        paths: AppPaths,
        secrets: Arc<dyn crate::secrets::SecretStore>,
    ) -> Result<Self, SessionError> {
        let store = Arc::new(Store::open(&paths.database)?);
        store.mark_orphaned_sessions_exited()?;
        Ok(Self::new(
            store,
            Self::deps_with_secrets(paths, secrets),
            provider_registry(),
        ))
    }

    /// The secret store a session authenticates with. A test uses it to prove the manager and the
    /// settings pane share one store rather than two.
    pub fn secrets(&self) -> Arc<dyn crate::secrets::SecretStore> {
        Arc::clone(&self.deps.secrets)
    }

    /// In-memory runtime for previews and tests that do not care about the file.
    pub fn in_memory(paths: &AppPaths) -> Result<Self, SessionError> {
        let store = Arc::new(Store::open_in_memory()?);
        Ok(Self::new(
            store,
            Self::deps(paths.clone()),
            provider_registry(),
        ))
    }

    /// A durable runtime whose only provider is the offline one. Previews and integration tests use
    /// this: the session pump, transcript and cancellation paths are real, the model is a script.
    pub fn open_offline(paths: AppPaths) -> Result<Self, SessionError> {
        let store = Arc::new(Store::open(&paths.database)?);
        store.mark_orphaned_sessions_exited()?;
        Ok(Self::new(
            store,
            Self::deps(paths),
            crate::providers::offline::registry(),
        ))
    }

    /// The in-memory form of [`Self::open_offline`].
    pub fn in_memory_offline(paths: &AppPaths) -> Result<Self, SessionError> {
        let store = Arc::new(Store::open_in_memory()?);
        Ok(Self::new(
            store,
            Self::deps(paths.clone()),
            crate::providers::offline::registry(),
        ))
    }

    fn deps(paths: AppPaths) -> ProviderDeps {
        Self::deps_with_secrets(paths, Arc::new(MemorySecretStore::new()))
    }

    fn deps_with_secrets(
        paths: AppPaths,
        secrets: Arc<dyn crate::secrets::SecretStore>,
    ) -> ProviderDeps {
        ProviderDeps {
            paths,
            http: crate::providers::http_client(),
            secrets,
        }
    }

    /// The shared store. Domains that own their own tables (settings, plans) go through this
    /// instead of opening a second connection to the same file.
    pub fn store(&self) -> Arc<Store> {
        Arc::clone(&self.store)
    }

    /// The event bus the shell forwards as `grove://session-event`.
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope<SessionEvent>> {
        self.bus.subscribe()
    }

    /// Every registered provider, with its models and auth, for the session picker and settings.
    pub fn providers(&self) -> Vec<ProviderSpec> {
        self.providers
            .iter()
            .map(|provider| provider.spec().clone())
            .collect()
    }

    /// Every provider's readiness: is a credential configured, and which model would it use.
    pub fn detect_all(&self) -> Vec<Detection> {
        self.providers
            .iter()
            .map(|provider| provider.detect(&self.deps))
            .collect()
    }

    /// Credential readiness for every provider, as the settings pane shows it.
    pub fn provider_statuses(&self) -> Vec<ProviderStatus> {
        Credentials::new(Arc::clone(&self.deps.secrets)).statuses()
    }

    /// The provider's remaining plan usage, or `None` for a provider without a plan endpoint.
    pub async fn plan_usage(&self, provider: &str) -> Result<Option<PlanUsage>, SessionError> {
        Ok(crate::providers::usage::plan_usage(provider, &self.deps).await?)
    }

    /// The provider a request did not name: the first one with a usable credential, else the first
    /// catalogued provider. A new session then fails on a *named* provider with an actionable
    /// "add a key" message instead of a mystery.
    fn default_provider(&self) -> Option<String> {
        self.providers
            .iter()
            .find(|provider| provider.detect(&self.deps).installed)
            .or_else(|| self.providers.first())
            .map(|provider| provider.id().to_string())
    }

    /// The display name a provider advertises (`Anthropic`, `OpenAI`, …), or `None` for one this
    /// build does not register.
    pub fn label_for(&self, provider: &str) -> Option<&str> {
        self.providers
            .iter()
            .find(|candidate| candidate.id() == provider)
            .map(|candidate| candidate.spec().name.as_str())
    }

    pub fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
        Ok(self.store.list_sessions()?)
    }

    pub fn get(&self, id: &str) -> Result<SessionDetail, SessionError> {
        self.store
            .session_detail(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))
    }

    /// Create the row, start the agent, and begin pumping. The transcript is durable from here on.
    pub async fn create(
        &self,
        request: CreateSessionRequest,
    ) -> Result<SessionDetail, SessionError> {
        let mut request = request;
        if request.provider.trim().is_empty() {
            // "Whatever works": the first provider with a credential, else the first catalogued one,
            // so an unnamed session still gets a precise error instead of an empty picker value.
            match self.default_provider() {
                Some(provider) => request.provider = provider,
                None => {
                    return Err(SessionError::Invalid("no providers are registered".into()));
                }
            }
        }
        let provider = self.provider(&request.provider)?;
        let id = Uuid::new_v4().to_string();
        let at = now();
        // A project session runs in its chosen worktree, or in the project folder when none was
        // named. A loose chat keeps the old cwd rule so it can still run anywhere.
        let project =
            match request
                .project_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
            {
                Some(project_id) => Some(self.store.project(project_id)?.ok_or_else(|| {
                    SessionError::Invalid(format!("unknown project: {project_id}"))
                })?),
                None => None,
            };
        let cwd = match &project {
            Some(project) => match request
                .worktree_path
                .as_deref()
                .map(str::trim)
                .filter(|path| !path.is_empty())
            {
                Some(path) => {
                    let candidate = PathBuf::from(path);
                    if !candidate.is_dir() {
                        return Err(SessionError::Invalid(format!(
                            "worktree folder does not exist: {path}"
                        )));
                    }
                    candidate
                }
                None => PathBuf::from(&project.path),
            },
            None => resolve_cwd(request.cwd.as_deref(), &self.deps.paths),
        };
        let model = non_empty(request.model);
        // A blank title is not a validation error: it asks the session to name itself from its first
        // message. The row still needs a title to draw, so it starts with the temporary one.
        let (title, title_source) = match non_empty(request.title) {
            Some(title) => (title, TitleSource::Manual),
            None => (TEMPORARY_SESSION_TITLE.to_string(), TitleSource::Auto),
        };
        // The request's own config is what the row keeps and what the launch uses.
        let config = request.config.unwrap_or_else(|| serde_json::json!({}));

        let session = Session {
            id: id.clone(),
            provider: request.provider,
            cwd: display_path(&cwd),
            model: model.clone(),
            title,
            title_source,
            status: SessionState::Starting,
            project_id: project.map(|project| project.id),
            awaiting: false,
            archived_at: None,
            created_at: at.clone(),
            updated_at: at,
        };
        self.store.insert_session(&session, &config)?;

        // A session is created with no pins, so this is usually empty; going through the same assembly
        // keeps "a start always carries the current knowledge" true for every caller.
        let assembly = self.context_assembly(&id);
        let context = StartContext {
            cwd,
            model,
            session_id: id.clone(),
            prompt: None,
            resume_token: None,
            config: config.clone(),
            context_files: assembly.docs.clone(),
            history: Vec::new(),
        };
        self.publish_context(&session, ContextReason::Start, &assembly, None);
        if let Err(error) = self.launch(&provider, &session, context).await {
            let _ = self.store.update_session_status(&id, SessionState::Error);
            return Err(error);
        }
        self.get(&id)
    }

    /// Send a user message. The message is persisted before the adapter is asked, so the transcript
    /// keeps what the user typed even if the provider dies mid-send.
    pub async fn send(&self, id: &str, text: &str) -> Result<(), SessionError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(SessionError::Invalid("message is empty".into()));
        }
        let live = self.live_session(id).await?;
        // Hold the command lock across both steps: the transcript order and the order the provider
        // sees commands are then the same, even when two sends race.
        let mut handle = live.commands.lock().await;

        // An auto session is still wearing its temporary title: the first thing the user says is the
        // best title available until a model refines it. This persists the row and announces the
        // change before the message, so the tree never shows a stale "New session".
        if let Some(session) = self.store.session(id)? {
            if session.title_source == TitleSource::Auto && session.title == TEMPORARY_SESSION_TITLE
            {
                self.record_title_through(
                    id,
                    &first_message_title(text),
                    TitleSource::Auto,
                    Some(&live),
                )
                .await?;
            }
        }

        let (message, persisted) =
            InjectedEvent::acknowledged(SessionEvent::new(EventKind::Message {
                role: Role::User,
                blocks: vec![Block::Text {
                    text: text.to_string(),
                }],
            }));
        live.inject
            .send(message)
            .map_err(|_| SessionError::NotRunning(id.to_string()))?;
        // Wait until the message is durable before the provider can react to it.
        let _ = persisted.await;

        handle.send(UserInput::text(text)).await?;
        self.mirror_resume_token(id, &**handle)?;
        Ok(())
    }

    pub async fn cancel(&self, id: &str) -> Result<(), SessionError> {
        let live = self.live_session(id).await?;
        let mut handle = live.commands.lock().await;
        handle.cancel().await?;
        self.mirror_resume_token(id, &**handle)?;
        Ok(())
    }

    pub async fn respond_permission(
        &self,
        id: &str,
        request_id: &str,
        decision: Decision,
    ) -> Result<(), SessionError> {
        let live = self.live_session(id).await?;
        let mut handle = live.commands.lock().await;
        handle.respond_permission(request_id, decision).await?;
        Ok(())
    }

    pub async fn respond_question(
        &self,
        id: &str,
        request_id: &str,
        answers: Vec<QuestionAnswer>,
    ) -> Result<(), SessionError> {
        let live = self.live_session(id).await?;
        let mut handle = live.commands.lock().await;
        handle.respond_question(request_id, answers).await?;
        Ok(())
    }

    /// Change the model for the next turn, and optionally the thinking level that goes with it.
    ///
    /// A live session keeps its process, its conversation and any parked permission: the agent only
    /// swaps the model its next request names (and, when supplied, the level it asks for). A stopped
    /// session just records the pair for the next resume. There is no transcript note; the composer's
    /// toast is the feedback.
    pub async fn set_model(
        &self,
        id: &str,
        model: &str,
        thinking: Option<ThinkingLevel>,
    ) -> Result<SessionDetail, SessionError> {
        let model = model.trim();
        if model.is_empty() {
            return Err(SessionError::Invalid("model is empty".into()));
        }
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        if let Some(live) = self.live.lock().await.get(id).cloned() {
            let mut handle = live.commands.lock().await;
            handle.set_model(model.to_string()).await?;
            if let Some(level) = thinking {
                handle.set_thinking(level).await?;
            }
        }
        self.store.set_session_model(id, model, thinking)?;
        self.get(id)
    }

    /// Change the reasoning level for the next turn. The setting is persisted in the session config,
    /// so a stopped session picks it up when it is resumed; a live session gets it immediately.
    pub async fn set_thinking(
        &self,
        id: &str,
        level: ThinkingLevel,
    ) -> Result<SessionDetail, SessionError> {
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        let mut config = self.store.adapter_config(id)?;
        if !config.is_object() {
            config = serde_json::json!({});
        }
        config["thinkingLevel"] = serde_json::json!(level.as_str());
        self.store.set_adapter_config(id, &config)?;

        if let Ok(live) = self.live_session(id).await {
            let mut handle = live.commands.lock().await;
            handle.set_thinking(level).await?;
        }
        self.get(id)
    }

    /// Change whether mutating tools ask before they run. The mode is persisted in the session
    /// config, so a stopped session picks it up on resume; a live session applies it from its next
    /// tool call, without cancelling a parked request or a running turn.
    pub async fn set_permission_mode(
        &self,
        id: &str,
        mode: PermissionMode,
    ) -> Result<SessionDetail, SessionError> {
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        if let Ok(live) = self.live_session(id).await {
            let mut handle = live.commands.lock().await;
            handle.set_permission_mode(mode).await?;
        }
        let mut config = self.store.adapter_config(id)?;
        if !config.is_object() {
            config = serde_json::json!({});
        }
        config["permissionMode"] = serde_json::json!(mode.as_str());
        self.store.set_adapter_config(id, &config)?;
        self.get(id)
    }

    /// Rename a session. Only the title and its provenance change: the agent, its config and the
    /// transcript are untouched, so a stopped session is renamed without being resumed.
    ///
    /// The rules live here rather than in the command and the row, so every caller — the context
    /// menu today, later automation — accepts exactly the same titles. A blank title is refused
    /// rather than defaulted: the row cancels an empty edit before calling.
    ///
    /// A rename is always `manual`, which also removes the session from automatic title generation,
    /// and `title_changed` patches the tree without a reload.
    pub async fn rename(&self, id: &str, title: &str) -> Result<Session, SessionError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(SessionError::Invalid("title is empty".into()));
        }
        if title.chars().count() > SESSION_TITLE_MAX_CHARS {
            return Err(SessionError::Invalid(format!(
                "title is longer than {SESSION_TITLE_MAX_CHARS} characters"
            )));
        }
        if title.chars().any(char::is_control) {
            return Err(SessionError::Invalid(
                "title contains control characters".into(),
            ));
        }
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        self.record_title(id, title, TitleSource::Manual).await?;
        self.store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))
    }

    /// Name a session with one small model call outside the agent loop.
    ///
    /// The model is the explicit global choice, else the suggested model of the first configured
    /// provider. The transcript supplies the first exchange; nothing else from it is sent. A failure
    /// is reported unchanged and leaves the row exactly as it was, so the placeholder survives for a
    /// retry or a manual rename.
    ///
    /// Only one generation may run per session at a time: the second caller is refused rather than
    /// spending another provider call on the same transcript.
    ///
    /// `force` separates the explicit "Generate name with AI" action from the automatic first-reply
    /// trigger. A non-forced call never overwrites a manual title, including one typed while the
    /// completion was already in flight, and it is silent when automatic naming is off.
    pub async fn generate_title(&self, id: &str, force: bool) -> Result<Session, SessionError> {
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        {
            let mut jobs = self.title_jobs.lock().await;
            if !jobs.insert(id.to_string()) {
                return Err(SessionError::Invalid(
                    "a title is already being generated for this session".into(),
                ));
            }
        }
        let result = self.generate_title_inner(id, force).await;
        self.title_jobs.lock().await.remove(id);
        result
    }

    async fn generate_title_inner(&self, id: &str, force: bool) -> Result<Session, SessionError> {
        let before = self
            .store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        // Nothing to do for an automatic attempt that meets a user-authored title, or when the user
        // turned automatic naming off. Both leave the stored row exactly as it is.
        if !force {
            if before.title_source == TitleSource::Manual {
                return Ok(before);
            }
            let settings = SettingsService::new(Arc::clone(&self.store))
                .get()
                .map_err(|error| SessionError::Invalid(error.to_string()))?;
            if !settings.auto_titles {
                return Ok(before);
            }
        }
        let title_model = self.title_model(!force)?;
        let events = self.store.session_events(id)?;
        let user = message_text(&events, Role::User).ok_or_else(|| {
            SessionError::Invalid("session has no user message to name it from".into())
        })?;
        let assistant = message_text(&events, Role::Assistant)
            .ok_or_else(|| SessionError::Invalid("session has no assistant reply yet".into()))?;

        let provider = self.provider(&title_model.provider)?;
        let answer = provider
            .complete(
                CompletionRequest {
                    model: title_model.model.clone(),
                    system: TITLE_SYSTEM_PROMPT.to_string(),
                    prompt: title_prompt(&user, &assistant),
                    max_tokens: TITLE_MAX_TOKENS,
                },
                self.deps.clone(),
            )
            .await?;
        let title = sanitize_title(&answer).ok_or_else(|| {
            SessionError::Invalid("the title model returned an empty title".into())
        })?;
        // The completion may have raced a rename. Re-read the row and leave a manual title alone,
        // returning what the user just wrote rather than the generated answer.
        let current = self
            .store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        if !force && current.title_source == TitleSource::Manual {
            return Ok(current);
        }
        self.record_title(id, &title, TitleSource::Generated)
            .await?;
        self.store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))
    }

    /// The explicit title model, else the suggested one. The suggestion is never persisted here:
    /// Settings shows it and the save persists whatever the form shows.
    ///
    /// `automatic` is false for the explicit menu action: turning automatic names off must not make
    /// the user's deliberate "Generate name with AI" fail.
    fn title_model(&self, automatic: bool) -> Result<TitleModel, SessionError> {
        let settings = SettingsService::new(Arc::clone(&self.store))
            .get()
            .map_err(|error| SessionError::Invalid(error.to_string()))?;
        if automatic && !settings.auto_titles {
            return Err(SessionError::Invalid(
                "automatic session names are off in Settings → General".into(),
            ));
        }
        if let Some(model) = settings.title_model {
            return Ok(model);
        }
        suggest_title_model(
            &self.provider_statuses(),
            &std::collections::BTreeMap::new(),
        )
        .ok_or_else(|| {
            SessionError::Invalid(
                "no provider is configured to name sessions; add a key in Settings → Providers"
                    .into(),
            )
        })
    }

    /// Persist a new title and announce it. A live session's pump is the writer, so the event lands in
    /// the transcript in order and reaches the window immediately; a stopped session has no pump, so
    /// the manager publishes directly (the same rule `mark` follows after a stop).
    async fn record_title(
        &self,
        id: &str,
        title: &str,
        source: TitleSource,
    ) -> Result<(), SessionError> {
        let live = { self.live.lock().await.get(id).cloned() };
        self.record_title_through(id, title, source, live.as_deref())
            .await
    }

    /// [`Self::record_title`] for a caller that already holds the live handle. Passing it in avoids
    /// taking the manager's live-session lock while the caller holds that session's command lock,
    /// which would invert the order `stop` uses.
    async fn record_title_through(
        &self,
        id: &str,
        title: &str,
        source: TitleSource,
        live: Option<&LiveSession>,
    ) -> Result<(), SessionError> {
        self.store.set_session_title(id, title, source)?;
        let event = SessionEvent::new(EventKind::TitleChanged {
            title: title.to_string(),
            source,
        });
        match live {
            Some(live) => {
                let (injected, persisted) = InjectedEvent::acknowledged(event.clone());
                if live.inject.send(injected).is_ok() {
                    // Wait for the pump to append it, so a caller that reads the transcript right
                    // after the command sees the same title the tree received.
                    let _ = persisted.await;
                } else {
                    pump::publish(&self.store, &self.bus, id, &event);
                }
            }
            None => pump::publish(&self.store, &self.bus, id, &event),
        }
        Ok(())
    }

    /// Stop the process but keep the row and the transcript. The session can be resumed later.
    pub async fn close(&self, id: &str) -> Result<(), SessionError> {
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        self.stop(id).await;
        self.mark(id, SessionState::Exited)
    }

    /// Hide a session from the active tree without losing it.
    ///
    /// A running agent is stopped first through [`Self::stop`], which is the single exit path: it
    /// publishes `Exited` and clears `awaiting`, so archiving a session with a parked permission never
    /// leaves the "needs you" badge behind. Nothing is deleted; the transcript, plan, pins and
    /// workers stay attached to the row.
    pub async fn archive(&self, id: &str) -> Result<Session, SessionError> {
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        // Never hold `live.commands` across this call: `stop` takes the same lock to shut the handle
        // down. Nothing here locks the session before asking to stop it.
        self.stop(id).await;
        self.store.set_session_archived(id, Some(&now()))?;
        self.store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))
    }

    /// Put an archived session back in the active tree. The agent stays stopped; the next send (or
    /// an explicit resume) starts it, exactly like any other stopped session.
    pub async fn restore(&self, id: &str) -> Result<Session, SessionError> {
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        self.store.set_session_archived(id, None)?;
        self.store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))
    }

    /// Delete a session permanently: stop a live agent, then remove the row. `events`, `plans`,
    /// `pins` and `subagents` cascade with it.
    pub async fn delete(&self, id: &str) -> Result<(), SessionError> {
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        self.stop(id).await;
        self.store.delete_session(id)?;
        Ok(())
    }

    /// Restart the adapter for an existing conversation: with its resume token when the adapter
    /// supports resume, otherwise with the transcript tail replayed as context.
    pub async fn resume(&self, id: &str) -> Result<Session, SessionError> {
        let session = self
            .store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        let provider = self.provider(&session.provider)?;
        let token = self.store.resume_token(id)?;
        // Re-resolve on every resume, so a provider path changed in Settings applies to the next
        // start of an existing conversation instead of only to brand-new sessions.
        let config = self.store.adapter_config(id)?;
        let events = self.store.session_events(id)?;
        self.stop(id).await;

        let assembly = self.context_assembly(id);
        let mut context = StartContext {
            cwd: PathBuf::from(&session.cwd),
            model: session.model.clone(),
            session_id: session.id.clone(),
            prompt: None,
            resume_token: None,
            config,
            context_files: assembly.docs.clone(),
            history: Vec::new(),
        };
        if provider.capabilities().resume && token.is_some() {
            context.resume_token = token;
        } else if !events.is_empty() {
            context.prompt = Some(render_tail(&events, 12));
        }

        self.mark(id, SessionState::Starting)?;
        self.publish_context(&session, ContextReason::Resume, &assembly, None);
        self.launch(&provider, &session, context).await?;
        Ok(self.store.session(id)?.unwrap_or(session))
    }

    /// Re-read the pins and the plan and push the current knowledge into the running session.
    ///
    /// The context is a snapshot taken at start: nothing re-reads a spec on its own. This is the
    /// explicit way to move it, and it is visible twice — the provider receives the rendered documents
    /// as a turn, and the transcript records what was sent.
    pub async fn refresh_context(&self, id: &str) -> Result<ContextAssembly, SessionError> {
        let live = self.live_session(id).await?;
        let session = self
            .store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        let assembly = self.context_assembly(id);

        // Hold the command lock across both steps, exactly like `send`: the transcript note and the
        // turn the provider sees must be in the same order for every caller.
        let mut handle = live.commands.lock().await;
        if let Some(event) = context_event(&session, ContextReason::Refresh, &assembly, None) {
            inject(&live, event, id).await?;
        }
        if !assembly.docs.is_empty() {
            handle
                .send(UserInput::text(render_documents(&assembly.docs)))
                .await?;
        }
        self.mirror_resume_token(id, &**handle)?;
        Ok(assembly)
    }

    /// Re-launch the same conversation on another provider or model.
    ///
    /// A **live, idle** session hands its whole normalized conversation to the new agent, so the
    /// provider change is lossless: the transcript is the record and stays untouched, and the new
    /// agent's first request carries the same messages. A stopped session has no in-process history
    /// left, so it falls back to replaying a transcript summary. A switch while a turn is running or
    /// a permission/question is parked is refused with a conflict: the shell queues it and retries
    /// when the session is idle, because stopping to relaunch would cancel the turn or discard the
    /// pending request. A real provider change adds a visible system note; a same-provider model
    /// change is just the composer's picker and does not add a context note. The resume token belongs
    /// to the old provider (and model), so it is dropped rather than handed over.
    pub async fn switch_provider(
        &self,
        id: &str,
        provider: &str,
        model: Option<String>,
    ) -> Result<SessionDetail, SessionError> {
        let session = self
            .store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        let same_provider = session.provider == provider;
        let model = match non_empty(model) {
            Some(model) => Some(model),
            None if same_provider => session.model.clone(),
            None => None,
        };
        if same_provider && model == session.model {
            return Err(SessionError::Invalid(format!(
                "session {id} already runs {provider} with that model"
            )));
        }
        let provider_id = provider.to_string();
        let provider = self.provider(&provider_id)?;
        // Capture the conversation before the stop, and only when it is safe to stop. The row is the
        // pump's view of the agent, so a busy turn or a parked request is visible here without
        // touching the command mutex (which a running send holds).
        let handoff = match self.live.lock().await.get(id).cloned() {
            Some(live) => {
                if session.awaiting || session.status == SessionState::Busy {
                    let state = if session.awaiting {
                        "waiting for an answer"
                    } else {
                        "running a turn"
                    };
                    return Err(SessionError::Conflict(format!(
                        "session {id} is {state}; switch providers when it is idle"
                    )));
                }
                let mut handle = live.commands.lock().await;
                trim_to_complete_assistant(handle.history().await)
            }
            None => Vec::new(),
        };
        let previous = session.provider.clone();
        let config = self.store.adapter_config(id)?;
        let events = self.store.session_events(id)?;
        let assembly = self.context_assembly(id);
        let summary = render_tail(&events, TRANSCRIPT_SUMMARY_LINES);

        self.stop(id).await;
        // A model name belongs to the provider that understood it, and so does the resume token.
        self.store
            .set_session_provider(id, &provider_id, model.as_deref())?;
        self.store.set_resume_token(id, None)?;
        let session = self.store.session(id)?.unwrap_or(session);
        self.mark(id, SessionState::Starting)?;
        if !same_provider {
            // A provider switch changes the wire client, so it is worth a transcript note. A
            // same-provider model change is not a context operation: it re-launches with the same
            // documents, but the user only asked to change the model. Publishing a refresh note
            // there produced a "no documents" notice on every picker change.
            self.publish_context(
                &session,
                ContextReason::ProviderSwitch,
                &assembly,
                Some((previous.as_str(), provider_id.as_str())),
            );
        }

        let context = StartContext {
            cwd: PathBuf::from(&session.cwd),
            model: session.model.clone(),
            session_id: session.id.clone(),
            prompt: (handoff.is_empty() && !summary.is_empty()).then_some(summary),
            resume_token: None,
            config,
            context_files: assembly.docs,
            history: handoff,
        };
        // A provider that refuses to start leaves the row on the new provider and the session stopped:
        // the conversation is intact and can be resumed (or switched again).
        if let Err(error) = self.launch(&provider, &session, context).await {
            let _ = self.store.update_session_status(id, SessionState::Error);
            return Err(error);
        }
        self.get(id)
    }

    /// The knowledge a fresh start of this session would receive right now.
    fn context_assembly(&self, session_id: &str) -> ContextAssembly {
        let source = self
            .context
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        match source {
            Some(source) => source.documents(session_id),
            None => ContextAssembly::default(),
        }
    }

    /// Record the context a session now carries. Called only when the session has no pump yet (start,
    /// resume, switch: all after `stop`), so writing directly keeps the one-writer rule.
    fn publish_context(
        &self,
        session: &Session,
        reason: ContextReason,
        assembly: &ContextAssembly,
        switch: Option<(&str, &str)>,
    ) {
        if let Some(event) = context_event(session, reason, assembly, switch) {
            pump::publish(&self.store, &self.bus, &session.id, &event);
        }
    }

    async fn launch(
        &self,
        provider: &Arc<dyn AgentProvider>,
        session: &Session,
        context: StartContext,
    ) -> Result<(), SessionError> {
        let mut handle = provider.start(context, self.deps.clone()).await?;
        // The adapter may already know the provider's conversation id; mirror it now, and again after
        // every command, so a restart has the freshest token the protocol offered.
        if let Some(token) = handle.resume_token() {
            self.store.set_resume_token(&session.id, Some(&token))?;
        }
        let events = handle.take_events();
        // A process-backed adapter learns its conversation id after this point; the pump mirrors the
        // shared slot so the row is current without waiting for another command.
        let resume_token = handle.resume_token_slot();
        let (inject, injected) = tokio::sync::mpsc::unbounded_channel();
        let pump = pump::spawn(
            Arc::clone(&self.store),
            self.bus.clone(),
            session.id.clone(),
            events,
            injected,
            resume_token,
        );
        let live = Arc::new(LiveSession {
            commands: Mutex::new(handle),
            inject,
            pump: Mutex::new(Some(pump)),
        });
        let displaced = self.live.lock().await.insert(session.id.clone(), live);
        if let Some(previous) = displaced {
            if let Some(pump) = previous.pump.lock().await.take() {
                pump.abort();
            }
        }
        // A launched agent is ready for its first message. Without this transition the row stays
        // `starting`, and a composer that queues while a turn is in flight never sends anything.
        self.mark(&session.id, SessionState::Ready)?;
        Ok(())
    }

    /// Stop the agent and leave the row `exited` with `awaiting` clear.
    ///
    /// Every stop path — `close`, `resume`, `switch_provider`, and the archive/delete stop to come —
    /// goes through here. The pump is aborted before it can drain the adapter's last events, so the
    /// final transition is written directly once its task has finished: a parked permission or
    /// question must never survive its process and leave the tree's "needs you" badge stuck.
    async fn stop(&self, id: &str) {
        if let Some(live) = self.live.lock().await.remove(id) {
            {
                let mut handle = live.commands.lock().await;
                let _ = handle.shutdown().await;
            }
            // Join, not just abort: a queued permission event published after the exit would re-arm
            // `awaiting`. Once the task is gone, `mark` is the session's only writer.
            if let Some(pump) = live.pump.lock().await.take() {
                pump.abort();
                let _ = pump.await;
            }
            let _ = self.mark(id, SessionState::Exited);
        }
        // Belt and braces for a row whose pump died without publishing: nothing is running after a
        // stop, so nothing can be waiting for an answer.
        let _ = self.store.update_session_awaiting(id, false);
    }

    /// Record a transition the adapter did not emit (stop, restart), exactly once.
    ///
    /// Callers run this only after [`SessionManager::stop`], when the session has no pump: writing
    /// directly is what keeps the "one writer per session" invariant.
    fn mark(&self, id: &str, state: SessionState) -> Result<(), SessionError> {
        let Some(session) = self.store.session(id)? else {
            return Err(SessionError::NotFound(id.to_string()));
        };
        if session.status == state {
            return Ok(());
        }
        pump::publish(
            &self.store,
            &self.bus,
            id,
            &SessionEvent::new(EventKind::Status { state }),
        );
        Ok(())
    }

    fn mirror_resume_token(&self, id: &str, handle: &dyn AgentSession) -> Result<(), SessionError> {
        if let Some(token) = handle.resume_token() {
            self.store.set_resume_token(id, Some(&token))?;
        }
        Ok(())
    }

    async fn live_session(&self, id: &str) -> Result<Arc<LiveSession>, SessionError> {
        if let Some(live) = self.live.lock().await.get(id).cloned() {
            return Ok(live);
        }
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        Err(SessionError::NotRunning(id.to_string()))
    }

    fn provider(&self, provider_id: &str) -> Result<Arc<dyn AgentProvider>, SessionError> {
        self.providers
            .iter()
            .find(|candidate| candidate.id() == provider_id)
            .cloned()
            .ok_or_else(|| SessionError::UnknownProvider(provider_id.to_string()))
    }
}

/// The visible system note for a context event, or `None` when there is nothing worth recording.
///
/// A start or resume with no pins and no plan stays silent (the header shows an empty context); an
/// explicit refresh or a provider switch always says what happened, because the user asked for it.
fn context_event(
    session: &Session,
    reason: ContextReason,
    assembly: &ContextAssembly,
    switch: Option<(&str, &str)>,
) -> Option<SessionEvent> {
    let explicit = matches!(
        reason,
        ContextReason::Refresh | ContextReason::ProviderSwitch
    );
    if assembly.docs.is_empty() && !explicit {
        return None;
    }
    Some(SessionEvent::new(EventKind::ContextRefreshed {
        reason,
        provider: session.provider.clone(),
        documents: assembly.documents.clone(),
        note: context_note(reason, &session.provider, assembly, switch),
    }))
}

fn context_note(
    reason: ContextReason,
    provider: &str,
    assembly: &ContextAssembly,
    switch: Option<(&str, &str)>,
) -> String {
    let summary = context_summary(assembly);
    match reason {
        ContextReason::Start => format!("Context sent to {provider}: {summary}"),
        ContextReason::Resume => format!("Context re-sent on resume to {provider}: {summary}"),
        ContextReason::Refresh => format!("Context refreshed for {provider}: {summary}"),
        ContextReason::ProviderSwitch => {
            let (from, to) = switch.unwrap_or(("?", provider));
            format!("provider switched from {from} to {to}; context re-sent: {summary}")
        }
    }
}

fn context_summary(assembly: &ContextAssembly) -> String {
    if assembly.docs.is_empty() {
        return "no documents".to_string();
    }
    let count = assembly.docs.len();
    let plural = if count == 1 { "" } else { "s" };
    let mut summary = format!("{count} document{plural} ({} chars)", assembly.total_chars);
    let truncated = assembly
        .documents
        .iter()
        .filter(|document| document.truncated)
        .count();
    if truncated > 0 {
        summary.push_str(&format!("; {truncated} truncated to fit"));
    }
    if assembly.omitted > 0 {
        summary.push_str(&format!("; {} omitted", assembly.omitted));
    }
    summary
}

/// Persist and broadcast one manager-authored event through the pump, waiting until it is durable.
async fn inject(
    live: &LiveSession,
    event: SessionEvent,
    session_id: &str,
) -> Result<(), SessionError> {
    let (injected, persisted) = InjectedEvent::acknowledged(event);
    live.inject
        .send(injected)
        .map_err(|_| SessionError::NotRunning(session_id.to_string()))?;
    // A dropped ack means the pump is gone; the event is already on its way out either way.
    let _ = persisted.await;
    Ok(())
}

fn resolve_cwd(requested: Option<&str>, paths: &AppPaths) -> PathBuf {
    requested
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| paths.app_data.clone())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The first non-empty text of a message with `role` in the transcript. Code and thinking blocks are
/// not what the user or the assistant said, so they do not become title material.
fn message_text(events: &[SessionEvent], role: Role) -> Option<String> {
    events.iter().find_map(|event| {
        let EventKind::Message {
            role: found,
            blocks,
        } = &event.kind
        else {
            return None;
        };
        if *found != role {
            return None;
        }
        let text = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    })
}

/// Cut a conversation at its last complete assistant message before handing it to another provider.
///
/// A message that asks for a tool is not complete: the tool results and the reply they produce are
/// the rest of that turn. A cancelled or failed turn can leave such a dangling call (or its orphaned
/// user message) at the end of the history; no other provider can be given the second half of a turn
/// it never ran, so the handoff stops at the last reply that finished.
fn trim_to_complete_assistant(history: Vec<Message>) -> Vec<Message> {
    let end = history.iter().rposition(|message| {
        message.role == MsgRole::Assistant
            && !message
                .blocks
                .iter()
                .any(|block| matches!(block, MsgBlock::ToolUse { .. }))
    });
    match end {
        Some(index) => history.into_iter().take(index + 1).collect(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ErrorCode;
    use crate::providers::catalog;
    use crate::providers::wire::{Message, MsgBlock};
    use crate::providers::{AuthMethod, Capabilities, Detection, ProviderSpec};
    use crate::sessions::title::TITLE_MAX_CHARS;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;
    use tokio::sync::{mpsc, Notify};

    /// The `StartContext`s one stub provider received, so a test reads what it was launched with.
    type Starts = Arc<StdMutex<Vec<StartContext>>>;

    /// A provider that records what the manager does to it, without touching the network.
    struct StubProvider {
        spec: ProviderSpec,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
        contexts: Arc<StdMutex<Vec<StartContext>>>,
        permissions: Arc<StdMutex<Vec<(String, Decision)>>>,
        /// The models the manager assigned to the live handle, in order.
        models: Arc<StdMutex<Vec<String>>>,
        /// The thinking levels the manager assigned to the live handle, in order.
        thinking: Arc<StdMutex<Vec<ThinkingLevel>>>,
        /// The approval modes the manager assigned to the live handle, in order.
        permission_modes: Arc<StdMutex<Vec<PermissionMode>>>,
        /// The outbound event channel of every started session, oldest first, so a test can park a
        /// permission or a question the way a real agent does mid-turn.
        events: Arc<StdMutex<Vec<mpsc::Sender<SessionEvent>>>>,
        /// One-shot completion requests, in call order.
        completions: Arc<StdMutex<Vec<CompletionRequest>>>,
        /// What the one-shot path answers.
        completion: Arc<StdMutex<StubCompletion>>,
        /// Whether a credential counts as configured (drives `detect`).
        ready: bool,
        /// Park a turn after it reports busy until the test releases `gate`, so a mid-turn switch
        /// is deterministic instead of racing a sleep.
        hold: bool,
        gate: Arc<Notify>,
        /// Park a title completion until the test releases `completion_gate`, so a rename can race it.
        completion_hold: bool,
        completion_gate: Arc<Notify>,
    }

    #[derive(Clone)]
    enum StubCompletion {
        Text(String),
        Error(String),
    }

    impl Default for StubCompletion {
        fn default() -> Self {
            StubCompletion::Text("Fix the login bug".into())
        }
    }

    impl StubProvider {
        fn new() -> Self {
            let mut spec = catalog::find("anthropic").expect("anthropic ships");
            spec.id = "stub".into();
            spec.name = "Stub".into();
            Self {
                spec,
                active: Arc::new(AtomicUsize::new(0)),
                peak: Arc::new(AtomicUsize::new(0)),
                contexts: Arc::new(StdMutex::new(Vec::new())),
                permissions: Arc::new(StdMutex::new(Vec::new())),
                models: Arc::new(StdMutex::new(Vec::new())),
                thinking: Arc::new(StdMutex::new(Vec::new())),
                permission_modes: Arc::new(StdMutex::new(Vec::new())),
                events: Arc::new(StdMutex::new(Vec::new())),
                completions: Arc::new(StdMutex::new(Vec::new())),
                completion: Arc::new(StdMutex::new(StubCompletion::default())),
                ready: true,
                hold: false,
                gate: Arc::new(Notify::new()),
                completion_hold: false,
                completion_gate: Arc::new(Notify::new()),
            }
        }

        /// A second registered provider, so a cross-provider switch has somewhere to go.
        fn named(id: &str) -> Self {
            let mut stub = Self::new();
            stub.spec.id = id.into();
            stub.spec.name = id.into();
            stub
        }

        fn with_ready(mut self, ready: bool) -> Self {
            self.ready = ready;
            self
        }

        /// Run under a catalogued provider id, so the credential-driven suggestion can find it.
        fn for_provider(mut self, id: &str) -> Self {
            self.spec = catalog::find(id).expect("the provider ships");
            self
        }

        fn with_completion(self, reply: &str) -> Self {
            *self
                .completion
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                StubCompletion::Text(reply.into());
            self
        }

        fn with_failing_completion(self, message: &str) -> Self {
            *self
                .completion
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                StubCompletion::Error(message.into());
            self
        }

        fn holding_turns(mut self) -> Self {
            self.hold = true;
            self
        }

        fn holding_completions(mut self) -> Self {
            self.completion_hold = true;
            self
        }
    }

    fn lock<T>(value: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
        value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[async_trait::async_trait]
    impl AgentProvider for StubProvider {
        fn id(&self) -> &str {
            &self.spec.id
        }

        fn spec(&self) -> &ProviderSpec {
            &self.spec
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities::agent()
        }

        fn detect(&self, _deps: &ProviderDeps) -> Detection {
            if self.ready {
                Detection::ready(AuthMethod::ApiKey, self.spec.default_model.clone())
            } else {
                Detection::missing(
                    AuthMethod::ApiKey,
                    self.spec.default_model.clone(),
                    "No API key for Stub.",
                )
            }
        }

        async fn complete(
            &self,
            request: CompletionRequest,
            _deps: ProviderDeps,
        ) -> Result<String, ProviderError> {
            lock(&self.completions).push(request);
            if self.completion_hold {
                self.completion_gate.notified().await;
            }
            // A real completion takes a network round trip; the delay lets two callers race so the
            // per-session guard is observable.
            tokio::time::sleep(Duration::from_millis(30)).await;
            match lock(&self.completion).clone() {
                StubCompletion::Text(text) => Ok(text),
                StubCompletion::Error(message) => Err(ProviderError::Api(message)),
            }
        }

        async fn start(
            &self,
            ctx: StartContext,
            _deps: ProviderDeps,
        ) -> Result<Box<dyn AgentSession>, ProviderError> {
            let history = ctx.history.clone();
            lock(&self.contexts).push(ctx);
            let (keepalive, events) = mpsc::channel(4);
            lock(&self.events).push(keepalive.clone());
            Ok(Box::new(StubSession {
                events: Some(events),
                events_out: keepalive,
                active: Arc::clone(&self.active),
                peak: Arc::clone(&self.peak),
                permissions: Arc::clone(&self.permissions),
                models: Arc::clone(&self.models),
                thinking: Arc::clone(&self.thinking),
                permission_modes: Arc::clone(&self.permission_modes),
                history,
                hold: self.hold,
                gate: Arc::clone(&self.gate),
            }))
        }
    }

    struct StubSession {
        events: Option<mpsc::Receiver<SessionEvent>>,
        events_out: mpsc::Sender<SessionEvent>,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
        permissions: Arc<StdMutex<Vec<(String, Decision)>>>,
        models: Arc<StdMutex<Vec<String>>>,
        thinking: Arc<StdMutex<Vec<ThinkingLevel>>>,
        permission_modes: Arc<StdMutex<Vec<PermissionMode>>>,
        /// The conversation the way the real agent keeps it: one user message per send and one
        /// assistant reply (or a dangling tool call, for the trim test).
        history: Vec<Message>,
        hold: bool,
        gate: Arc<Notify>,
    }

    #[async_trait::async_trait]
    impl AgentSession for StubSession {
        fn take_events(&mut self) -> mpsc::Receiver<SessionEvent> {
            self.events.take().expect("called once")
        }

        async fn send(&mut self, input: UserInput) -> Result<(), ProviderError> {
            let in_flight = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(in_flight, Ordering::SeqCst);
            if self.hold {
                self.events_out
                    .send(SessionEvent::new(EventKind::Status {
                        state: SessionState::Busy,
                    }))
                    .await
                    .expect("the pump still listens");
                self.gate.notified().await;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            self.history.push(Message::user(input.text.clone()));
            if input.text == "unfinished" {
                self.history
                    .push(Message::assistant(vec![MsgBlock::ToolUse {
                        id: "call-1".into(),
                        name: "run_command".into(),
                        input: serde_json::json!({ "command": "sleep 60" }),
                    }]));
            } else {
                self.history.push(Message::assistant(vec![MsgBlock::Text {
                    text: format!("ack: {}", input.text),
                }]));
            }
            if self.hold {
                self.events_out
                    .send(SessionEvent::new(EventKind::Status {
                        state: SessionState::Idle,
                    }))
                    .await
                    .expect("the pump still listens");
            }
            Ok(())
        }

        async fn history(&mut self) -> Vec<Message> {
            self.history.clone()
        }

        async fn respond_permission(
            &mut self,
            request_id: &str,
            decision: Decision,
        ) -> Result<(), ProviderError> {
            lock(&self.permissions).push((request_id.to_string(), decision));
            Ok(())
        }

        async fn respond_question(
            &mut self,
            _request_id: &str,
            _answers: Vec<crate::sessions::event::QuestionAnswer>,
        ) -> Result<(), ProviderError> {
            Ok(())
        }

        async fn set_thinking(
            &mut self,
            level: crate::providers::ThinkingLevel,
        ) -> Result<(), ProviderError> {
            lock(&self.thinking).push(level);
            Ok(())
        }

        async fn set_model(&mut self, model: String) -> Result<(), ProviderError> {
            lock(&self.models).push(model);
            Ok(())
        }

        async fn set_permission_mode(
            &mut self,
            mode: crate::providers::PermissionMode,
        ) -> Result<(), ProviderError> {
            lock(&self.permission_modes).push(mode);
            Ok(())
        }

        async fn cancel(&mut self) -> Result<(), ProviderError> {
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), ProviderError> {
            Ok(())
        }

        fn resume_token(&self) -> Option<String> {
            None
        }
    }

    fn manager_from(stubs: Vec<StubProvider>) -> SessionManager {
        let paths = AppPaths::new(std::env::temp_dir().join("grove-manager-tests"));
        let deps = ProviderDeps {
            paths,
            http: reqwest::Client::new(),
            secrets: Arc::new(MemorySecretStore::new()),
        };
        let providers: Vec<Arc<dyn AgentProvider>> = stubs
            .into_iter()
            .map(|stub| Arc::new(stub) as Arc<dyn AgentProvider>)
            .collect();
        SessionManager::new(Arc::new(Store::open_in_memory().unwrap()), deps, providers)
    }

    fn manager_with(stub: StubProvider) -> (SessionManager, Starts) {
        let contexts = Arc::clone(&stub.contexts);
        (manager_from(vec![stub]), contexts)
    }

    fn manager() -> SessionManager {
        manager_with(StubProvider::new()).0
    }

    async fn create(manager: &SessionManager) -> String {
        manager
            .create(CreateSessionRequest {
                provider: "stub".into(),
                ..Default::default()
            })
            .await
            .expect("session is created")
            .session
            .id
    }

    async fn create_on(manager: &SessionManager, provider: &str) -> String {
        manager
            .create(CreateSessionRequest {
                provider: provider.into(),
                ..Default::default()
            })
            .await
            .expect("session is created")
            .session
            .id
    }

    /// Two registered providers and the `StartContext` each received, so a cross-provider handoff can
    /// be checked from what the new provider was actually launched with.
    fn manager_two() -> (SessionManager, Starts, Starts) {
        let from = StubProvider::named("stub-a");
        let to = StubProvider::named("stub-b");
        let from_starts = Arc::clone(&from.contexts);
        let to_starts = Arc::clone(&to.contexts);
        (manager_from(vec![from, to]), from_starts, to_starts)
    }

    fn latest_sink(
        events: &StdMutex<Vec<mpsc::Sender<SessionEvent>>>,
    ) -> mpsc::Sender<SessionEvent> {
        lock(events)
            .last()
            .cloned()
            .expect("the stub started a session")
    }

    /// The pump applies events asynchronously; wait until the row mirrors the parked request.
    async fn wait_until_awaiting(manager: &SessionManager, session: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            if manager
                .get(session)
                .expect("the session exists")
                .session
                .awaiting
            {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the parked request never reached the row"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Wait for a status the pump publishes on its own schedule (busy when a turn starts, idle when it
    /// ends), so a test observes the transition instead of sleeping past it.
    async fn wait_until_status(manager: &SessionManager, session: &str, state: SessionState) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let current = manager.get(session).expect("the session exists").session;
            if current.status == state {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the session never reached {state:?}; it stayed {:?}",
                current.status
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    fn await_is_clear(manager: &SessionManager, session: &str) {
        assert!(
            !manager.get(session).unwrap().session.awaiting,
            "the stopped turn cannot still be waiting for an answer"
        );
        assert!(
            !manager
                .list()
                .unwrap()
                .into_iter()
                .find(|row| row.session.id == session)
                .expect("the session is listed")
                .session
                .awaiting,
            "the tree's \"needs you\" row must clear"
        );
    }

    /// Whether the bus carried an `exited` status since the last read.
    fn saw_exited(bus: &mut broadcast::Receiver<EventEnvelope<SessionEvent>>) -> bool {
        loop {
            match bus.try_recv() {
                Ok(envelope) => {
                    if envelope.payload.kind
                        == (EventKind::Status {
                            state: SessionState::Exited,
                        })
                    {
                        return true;
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => return false,
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(broadcast::error::TryRecvError::Closed) => return false,
            }
        }
    }

    #[tokio::test]
    async fn switching_provider_with_a_parked_permission_is_refused_and_keeps_the_request() {
        // The tree and titlebar read `awaiting` from the row. A switch has to wait for that answer:
        // stopping the agent to relaunch it would discard the parked request and strand the badge.
        let stub = StubProvider::new();
        let events = Arc::clone(&stub.events);
        let (manager, contexts) = manager_with(stub);
        let session = create(&manager).await;

        latest_sink(&events)
            .send(SessionEvent::new(EventKind::PermissionRequest {
                request_id: "req-1".into(),
                title: "cargo test".into(),
                detail: "Runs the suite".into(),
            }))
            .await
            .expect("the stub accepts the request");
        wait_until_awaiting(&manager, &session).await;
        let mut bus = manager.subscribe();
        let before = manager.get(&session).unwrap().session;

        let error = manager
            .switch_provider(&session, "stub", Some("claude-opus-4-1".into()))
            .await
            .expect_err("a switch while the agent waits for the user is refused");
        assert!(matches!(&error, SessionError::Conflict(_)), "{error}");
        assert_eq!(IpcError::from(error).code, ErrorCode::Conflict);

        let after = manager.get(&session).unwrap().session;
        assert!(
            after.awaiting,
            "the permission is still the user's to answer"
        );
        assert_eq!(after.provider, before.provider);
        assert_eq!(after.model, before.model);
        assert_eq!(lock(&contexts).len(), 1, "nothing was relaunched");
        assert!(
            !saw_exited(&mut bus),
            "a refused switch must not stop the agent"
        );
        // The live agent is intact: the answer still reaches it.
        manager
            .respond_permission(&session, "req-1", Decision::Approve)
            .await
            .expect("the request is still parked on the live agent");
    }

    #[tokio::test]
    async fn switching_provider_with_a_parked_question_is_refused_and_keeps_the_request() {
        // `ask_user` parks a turn the same way a permission does; neither can be thrown away by a
        // provider relaunch.
        let stub = StubProvider::new();
        let events = Arc::clone(&stub.events);
        let (manager, contexts) = manager_with(stub);
        let session = create(&manager).await;

        latest_sink(&events)
            .send(SessionEvent::new(EventKind::QuestionRequest {
                request_id: "q-1".into(),
                questions: Vec::new(),
            }))
            .await
            .expect("the stub accepts the question");
        wait_until_awaiting(&manager, &session).await;

        let error = manager
            .switch_provider(&session, "stub", Some("claude-opus-4-1".into()))
            .await
            .expect_err("a switch while the agent waits for an answer is refused");
        assert!(matches!(&error, SessionError::Conflict(_)), "{error}");

        assert!(
            manager.get(&session).unwrap().session.awaiting,
            "the question is still the user's to answer"
        );
        assert_eq!(lock(&contexts).len(), 1, "nothing was relaunched");
    }

    #[tokio::test]
    async fn switching_provider_while_a_turn_is_running_is_refused_until_idle() {
        let stub = StubProvider::new().holding_turns();
        let gate = Arc::clone(&stub.gate);
        let (manager, contexts) = manager_with(stub);
        let session = create(&manager).await;
        let mut bus = manager.subscribe();

        let sending = manager.send(&session, "a long turn");
        let refused = async {
            wait_until_status(&manager, &session, SessionState::Busy).await;
            let refused = manager
                .switch_provider(&session, "stub", Some("claude-opus-4-1".into()))
                .await;
            // Let the held turn finish whether or not the switch was refused, so a regression cannot
            // hang the suite.
            gate.notify_one();
            refused
        };
        let (sent, refused) = tokio::join!(sending, refused);
        sent.expect("the running turn is untouched");
        let error = refused.expect_err("a switch during a running turn is refused");
        assert!(matches!(&error, SessionError::Conflict(_)), "{error}");
        assert_eq!(IpcError::from(error).code, ErrorCode::Conflict);
        assert_eq!(
            lock(&contexts).len(),
            1,
            "the busy session was not stopped or relaunched"
        );
        assert!(
            !saw_exited(&mut bus),
            "the refused switch must not stop the agent"
        );
        assert_eq!(
            manager.get(&session).unwrap().session.model,
            None,
            "a refused switch does not persist the new model"
        );

        // The shell retries when the turn ends; at idle the switch is accepted.
        wait_until_status(&manager, &session, SessionState::Idle).await;
        manager
            .switch_provider(&session, "stub", Some("claude-opus-4-1".into()))
            .await
            .expect("an idle session switches");
        assert_eq!(lock(&contexts).len(), 2, "the idle retry relaunched");
        assert_eq!(
            manager.get(&session).unwrap().session.model.as_deref(),
            Some("claude-opus-4-1")
        );
    }

    #[tokio::test]
    async fn closing_a_session_with_a_pending_question_clears_awaiting_once() {
        // The stop button goes through `close`: the explicit exit must clear awaiting and must not
        // duplicate the status event the transcript and tree act on.
        let stub = StubProvider::new();
        let events = Arc::clone(&stub.events);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;

        latest_sink(&events)
            .send(SessionEvent::new(EventKind::QuestionRequest {
                request_id: "q-1".into(),
                questions: Vec::new(),
            }))
            .await
            .expect("the stub accepts the question");
        wait_until_awaiting(&manager, &session).await;
        let mut bus = manager.subscribe();

        manager.close(&session).await.unwrap();

        assert_eq!(
            manager.get(&session).unwrap().session.status,
            SessionState::Exited,
            "a stopped session's row is exited"
        );
        await_is_clear(&manager, &session);
        assert_eq!(
            manager
                .get(&session)
                .unwrap()
                .events
                .iter()
                .filter(|event| event.kind
                    == (EventKind::Status {
                        state: SessionState::Exited,
                    }))
                .count(),
            1,
            "a stopped session publishes exactly one exit"
        );
        assert!(
            saw_exited(&mut bus),
            "the stop must publish the exited status the tree clears on"
        );
    }

    #[tokio::test]
    async fn archiving_a_running_session_stops_it_publishes_exit_and_clears_awaiting() {
        // Archive is a soft hide, but it still owns the process: the agent must stop before the row
        // leaves the active tree, and a parked permission must not outlive the stop.
        let stub = StubProvider::new();
        let events = Arc::clone(&stub.events);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;

        latest_sink(&events)
            .send(SessionEvent::new(EventKind::PermissionRequest {
                request_id: "req-1".into(),
                title: "cargo test".into(),
                detail: "Runs the suite".into(),
            }))
            .await
            .expect("the stub accepts the request");
        wait_until_awaiting(&manager, &session).await;
        let mut bus = manager.subscribe();

        let archived = manager.archive(&session).await.expect("archive");

        assert!(
            archived.archived_at.is_some(),
            "the returned row carries the stamp the tree filters on"
        );
        assert_eq!(
            manager.get(&session).unwrap().session.archived_at,
            archived.archived_at
        );
        assert_eq!(
            manager.get(&session).unwrap().session.status,
            SessionState::Exited,
            "archiving a running session stops its agent"
        );
        await_is_clear(&manager, &session);
        assert!(
            saw_exited(&mut bus),
            "the stop must publish the exited status the tree clears on"
        );
        assert!(
            matches!(
                manager.send(&session, "still here?").await,
                Err(SessionError::NotRunning(_))
            ),
            "an archived session has no live agent to send to"
        );
    }

    #[tokio::test]
    async fn archiving_a_stopped_session_keeps_the_transcript_and_restore_returns_it() {
        let manager = manager();
        let session = create(&manager).await;
        manager
            .send(&session, "remember the number 7")
            .await
            .unwrap();
        manager.close(&session).await.unwrap();
        let before = manager.get(&session).unwrap();

        let archived = manager
            .archive(&session)
            .await
            .expect("archive a stopped session");
        assert!(archived.archived_at.is_some());
        let hidden = manager.get(&session).unwrap();
        assert_eq!(hidden.session.archived_at, archived.archived_at);
        assert_eq!(
            hidden.events, before.events,
            "archiving never rewrites the transcript"
        );

        let restored = manager.restore(&session).await.expect("restore");
        assert_eq!(restored.archived_at, None);
        assert_eq!(manager.get(&session).unwrap().events, before.events);
    }

    #[tokio::test]
    async fn deleting_a_session_removes_it_with_every_dependent_row() {
        let stub = StubProvider::new();
        let events = Arc::clone(&stub.events);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;
        manager
            .send(&session, "keep this transcript")
            .await
            .unwrap();
        let store = manager.store();
        store.write_plan(&session, "# Plan\n", 0, "hash").unwrap();
        store.add_pin(&session, "grove-architecture").unwrap();
        store
            .start_subagent(
                &session,
                "call-1",
                "audit the store",
                "2025-01-01T00:00:00.000Z",
            )
            .unwrap();
        latest_sink(&events)
            .send(SessionEvent::new(EventKind::PermissionRequest {
                request_id: "req-1".into(),
                title: "cargo test".into(),
                detail: "Runs the suite".into(),
            }))
            .await
            .expect("the stub accepts the request");
        wait_until_awaiting(&manager, &session).await;
        let mut bus = manager.subscribe();

        manager.delete(&session).await.expect("delete");

        assert!(
            manager.get(&session).is_err(),
            "the deleted session is gone from the store"
        );
        assert!(store.session(&session).unwrap().is_none());
        assert!(store.session_events(&session).unwrap().is_empty());
        assert!(store.plan_row(&session).unwrap().is_none());
        assert!(store.pins(&session).unwrap().is_empty());
        assert!(
            saw_exited(&mut bus),
            "delete stops a live agent before removing its row"
        );
    }

    #[tokio::test]
    async fn archiving_or_deleting_an_unknown_session_is_not_found() {
        let manager = manager();
        assert!(matches!(
            manager.archive("missing").await,
            Err(SessionError::NotFound(_))
        ));
        assert!(matches!(
            manager.restore("missing").await,
            Err(SessionError::NotFound(_))
        ));
        assert!(matches!(
            manager.delete("missing").await,
            Err(SessionError::NotFound(_))
        ));
    }

    #[test]
    fn providers_reports_the_registered_specs_with_their_models() {
        let manager = manager();
        let providers = manager.providers();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "stub");
        assert!(!providers[0].models.is_empty(), "the picker needs models");
        assert_eq!(manager.label_for("stub"), Some("Stub"));
        assert_eq!(manager.label_for("nope"), None);
    }

    #[test]
    fn detect_all_reports_readiness_in_registry_order() {
        let (manager, _contexts) = manager_with(StubProvider::new().with_ready(false));
        let detections = manager.detect_all();
        assert_eq!(detections.len(), 1);
        assert!(!detections[0].installed);
        assert_eq!(detections[0].method, "api_key");
        assert_eq!(detections[0].default_model, "claude-sonnet-4-5");
    }

    #[tokio::test]
    async fn an_unnamed_provider_falls_back_to_one_that_is_ready() {
        let manager = manager();
        let detail = manager
            .create(CreateSessionRequest {
                provider: String::new(),
                ..Default::default()
            })
            .await
            .expect("a default provider is chosen");
        assert_eq!(detail.session.provider, "stub");
    }

    #[tokio::test]
    async fn a_permission_answer_reaches_the_live_session() {
        // The Tauri command is a one-line forward to this call, so proving the manager delivers the
        // answer to the runtime is what proves the buttons in the transcript can unblock a turn.
        let stub = StubProvider::new();
        let permissions = Arc::clone(&stub.permissions);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;

        manager
            .respond_permission(&session, "req-1", Decision::Approve)
            .await
            .expect("the answer is delivered");
        manager
            .respond_permission(&session, "req-2", Decision::Deny)
            .await
            .expect("the answer is delivered");

        assert_eq!(
            *lock(&permissions),
            vec![
                ("req-1".to_string(), Decision::Approve),
                ("req-2".to_string(), Decision::Deny),
            ]
        );
    }

    #[tokio::test]
    async fn create_and_resume_both_launch_the_provider_with_the_stored_config() {
        let (manager, contexts) = manager_with(StubProvider::new());
        let session = create(&manager).await;
        {
            let launched = lock(&contexts);
            assert_eq!(launched.len(), 1, "create launches");
            assert_eq!(
                launched[0].session_id, session,
                "the launch carries the session id"
            );
        }
        assert_eq!(
            manager.get(&session).unwrap().session.status,
            SessionState::Ready,
            "a launched session accepts its first message"
        );

        manager.close(&session).await.unwrap();
        manager.resume(&session).await.unwrap();
        {
            let launched = lock(&contexts);
            assert_eq!(launched.len(), 2, "resume launches again");
            assert_eq!(
                launched[1].session_id, session,
                "the resume keeps the session id"
            );
        }
        assert_eq!(
            manager.get(&session).unwrap().session.status,
            SessionState::Ready,
            "a resumed session accepts its next message"
        );
    }

    #[tokio::test]
    async fn a_model_change_updates_the_live_agent_without_relaunching_it() {
        let stub = StubProvider::new();
        let models = Arc::clone(&stub.models);
        let (manager, contexts) = manager_with(stub);
        let session = create(&manager).await;
        let before = manager.get(&session).unwrap().events.len();

        let detail = manager
            .set_model(&session, "claude-opus-4-1", None)
            .await
            .expect("the live agent accepts a model change");

        assert_eq!(detail.session.model.as_deref(), Some("claude-opus-4-1"));
        assert_eq!(*lock(&models), vec!["claude-opus-4-1".to_string()]);
        assert_eq!(lock(&contexts).len(), 1, "a hot swap does not relaunch");
        assert_eq!(detail.session.status, SessionState::Ready);
        assert_eq!(
            detail.events.len(),
            before,
            "a same-provider swap adds no transcript event"
        );
    }

    #[tokio::test]
    async fn a_model_change_persists_the_model_and_thinking_together() {
        let stub = StubProvider::new();
        let models = Arc::clone(&stub.models);
        let thinking = Arc::clone(&stub.thinking);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;

        let detail = manager
            .set_model(&session, "claude-opus-4-1", Some(ThinkingLevel::High))
            .await
            .expect("the pair is accepted");

        assert_eq!(detail.session.model.as_deref(), Some("claude-opus-4-1"));
        assert_eq!(detail.thinking_level, ThinkingLevel::High);
        assert_eq!(*lock(&models), vec!["claude-opus-4-1".to_string()]);
        assert_eq!(
            *lock(&thinking),
            vec![ThinkingLevel::High],
            "the live agent gets the clamped level with the model"
        );
        let stored = manager.get(&session).unwrap();
        assert_eq!(stored.session.model.as_deref(), Some("claude-opus-4-1"));
        assert_eq!(stored.thinking_level, ThinkingLevel::High);
    }

    #[tokio::test]
    async fn a_stopped_session_keeps_its_new_model_and_resumes_with_it() {
        let (manager, contexts) = manager_with(StubProvider::new());
        let session = create(&manager).await;
        manager.close(&session).await.unwrap();

        let stopped = manager
            .set_model(&session, "claude-opus-4-1", Some(ThinkingLevel::Low))
            .await
            .expect("a stopped session can still choose its next model");
        assert_eq!(stopped.session.status, SessionState::Exited);
        assert_eq!(stopped.session.model.as_deref(), Some("claude-opus-4-1"));
        assert_eq!(stopped.thinking_level, ThinkingLevel::Low);
        assert_eq!(lock(&contexts).len(), 1, "nothing was launched");

        manager.resume(&session).await.unwrap();
        let launched = lock(&contexts);
        assert_eq!(launched.len(), 2, "resume launches once");
        assert_eq!(
            launched[1].model.as_deref(),
            Some("claude-opus-4-1"),
            "the resumed agent starts on the chosen model"
        );
        assert_eq!(
            launched[1].config["thinkingLevel"], "low",
            "the chosen level resumes with the model"
        );
    }

    #[tokio::test]
    async fn a_hot_swap_during_a_parked_permission_does_not_strand_the_turn() {
        // A hot swap leaves the running turn alone, so the parked permission stays answerable. The
        // C5 stop path is still what clears `awaiting` when the session is actually stopped.
        let stub = StubProvider::new();
        let events = Arc::clone(&stub.events);
        let permissions = Arc::clone(&stub.permissions);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;

        latest_sink(&events)
            .send(SessionEvent::new(EventKind::PermissionRequest {
                request_id: "req-1".into(),
                title: "cargo test".into(),
                detail: "Runs the suite".into(),
            }))
            .await
            .expect("the stub accepts the request");
        wait_until_awaiting(&manager, &session).await;
        let mut bus = manager.subscribe();

        manager
            .set_model(&session, "claude-opus-4-1", None)
            .await
            .expect("a hot swap during a parked permission is allowed");

        assert!(
            manager.get(&session).unwrap().session.awaiting,
            "the parked turn is still waiting for the user"
        );
        assert!(
            !saw_exited(&mut bus),
            "a hot swap must not stop the agent out from under the parked request"
        );
        manager
            .respond_permission(&session, "req-1", Decision::Approve)
            .await
            .expect("the answer still reaches the live agent");
        assert_eq!(
            *lock(&permissions),
            vec![("req-1".to_string(), Decision::Approve)]
        );

        manager.close(&session).await.unwrap();
        await_is_clear(&manager, &session);
    }

    #[tokio::test]
    async fn an_empty_or_unknown_model_change_is_rejected() {
        let manager = manager();
        let session = create(&manager).await;

        let empty = manager.set_model(&session, "   ", None).await.unwrap_err();
        assert!(matches!(empty, SessionError::Invalid(_)));
        let missing = manager
            .set_model("missing", "claude-opus-4-1", None)
            .await
            .unwrap_err();
        assert!(matches!(missing, SessionError::NotFound(_)));
        assert_eq!(
            manager.get(&session).unwrap().session.model,
            None,
            "a rejected change leaves the row alone"
        );
    }

    #[tokio::test]
    async fn thinking_level_persists_and_updates_a_stopped_session() {
        let manager = manager();
        let session = create(&manager).await;

        let live = manager
            .set_thinking(&session, ThinkingLevel::High)
            .await
            .unwrap();
        assert_eq!(live.thinking_level, ThinkingLevel::High);

        manager.close(&session).await.unwrap();
        let stopped = manager
            .set_thinking(&session, ThinkingLevel::Low)
            .await
            .unwrap();
        assert_eq!(stopped.thinking_level, ThinkingLevel::Low);
        assert_eq!(
            manager.get(&session).unwrap().thinking_level,
            ThinkingLevel::Low
        );
    }

    #[tokio::test]
    async fn permission_mode_applies_live_and_survives_a_stop() {
        let stub = StubProvider::new();
        let modes = Arc::clone(&stub.permission_modes);
        let manager = manager_from(vec![stub]);
        let session = create(&manager).await;

        let live = manager
            .set_permission_mode(&session, PermissionMode::Full)
            .await
            .unwrap();
        assert_eq!(live.permission_mode, PermissionMode::Full);
        assert_eq!(*lock(&modes), vec![PermissionMode::Full]);

        manager.close(&session).await.unwrap();
        let stopped = manager
            .set_permission_mode(&session, PermissionMode::Ask)
            .await
            .unwrap();
        assert_eq!(stopped.permission_mode, PermissionMode::Ask);
        assert_eq!(
            manager.get(&session).unwrap().permission_mode,
            PermissionMode::Ask
        );
    }

    #[tokio::test]
    async fn a_new_session_reads_its_permission_mode_from_the_config() {
        let manager = manager();
        let detail = manager
            .create(CreateSessionRequest {
                provider: "stub".into(),
                config: Some(serde_json::json!({ "permissionMode": "full" })),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(detail.permission_mode, PermissionMode::Full);
    }

    #[tokio::test]
    async fn a_blank_title_creates_an_auto_session_and_a_typed_title_stays_manual() {
        let manager = manager();
        for blank in [None, Some("   ".to_string())] {
            let detail = manager
                .create(CreateSessionRequest {
                    provider: "stub".into(),
                    title: blank,
                    ..Default::default()
                })
                .await
                .expect("a blank title is allowed");
            assert_eq!(
                detail.session.title, TEMPORARY_SESSION_TITLE,
                "a blank title gets the temporary row title"
            );
            assert_eq!(detail.session.title_source, TitleSource::Auto);
        }

        let detail = manager
            .create(CreateSessionRequest {
                provider: "stub".into(),
                title: Some("  Release notes  ".into()),
                ..Default::default()
            })
            .await
            .expect("a typed title is used as written");
        assert_eq!(detail.session.title, "Release notes");
        assert_eq!(detail.session.title_source, TitleSource::Manual);
    }

    #[tokio::test]
    async fn the_first_send_writes_a_word_boundary_placeholder_and_keeps_the_session_auto() {
        let stub = StubProvider::new();
        let contexts = Arc::clone(&stub.contexts);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;
        let mut bus = manager.subscribe();
        let message = "Refactor the session store so a blank title becomes a readable \
                       placeholder after the first message and never needs the provider";

        manager.send(&session, message).await.expect("send");

        let stored = manager.get(&session).unwrap().session;
        assert_eq!(
            stored.title_source,
            TitleSource::Auto,
            "a placeholder stays auto"
        );
        assert!(
            stored.title.chars().count() <= TITLE_MAX_CHARS + 1,
            "the placeholder is capped: {}",
            stored.title
        );
        assert!(
            stored.title.ends_with('…'),
            "a truncated placeholder says so: {}",
            stored.title
        );
        assert!(
            !stored.title.ends_with(" …"),
            "the ellipsis never follows a space: {}",
            stored.title
        );
        assert!(
            stored
                .title
                .starts_with("Refactor the session store so a blank title"),
            "the whole words that fit are kept: {}",
            stored.title
        );
        assert_eq!(
            lock(&contexts).len(),
            1,
            "the placeholder path never starts a provider"
        );

        let announced = manager
            .get(&session)
            .unwrap()
            .events
            .into_iter()
            .find(|event| matches!(event.kind, EventKind::TitleChanged { .. }))
            .expect("the placeholder is announced");
        assert_eq!(
            announced.kind,
            EventKind::TitleChanged {
                title: stored.title.clone(),
                source: TitleSource::Auto,
            }
        );
        let mut seen = false;
        while let Ok(envelope) = bus.try_recv() {
            if let EventKind::TitleChanged { title, source } = envelope.payload.kind {
                assert_eq!(title, stored.title);
                assert_eq!(source, TitleSource::Auto);
                seen = true;
            }
        }
        assert!(seen, "the sidebar receives title_changed without a reload");
    }

    #[tokio::test]
    async fn only_the_first_send_replaces_the_placeholder_and_a_typed_title_is_never_touched() {
        let manager = manager();
        let auto = create(&manager).await;
        manager.send(&auto, "First message").await.unwrap();
        let first = manager.get(&auto).unwrap().session;
        manager.send(&auto, "A later message").await.unwrap();
        assert_eq!(
            manager.get(&auto).unwrap().session.title,
            first.title,
            "the placeholder is taken from the first message only"
        );

        let typed = manager
            .create(CreateSessionRequest {
                provider: "stub".into(),
                title: Some("Keep me".into()),
                ..Default::default()
            })
            .await
            .unwrap()
            .session
            .id;
        manager
            .send(&typed, "A very different first message")
            .await
            .unwrap();
        let stored = manager.get(&typed).unwrap().session;
        assert_eq!(stored.title, "Keep me");
        assert_eq!(stored.title_source, TitleSource::Manual);
        assert!(
            !manager
                .get(&typed)
                .unwrap()
                .events
                .iter()
                .any(|event| matches!(event.kind, EventKind::TitleChanged { .. })),
            "a user-written title produces no title event"
        );
    }

    #[tokio::test]
    async fn renaming_trims_the_title_and_persists_the_new_one() {
        let manager = manager();
        let session = create(&manager).await;
        let before = manager.get(&session).unwrap().session;
        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut bus = manager.subscribe();

        let renamed = manager
            .rename(&session, "  Release notes  ")
            .await
            .expect("a valid title renames the session");

        assert_eq!(renamed.title, "Release notes");
        assert_eq!(
            renamed.title_source,
            TitleSource::Manual,
            "a rename takes the session out of automatic naming"
        );
        let detail = manager.get(&session).unwrap();
        assert_eq!(
            detail.session.title, "Release notes",
            "the tree reads the stored row, not the returned copy"
        );
        assert_eq!(detail.session.title_source, TitleSource::Manual);
        assert!(
            detail.events.iter().any(|event| event.kind
                == EventKind::TitleChanged {
                    title: "Release notes".into(),
                    source: TitleSource::Manual,
                }),
            "a rename is announced so the sidebar and header patch without a reload"
        );
        assert!(
            matches!(
                bus.try_recv().map(|envelope| envelope.payload.kind),
                Ok(EventKind::TitleChanged { .. })
            ),
            "the live window receives title_changed"
        );
        assert_ne!(
            detail.session.updated_at, before.updated_at,
            "a rename moves the session's updated timestamp"
        );
    }

    #[tokio::test]
    async fn renaming_a_stopped_session_still_announces_the_change() {
        // A stopped session has no pump, so rename writes the event directly; skipping that path
        // would leave a reopened tree stale until the next reload.
        let manager = manager();
        let session = create(&manager).await;
        manager.close(&session).await.unwrap();

        let renamed = manager
            .rename(&session, "Stopped but renamed")
            .await
            .unwrap();

        assert_eq!(renamed.title_source, TitleSource::Manual);
        assert!(
            manager
                .get(&session)
                .unwrap()
                .events
                .iter()
                .any(|event| event.kind
                    == EventKind::TitleChanged {
                        title: "Stopped but renamed".into(),
                        source: TitleSource::Manual,
                    }),
            "the stopped path still records title_changed"
        );
    }

    #[tokio::test]
    async fn rename_rejects_blank_control_and_overlong_titles() {
        let manager = manager();
        let session = create(&manager).await;
        let long = "x".repeat(81);
        for (title, why) in [
            ("", "an empty title is not a rename"),
            ("   ", "a whitespace-only title is not a rename"),
            ("bad\u{7}title", "control characters would corrupt the row"),
            (
                long.as_str(),
                "a title longer than 80 characters is refused",
            ),
        ] {
            let error = manager.rename(&session, title).await.unwrap_err();
            assert!(matches!(error, SessionError::Invalid(_)), "{why}");
        }
        assert_eq!(
            manager.get(&session).unwrap().session.title,
            TEMPORARY_SESSION_TITLE,
            "a refused title leaves the stored row alone"
        );

        let at_limit = "x".repeat(80);
        manager
            .rename(&session, &at_limit)
            .await
            .expect("80 characters is the limit, not one over it");

        let missing = manager.rename("missing", "Title").await.unwrap_err();
        assert!(matches!(missing, SessionError::NotFound(_)));
    }

    #[tokio::test]
    async fn a_resume_replays_the_transcript_tail_because_the_agent_has_no_server_side_state() {
        let (manager, contexts) = manager_with(StubProvider::new());
        let session = create(&manager).await;
        manager
            .send(&session, "remember the number 7")
            .await
            .unwrap();
        manager.close(&session).await.unwrap();
        manager.resume(&session).await.unwrap();

        let launched = lock(&contexts);
        let resumed = launched.last().expect("resume launched");
        let prompt = resumed.prompt.as_deref().unwrap_or_default();
        assert!(
            prompt.contains("remember the number 7"),
            "the replayed tail carries the conversation: {prompt:?}"
        );
    }

    #[tokio::test]
    async fn concurrent_commands_for_one_session_are_serialized() {
        let stub = StubProvider::new();
        let peak = Arc::clone(&stub.peak);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;

        let (first, second) =
            tokio::join!(manager.send(&session, "one"), manager.send(&session, "two"));
        first.unwrap();
        second.unwrap();

        assert_eq!(
            peak.load(Ordering::SeqCst),
            1,
            "two sends on one session must never overlap"
        );
    }

    #[tokio::test]
    async fn sending_persists_the_user_message() {
        let manager = manager();
        let session = create(&manager).await;

        manager.send(&session, "  hello  ").await.unwrap();

        let detail = manager.get(&session).unwrap();
        let last = detail.events.last().expect("the user message is persisted");
        assert_eq!(
            last.kind,
            EventKind::Message {
                role: Role::User,
                blocks: vec![Block::Text {
                    text: "hello".into()
                }],
            }
        );
    }

    #[tokio::test]
    async fn unknown_and_stopped_sessions_report_distinct_errors() {
        let manager = manager();

        let missing = manager.send("missing", "hi").await.unwrap_err();
        assert!(matches!(missing, SessionError::NotFound(_)));

        let session = create(&manager).await;
        manager.close(&session).await.unwrap();

        let stopped = manager.send(&session, "hi").await.unwrap_err();
        assert!(matches!(stopped, SessionError::NotRunning(_)));

        let detail = manager.get(&session).unwrap();
        assert_eq!(detail.session.status, SessionState::Exited);
        assert_eq!(
            detail
                .events
                .iter()
                .filter(|event| event.kind
                    == (EventKind::Status {
                        state: SessionState::Exited
                    }))
                .count(),
            1,
            "close records the exited transition once"
        );
    }

    #[tokio::test]
    async fn unknown_provider_is_rejected_without_a_row() {
        let manager = manager();
        let error = manager
            .create(CreateSessionRequest {
                provider: "nope".into(),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert!(matches!(error, SessionError::UnknownProvider(_)));
        assert!(manager.list().unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_messages_are_rejected() {
        let manager = manager();
        let session = create(&manager).await;
        let error = manager.send(&session, "   ").await.unwrap_err();
        assert!(matches!(error, SessionError::Invalid(_)));
    }

    #[tokio::test]
    async fn switching_provider_drops_the_resume_token_and_keeps_the_transcript() {
        let (manager, contexts) = manager_with(StubProvider::new());
        let session = create(&manager).await;
        manager.send(&session, "hello").await.unwrap();

        let detail = manager
            .switch_provider(&session, "stub", Some("claude-opus-4-1".into()))
            .await
            .expect("a model change is a switch");
        assert_eq!(detail.session.model.as_deref(), Some("claude-opus-4-1"));
        assert!(
            detail
                .events
                .iter()
                .any(|event| matches!(event.kind, EventKind::Message { .. })),
            "the transcript survives the switch"
        );
        assert_eq!(lock(&contexts).len(), 2, "the switch relaunches");
        assert!(
            !detail
                .events
                .iter()
                .any(|event| matches!(event.kind, EventKind::ContextRefreshed { .. })),
            "a model change must not add a context-refresh notice: {detail:?}"
        );
    }

    fn append_message(manager: &SessionManager, id: &str, role: Role, text: &str) {
        manager
            .store()
            .append_event(
                id,
                &SessionEvent::new(EventKind::Message {
                    role,
                    blocks: vec![Block::Text { text: text.into() }],
                }),
            )
            .expect("the event log accepts the message");
    }

    fn choose_title_model(manager: &SessionManager, provider: &str, model: &str) {
        crate::settings::SettingsService::new(manager.store())
            .set(crate::settings::SettingsPatch {
                title_model: Some(crate::settings::TitleModel {
                    provider: provider.into(),
                    model: model.into(),
                }),
                ..Default::default()
            })
            .expect("the title model saves");
    }

    fn title_changes(
        bus: &mut broadcast::Receiver<EventEnvelope<SessionEvent>>,
    ) -> Vec<(String, TitleSource)> {
        let mut changes = Vec::new();
        loop {
            match bus.try_recv() {
                Ok(envelope) => {
                    if let EventKind::TitleChanged { title, source } = envelope.payload.kind {
                        changes.push((title, source));
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => return changes,
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(broadcast::error::TryRecvError::Closed) => return changes,
            }
        }
    }

    fn exchange(manager: &SessionManager, session: &str) {
        append_message(
            manager,
            session,
            Role::User,
            "Wire up the GitHub OAuth device flow.",
        );
        append_message(
            manager,
            session,
            Role::Assistant,
            "The device flow starts with a short-lived user code.",
        );
    }

    #[tokio::test]
    async fn switching_hands_the_live_history_to_the_new_provider_without_a_replay() {
        let (manager, from_starts, to_starts) = manager_two();
        let session = create_on(&manager, "stub-a").await;
        manager.send(&session, "first task").await.unwrap();

        let detail = manager
            .switch_provider(&session, "stub-b", Some("claude-opus-4-1".into()))
            .await
            .expect("an idle live session hands over");

        let starts = lock(&to_starts);
        assert_eq!(starts.len(), 1, "the new provider starts once");
        assert_eq!(
            starts[0].prompt, None,
            "the full history replaces the transcript replay"
        );
        assert_eq!(
            starts[0].history,
            vec![
                Message::user("first task"),
                Message::assistant(vec![MsgBlock::Text {
                    text: "ack: first task".into()
                }]),
            ]
        );
        assert_eq!(
            lock(&from_starts).len(),
            1,
            "the old provider is not restarted"
        );
        assert_eq!(detail.session.provider, "stub-b");
        assert_eq!(detail.session.model.as_deref(), Some("claude-opus-4-1"));
        assert!(
            detail.events.iter().any(|event| matches!(
                &event.kind,
                EventKind::ContextRefreshed {
                    reason: ContextReason::ProviderSwitch,
                    ..
                }
            )),
            "a real provider change keeps its visible note: {:?}",
            detail.events
        );
    }

    #[tokio::test]
    async fn generating_a_title_sanitizes_it_and_announces_the_new_source() {
        let (manager, _contexts) =
            manager_with(StubProvider::new().with_completion("\"Title: Fix the login bug.\""));
        let session = create(&manager).await;
        exchange(&manager, &session);
        choose_title_model(&manager, "stub", "stub-small");
        let mut bus = manager.subscribe();

        let updated = manager
            .generate_title(&session, true)
            .await
            .expect("the model answered");

        assert_eq!(updated.title, "Fix the login bug");
        assert_eq!(updated.title_source, TitleSource::Generated);
        let changes = title_changes(&mut bus);
        assert_eq!(
            changes,
            vec![("Fix the login bug".to_string(), TitleSource::Generated)]
        );
        let stored = manager.get(&session).unwrap();
        assert!(stored.events.iter().any(|event| matches!(
            &event.kind,
            EventKind::TitleChanged { title, source } if title == "Fix the login bug" && *source == TitleSource::Generated
        )));
    }

    #[tokio::test]
    async fn the_title_call_carries_one_small_prompt_and_nothing_else() {
        let stub = StubProvider::new();
        let completions = Arc::clone(&stub.completions);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;
        exchange(&manager, &session);
        choose_title_model(&manager, "stub", "stub-small");

        manager.generate_title(&session, true).await.unwrap();

        let sent = lock(&completions);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].model, "stub-small");
        assert_eq!(sent[0].system, crate::sessions::title::TITLE_SYSTEM_PROMPT);
        assert_eq!(sent[0].max_tokens, 64);
        assert!(sent[0].prompt.contains("GitHub OAuth device flow"));
        assert!(sent[0].prompt.contains("short-lived user code"));
    }

    #[tokio::test]
    async fn a_failed_completion_keeps_the_placeholder_and_the_auto_source() {
        let (manager, _contexts) =
            manager_with(StubProvider::new().with_failing_completion("rate limited"));
        let session = create(&manager).await;
        exchange(&manager, &session);
        choose_title_model(&manager, "stub", "stub-small");

        let error = manager.generate_title(&session, true).await.unwrap_err();

        assert!(
            matches!(&error, SessionError::Provider(ProviderError::Api(message)) if message == "rate limited"),
            "the provider error travels unchanged: {error:?}"
        );
        let stored = manager.get(&session).unwrap();
        assert_eq!(stored.session.title, TEMPORARY_SESSION_TITLE);
        assert_eq!(stored.session.title_source, TitleSource::Auto);
        assert!(
            !stored
                .events
                .iter()
                .any(|event| matches!(event.kind, EventKind::TitleChanged { .. })),
            "a failure must not announce a title"
        );

        // The per-session guard is released on failure, so the user can retry.
        assert!(manager.generate_title(&session, true).await.is_err());
    }

    #[tokio::test]
    async fn generating_a_title_refuses_before_the_first_assistant_reply() {
        let (manager, _contexts) = manager_with(StubProvider::new());
        let session = create(&manager).await;
        append_message(&manager, &session, Role::User, "Start here");
        choose_title_model(&manager, "stub", "stub-small");

        let error = manager.generate_title(&session, true).await.unwrap_err();

        assert!(
            matches!(&error, SessionError::Invalid(message) if message.contains("assistant")),
            "{error}"
        );
        assert_eq!(
            manager.get(&session).unwrap().session.title,
            TEMPORARY_SESSION_TITLE
        );
    }

    #[tokio::test]
    async fn generating_a_title_refuses_when_no_model_is_available() {
        let (manager, _contexts) = manager_with(StubProvider::new());
        let session = create(&manager).await;
        exchange(&manager, &session);

        let error = manager.generate_title(&session, true).await.unwrap_err();

        assert!(matches!(error, SessionError::Invalid(_)), "{error}");
        assert_eq!(
            manager.get(&session).unwrap().session.title,
            TEMPORARY_SESSION_TITLE
        );
    }

    #[tokio::test]
    async fn automatic_generation_is_silent_when_titles_are_off() {
        let stub = StubProvider::new();
        let completions = Arc::clone(&stub.completions);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;
        exchange(&manager, &session);
        choose_title_model(&manager, "stub", "stub-small");
        crate::settings::SettingsService::new(manager.store())
            .set(crate::settings::SettingsPatch {
                auto_titles: Some(false),
                ..Default::default()
            })
            .expect("automatic naming can be turned off");

        let unchanged = manager.generate_title(&session, false).await.unwrap();

        assert_eq!(unchanged.title, TEMPORARY_SESSION_TITLE);
        assert_eq!(unchanged.title_source, TitleSource::Auto);
        assert!(
            lock(&completions).is_empty(),
            "Off means no completion call at all"
        );

        // The explicit menu action is a different path: it still names the session.
        let forced = manager.generate_title(&session, true).await.unwrap();
        assert_eq!(forced.title, "Fix the login bug");
        assert_eq!(lock(&completions).len(), 1);
    }

    #[tokio::test]
    async fn a_rename_during_generation_wins_over_the_model_answer() {
        let stub = StubProvider::new()
            .with_completion("\"Model title\"")
            .holding_completions();
        let completions = Arc::clone(&stub.completions);
        let gate = Arc::clone(&stub.completion_gate);
        let (manager, _contexts) = manager_with(stub);
        let session = create(&manager).await;
        exchange(&manager, &session);
        choose_title_model(&manager, "stub", "stub-small");
        let mut bus = manager.subscribe();

        let generation = manager.generate_title(&session, false);
        let rename = async {
            loop {
                if !lock(&completions).is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let renamed = manager.rename(&session, "My own title").await.unwrap();
            gate.notify_one();
            renamed
        };
        let (generated, renamed) = tokio::join!(generation, rename);

        assert_eq!(renamed.title, "My own title");
        let generated = generated.expect("the completion still returns a row");
        assert_eq!(
            generated.title, "My own title",
            "the in-flight answer must not overwrite what the user typed"
        );
        let stored = manager.get(&session).unwrap();
        assert_eq!(stored.session.title, "My own title");
        assert_eq!(stored.session.title_source, TitleSource::Manual);
        assert!(
            title_changes(&mut bus)
                .iter()
                .all(|(_, source)| *source == TitleSource::Manual),
            "no generated title is announced after the rename"
        );
    }

    #[tokio::test]
    async fn the_suggested_model_is_used_when_the_setting_is_unset() {
        let stub = StubProvider::new().for_provider("anthropic");
        let completions = Arc::clone(&stub.completions);
        let (manager, _contexts) = manager_with(stub);
        manager
            .secrets()
            .set("provider.anthropic.api_key", "test-key")
            .unwrap();
        let session = manager
            .create(CreateSessionRequest {
                provider: "anthropic".into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .session
            .id;
        exchange(&manager, &session);

        manager.generate_title(&session, true).await.unwrap();

        assert_eq!(
            lock(&completions)[0].model,
            "claude-haiku-4-5",
            "the first configured provider's suggested small model names the session"
        );
        assert_eq!(
            manager.get(&session).unwrap().session.title,
            "Fix the login bug"
        );
    }

    #[tokio::test]
    async fn a_handoff_stops_at_the_last_complete_assistant_message() {
        let (manager, _from_starts, to_starts) = manager_two();
        let session = create_on(&manager, "stub-a").await;
        manager.send(&session, "first task").await.unwrap();
        // A cancelled or failed turn can leave an assistant tool call whose results never arrived.
        manager.send(&session, "unfinished").await.unwrap();

        manager
            .switch_provider(&session, "stub-b", None)
            .await
            .expect("an idle live session hands over");

        assert_eq!(
            lock(&to_starts)[0].history,
            vec![
                Message::user("first task"),
                Message::assistant(vec![MsgBlock::Text {
                    text: "ack: first task".into()
                }]),
            ],
            "the dangling tool call and its user message are not handed to another provider"
        );
    }

    #[tokio::test]
    async fn concurrent_generations_for_one_session_are_refused() {
        let (manager, _contexts) = manager_with(StubProvider::new());
        let session = create(&manager).await;
        exchange(&manager, &session);
        choose_title_model(&manager, "stub", "stub-small");

        let (first, second) = tokio::join!(
            manager.generate_title(&session, true),
            manager.generate_title(&session, true)
        );

        let results = [&first, &second];
        let successes = results.iter().filter(|result| result.is_ok()).count();
        assert_eq!(successes, 1, "only one call may run: {first:?} {second:?}");
        let refused = results
            .iter()
            .find(|result| result.is_err())
            .expect("the second call is refused");
        assert!(
            matches!(refused, Err(SessionError::Invalid(message)) if message.contains("already")),
            "{refused:?}"
        );
    }

    #[tokio::test]
    async fn a_stopped_session_keeps_the_transcript_replay_on_a_switch() {
        let (manager, _from_starts, to_starts) = manager_two();
        let session = create_on(&manager, "stub-a").await;
        manager
            .send(&session, "remember the number 7")
            .await
            .unwrap();
        manager.close(&session).await.unwrap();

        manager
            .switch_provider(&session, "stub-b", None)
            .await
            .expect("a stopped session switches from its transcript");

        let start = &lock(&to_starts)[0];
        assert!(
            start.history.is_empty(),
            "there is no live history to export: {:?}",
            start.history
        );
        let prompt = start.prompt.as_deref().expect("the transcript replay");
        assert!(
            prompt.contains("remember the number 7"),
            "the replay carries the conversation: {prompt:?}"
        );
    }
}
