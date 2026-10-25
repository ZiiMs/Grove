//! The session runtime: live sessions, their transcript, and the commands that drive them.
//!
//! A session is a row plus, while it is running, one in-process agent and one pump task. The manager
//! owns both. Commands (`send`, `cancel`, `respond_permission`) go through a per-session mutex, so a
//! session never has two turns in flight; events come back the other way through the pump.
//!
//! Nothing here imports `tauri`: the shell forwards events and exposes commands, and the whole runtime
//! is testable headlessly.

use crate::ipc::{
    CreateSessionRequest, EventEnvelope, IpcError, ProviderStatus, Session, SessionDetail,
    SessionSummary,
};
use crate::paths::{display_path, AppPaths};
use crate::providers::agent::registry as provider_registry;
use crate::providers::credentials::Credentials;
use crate::providers::{
    AgentProvider, AgentSession, Detection, ProviderDeps, ProviderError, ProviderSpec,
    StartContext, UserInput,
};
use crate::secrets::MemorySecretStore;
use crate::sessions::context::{render_documents, ContextAssembly, ContextSource};
use crate::sessions::event::{
    now, render_tail, Block, ContextReason, Decision, EventKind, Role, SessionEvent, SessionState,
};
use crate::sessions::pump::{self, InjectedEvent, SessionBus};
use crate::store::{Store, StoreError};
use std::collections::HashMap;
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
    pump: tokio::task::JoinHandle<()>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("session is not running: {0} (resume it first)")]
    NotRunning(String),
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
            SessionError::NotRunning(_) => IpcError::unavailable(message),
            SessionError::Provider(ProviderError::Unavailable(_)) => IpcError::unavailable(message),
            SessionError::Provider(_) => IpcError::invalid(message),
            SessionError::Store(_) | SessionError::Io(_) => IpcError::io(message),
        }
    }
}

/// How many transcript lines a provider switch replays as the new provider's opening prompt.
pub const TRANSCRIPT_SUMMARY_LINES: usize = 24;

/// Owns every live session and the store they persist into. One instance per application.
pub struct SessionManager {
    store: Arc<Store>,
    deps: ProviderDeps,
    providers: Vec<Arc<dyn AgentProvider>>,
    /// Pinned specs and the plan. Absent in tests and previews that never assemble knowledge: a
    /// session then starts with an empty context rather than failing.
    context: std::sync::Mutex<Option<Arc<dyn ContextSource>>>,
    live: Mutex<HashMap<String, Arc<LiveSession>>>,
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
        let title =
            non_empty(request.title).unwrap_or_else(|| format!("{} session", request.provider));
        // The request's own config is what the row keeps and what the launch uses.
        let config = request.config.unwrap_or_else(|| serde_json::json!({}));

        let session = Session {
            id: id.clone(),
            provider: request.provider,
            cwd: display_path(&cwd),
            model: model.clone(),
            title,
            status: SessionState::Starting,
            project_id: project.map(|project| project.id),
            awaiting: false,
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

    /// Stop the process but keep the row and the transcript. The session can be resumed later.
    pub async fn close(&self, id: &str) -> Result<(), SessionError> {
        if self.store.session(id)?.is_none() {
            return Err(SessionError::NotFound(id.to_string()));
        }
        self.stop(id).await;
        self.mark(id, SessionState::Exited)
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
    /// The transcript is the record and stays untouched: the new provider gets the same context
    /// documents plus a summary of the conversation so far, and the change itself is a visible system
    /// note. The resume token belongs to the old provider (and model), so it is dropped rather than
    /// handed over. A same-provider call with a new model is the composer's model picker.
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
        if same_provider {
            // The provider did not change; this is a context/model refresh rather than a switch.
            self.publish_context(&session, ContextReason::Refresh, &assembly, None);
        } else {
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
            prompt: (!summary.is_empty()).then_some(summary),
            resume_token: None,
            config,
            context_files: assembly.docs,
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
            pump,
        });
        if let Some(previous) = self.live.lock().await.insert(session.id.clone(), live) {
            previous.pump.abort();
        }
        // A launched agent is ready for its first message. Without this transition the row stays
        // `starting`, and a composer that queues while a turn is in flight never sends anything.
        self.mark(&session.id, SessionState::Ready)?;
        Ok(())
    }

    async fn stop(&self, id: &str) {
        if let Some(live) = self.live.lock().await.remove(id) {
            {
                let mut handle = live.commands.lock().await;
                let _ = handle.shutdown().await;
            }
            live.pump.abort();
        }
    }

    /// Record a transition the adapter did not emit (close, restart), exactly once.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::catalog;
    use crate::providers::{AuthMethod, Capabilities, Detection, ProviderSpec};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;
    use tokio::sync::mpsc;

    /// A provider that records what the manager does to it, without touching the network.
    struct StubProvider {
        spec: ProviderSpec,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
        contexts: Arc<StdMutex<Vec<StartContext>>>,
        permissions: Arc<StdMutex<Vec<(String, Decision)>>>,
        /// Whether a credential counts as configured (drives `detect`).
        ready: bool,
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
                ready: true,
            }
        }

        fn with_ready(mut self, ready: bool) -> Self {
            self.ready = ready;
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

        async fn start(
            &self,
            ctx: StartContext,
            _deps: ProviderDeps,
        ) -> Result<Box<dyn AgentSession>, ProviderError> {
            lock(&self.contexts).push(ctx);
            let (keepalive, events) = mpsc::channel(4);
            Ok(Box::new(StubSession {
                events: Some(events),
                _keepalive: keepalive,
                active: Arc::clone(&self.active),
                peak: Arc::clone(&self.peak),
                permissions: Arc::clone(&self.permissions),
            }))
        }
    }

    struct StubSession {
        events: Option<mpsc::Receiver<SessionEvent>>,
        _keepalive: mpsc::Sender<SessionEvent>,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
        permissions: Arc<StdMutex<Vec<(String, Decision)>>>,
    }

    #[async_trait::async_trait]
    impl AgentSession for StubSession {
        fn take_events(&mut self) -> mpsc::Receiver<SessionEvent> {
            self.events.take().expect("called once")
        }

        async fn send(&mut self, _input: UserInput) -> Result<(), ProviderError> {
            let in_flight = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(in_flight, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(25)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }

        async fn respond_permission(
            &mut self,
            request_id: &str,
            decision: Decision,
        ) -> Result<(), ProviderError> {
            lock(&self.permissions).push((request_id.to_string(), decision));
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

    fn manager_with(stub: StubProvider) -> (SessionManager, Arc<StdMutex<Vec<StartContext>>>) {
        let contexts = Arc::clone(&stub.contexts);
        let paths = AppPaths::new(std::env::temp_dir().join("grove-manager-tests"));
        let deps = ProviderDeps {
            paths,
            http: reqwest::Client::new(),
            secrets: Arc::new(MemorySecretStore::new()),
        };
        let manager = SessionManager::new(
            Arc::new(Store::open_in_memory().unwrap()),
            deps,
            vec![Arc::new(stub) as Arc<dyn AgentProvider>],
        );
        (manager, contexts)
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
    }
}
