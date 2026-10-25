//! The session runtime: live sessions, their transcript, and the commands that drive them.
//!
//! A session is a row plus, while it is running, one adapter process and one pump task. The manager
//! owns both. Commands (`send`, `cancel`, `respond_permission`) go through a per-session mutex, so no
//! two writes ever interleave on one stdio pipe; events come back the other way through the pump.
//!
//! Nothing here imports `tauri`: the shell forwards events and exposes commands, and the whole runtime
//! is testable headlessly against the mock adapter.

use crate::adapters::registry::builtin_registry;
use crate::adapters::{
    AdapterDeps, AdapterError, Detection, HarnessAdapter, HarnessSession, StartContext, UserInput,
};
use crate::ipc::{
    CreateSessionRequest, EventEnvelope, HarnessInfo, IpcError, Session, SessionDetail,
    SessionSummary,
};
use crate::paths::{display_path, AppPaths};
use crate::secrets::MemorySecretStore;
use crate::sessions::context::{render_documents, ContextAssembly, ContextSource};
use crate::sessions::event::{
    now, render_tail, Block, ContextReason, Decision, EventKind, HarnessEvent, Role, SessionState,
};
use crate::sessions::pump::{self, InjectedEvent, SessionBus};
use crate::store::{Store, StoreError};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

/// A running session: the adapter handle plus the task draining its events.
struct LiveSession {
    /// Serializes every command to this session.
    commands: Mutex<Box<dyn HarnessSession>>,
    /// The pump is the single writer for this session, so manager-authored events go through it.
    inject: tokio::sync::mpsc::UnboundedSender<InjectedEvent>,
    pump: tokio::task::JoinHandle<()>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("unknown harness: {0}")]
    UnknownHarness(String),
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("session is not running: {0} (resume it first)")]
    NotRunning(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error(transparent)]
    Adapter(#[from] AdapterError),
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
            SessionError::UnknownHarness(_) | SessionError::Invalid(_) => {
                IpcError::invalid(message)
            }
            SessionError::NotRunning(_) => IpcError::unavailable(message),
            SessionError::Adapter(AdapterError::Unavailable(_))
            | SessionError::Adapter(AdapterError::Unsupported(_)) => IpcError::unavailable(message),
            SessionError::Adapter(_) => IpcError::invalid(message),
            SessionError::Store(_) | SessionError::Io(_) => IpcError::io(message),
        }
    }
}

/// How many transcript lines a harness switch replays as the new harness's opening prompt.
pub const TRANSCRIPT_SUMMARY_LINES: usize = 24;

/// Where a session's adapter config comes from besides the request itself.
///
/// `StartContext.config` is "adapter-specific, from settings" (see `docs/adapters.md`), and the one
/// setting every process-backed adapter reads is `config.command`. The manager resolves it through
/// this port so it never imports `SettingsService` — exactly like [`ContextSource`] for knowledge.
///
/// **Merge precedence is explicit and only two levels deep:** a `config.command` the caller passed on
/// the request (and which is stored on the row) always wins, because it is the more specific
/// instruction; otherwise the configured harness path in Settings fills it in; every other key in
/// `config` is passed through untouched. Nothing is merged key-by-key beyond `command`, so an
/// adapter's own defaults are never silently overwritten.
pub trait AdapterConfigSource: Send + Sync {
    /// The command a harness should launch, when the user configured one in Settings.
    fn command_for(&self, harness: &str) -> Option<String>;
}

/// Owns every live session and the store they persist into. One instance per application.
pub struct SessionManager {
    store: Arc<Store>,
    deps: AdapterDeps,
    adapters: Vec<Arc<dyn HarnessAdapter>>,
    /// Pinned specs and the plan. Absent in tests and previews that never assemble knowledge: a
    /// session then starts with an empty context rather than failing.
    context: std::sync::Mutex<Option<Arc<dyn ContextSource>>>,
    /// Configured harness binaries from settings. Absent in tests and previews: a session then
    /// launches the adapter's own default command.
    config_source: std::sync::Mutex<Option<Arc<dyn AdapterConfigSource>>>,
    live: Mutex<HashMap<String, Arc<LiveSession>>>,
    bus: SessionBus,
}

impl SessionManager {
    pub fn new(
        store: Arc<Store>,
        deps: AdapterDeps,
        adapters: Vec<Arc<dyn HarnessAdapter>>,
    ) -> Self {
        let (bus, _) = broadcast::channel(512);
        Self {
            store,
            deps,
            adapters,
            context: std::sync::Mutex::new(None),
            config_source: std::sync::Mutex::new(None),
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

    /// Attach the settings-backed command source. `GroveCore::assemble` wires it, so a harness path
    /// configured in the Settings panel reaches the next session of that harness; tests attach a
    /// stub, or nothing at all.
    pub fn set_config_source(&self, source: Arc<dyn AdapterConfigSource>) {
        let mut slot = self
            .config_source
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(source);
    }

    /// The config a session of `harness` launches with: the caller's config, plus the configured
    /// harness path as `command` when the caller named none (see [`AdapterConfigSource`] for the
    /// precedence rule).
    fn start_config(&self, harness: &str, config: serde_json::Value) -> serde_json::Value {
        let mut config = if config.is_object() {
            config
        } else {
            serde_json::json!({})
        };
        let explicit = config
            .get("command")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .is_some_and(|command| !command.is_empty());
        if !explicit {
            if let Some(configured) = self.configured_command(harness) {
                config["command"] = serde_json::Value::String(configured);
            }
        }
        config
    }

    fn configured_command(&self, harness: &str) -> Option<String> {
        let source = self
            .config_source
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()?;
        source.command_for(harness)
    }

    /// Open the durable store at `paths.database` and wire the built-in adapters. Sessions left live
    /// by a previous run are marked exited: after a restart they are conversations, not processes.
    pub fn open(paths: AppPaths) -> Result<Self, SessionError> {
        let store = Arc::new(Store::open(&paths.database)?);
        store.mark_orphaned_sessions_exited()?;
        Ok(Self::new(store, Self::deps(paths), builtin_registry()))
    }

    /// In-memory runtime for previews and tests that do not care about the file.
    pub fn in_memory(paths: &AppPaths) -> Result<Self, SessionError> {
        let store = Arc::new(Store::open_in_memory()?);
        Ok(Self::new(
            store,
            Self::deps(paths.clone()),
            builtin_registry(),
        ))
    }

    fn deps(paths: AppPaths) -> AdapterDeps {
        AdapterDeps {
            paths,
            // Adapters that need the network build their own requests; sessions never call this.
            http: reqwest::Client::new(),
            // Ticket 07 swaps the OS keychain in behind this same port.
            secrets: Arc::new(MemorySecretStore::new()),
        }
    }

    /// The shared store. Domains that own their own tables (settings, plans) go through this
    /// instead of opening a second connection to the same file.
    pub fn store(&self) -> Arc<Store> {
        Arc::clone(&self.store)
    }

    /// The event bus the shell forwards as `grove://session-event`.
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope<HarnessEvent>> {
        self.bus.subscribe()
    }

    /// Every registered harness, with honest capabilities, for the session picker.
    pub fn harnesses(&self) -> Vec<HarnessInfo> {
        self.adapters
            .iter()
            .map(|adapter| HarnessInfo {
                id: adapter.id().as_str().to_string(),
                capabilities: adapter.capabilities(),
            })
            .collect()
    }

    /// Ask every registered adapter whether its harness is installed. The doctor consumes this: it
    /// reports what the adapters themselves believe, in the same order as [`Self::harnesses`].
    pub fn detect_all(&self) -> Vec<Detection> {
        self.adapters
            .iter()
            .map(|adapter| adapter.detect(&self.deps))
            .collect()
    }

    /// The display name an adapter advertises (`Claude Code`, `Codex`, …), or `None` for a harness
    /// this build does not register. The doctor reads it from the registry rather than keeping a
    /// second list of harness names.
    pub fn label_for(&self, harness: &str) -> Option<&'static str> {
        self.adapters
            .iter()
            .find(|adapter| adapter.id().as_str() == harness)
            .map(|adapter| adapter.display_name())
    }

    pub fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
        Ok(self.store.list_sessions()?)
    }

    pub fn get(&self, id: &str) -> Result<SessionDetail, SessionError> {
        self.store
            .session_detail(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))
    }

    /// Create the row, start the adapter, and begin pumping. The transcript is durable from here on.
    pub async fn create(
        &self,
        request: CreateSessionRequest,
    ) -> Result<SessionDetail, SessionError> {
        let adapter = self.adapter(&request.harness)?;
        let id = Uuid::new_v4().to_string();
        let at = now();
        let cwd = resolve_cwd(request.cwd.as_deref(), &self.deps.paths);
        let model = non_empty(request.model);
        let title =
            non_empty(request.title).unwrap_or_else(|| format!("{} session", request.harness));
        // The request's own config is what the row keeps; the launch config additionally resolves a
        // configured harness path (see [`AdapterConfigSource`]).
        let config = request.config.unwrap_or_else(|| serde_json::json!({}));
        let launch_config = self.start_config(&request.harness, config.clone());

        let session = Session {
            id: id.clone(),
            harness: request.harness,
            cwd: display_path(&cwd),
            model: model.clone(),
            title,
            status: SessionState::Starting,
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
            prompt: None,
            resume_token: None,
            config: launch_config,
            context_files: assembly.docs.clone(),
        };
        self.publish_context(&session, ContextReason::Start, &assembly, None);
        if let Err(error) = self.launch(&adapter, &session, context).await {
            let _ = self.store.update_session_status(&id, SessionState::Error);
            return Err(error);
        }
        self.get(&id)
    }

    /// Send a user message. The message is persisted before the adapter is asked, so the transcript
    /// keeps what the user typed even if the harness dies mid-send.
    pub async fn send(&self, id: &str, text: &str) -> Result<(), SessionError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(SessionError::Invalid("message is empty".into()));
        }
        let live = self.live_session(id).await?;
        // Hold the command lock across both steps: the transcript order and the order the harness
        // sees commands are then the same, even when two sends race.
        let mut handle = live.commands.lock().await;

        let (message, persisted) =
            InjectedEvent::acknowledged(HarnessEvent::new(EventKind::Message {
                role: Role::User,
                blocks: vec![Block::Text {
                    text: text.to_string(),
                }],
            }));
        live.inject
            .send(message)
            .map_err(|_| SessionError::NotRunning(id.to_string()))?;
        // Wait until the message is durable before the harness can react to it.
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
        let adapter = self.adapter(&session.harness)?;
        let token = self.store.resume_token(id)?;
        // Re-resolve on every resume, so a harness path changed in Settings applies to the next
        // start of an existing conversation instead of only to brand-new sessions.
        let config = self.start_config(&session.harness, self.store.adapter_config(id)?);
        let events = self.store.session_events(id)?;
        self.stop(id).await;

        let assembly = self.context_assembly(id);
        let mut context = StartContext {
            cwd: PathBuf::from(&session.cwd),
            model: session.model.clone(),
            prompt: None,
            resume_token: None,
            config,
            context_files: assembly.docs.clone(),
        };
        if adapter.capabilities().resume && token.is_some() {
            context.resume_token = token;
        } else if !events.is_empty() {
            context.prompt = Some(render_tail(&events, 12));
        }

        self.mark(id, SessionState::Starting)?;
        self.publish_context(&session, ContextReason::Resume, &assembly, None);
        self.launch(&adapter, &session, context).await?;
        Ok(self.store.session(id)?.unwrap_or(session))
    }

    /// Re-read the pins and the plan and push the current knowledge into the running session.
    ///
    /// The context is a snapshot taken at start: nothing re-reads a spec on its own. This is the
    /// explicit way to move it, and it is visible twice — the harness receives the rendered documents
    /// as a turn, and the transcript records what was sent.
    pub async fn refresh_context(&self, id: &str) -> Result<ContextAssembly, SessionError> {
        let live = self.live_session(id).await?;
        let session = self
            .store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        let assembly = self.context_assembly(id);

        // Hold the command lock across both steps, exactly like `send`: the transcript note and the
        // turn the harness sees must be in the same order for every caller.
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

    /// Re-launch the same conversation on a different registered harness.
    ///
    /// The transcript is the record and stays untouched: the new harness gets the same context
    /// documents plus a summary of the conversation so far, and the switch itself is a visible system
    /// note. The resume token belongs to the old harness, so it is dropped rather than handed over.
    pub async fn switch_harness(
        &self,
        id: &str,
        harness: &str,
    ) -> Result<SessionDetail, SessionError> {
        let session = self
            .store
            .session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        if session.harness == harness {
            return Err(SessionError::Invalid(format!(
                "session {id} already runs {harness}"
            )));
        }
        let adapter = self.adapter(harness)?;
        let previous = session.harness.clone();
        let config = self.start_config(harness, self.store.adapter_config(id)?);
        let events = self.store.session_events(id)?;
        let assembly = self.context_assembly(id);
        let summary = render_tail(&events, TRANSCRIPT_SUMMARY_LINES);

        self.stop(id).await;
        // A model name belongs to the harness that understood it, and so does the resume token.
        self.store.set_session_harness(id, harness, None)?;
        self.store.set_resume_token(id, None)?;
        let session = self.store.session(id)?.unwrap_or(session);
        self.mark(id, SessionState::Starting)?;
        self.publish_context(
            &session,
            ContextReason::HarnessSwitch,
            &assembly,
            Some((previous.as_str(), harness)),
        );

        let context = StartContext {
            cwd: PathBuf::from(&session.cwd),
            model: session.model.clone(),
            prompt: (!summary.is_empty()).then_some(summary),
            resume_token: None,
            config,
            context_files: assembly.docs,
        };
        // A harness that refuses to start leaves the row on the new harness and the session stopped:
        // the conversation is intact and can be resumed (or switched again).
        if let Err(error) = self.launch(&adapter, &session, context).await {
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
        adapter: &Arc<dyn HarnessAdapter>,
        session: &Session,
        context: StartContext,
    ) -> Result<(), SessionError> {
        let mut handle = adapter.start(context, self.deps.clone()).await?;
        // The adapter may already know the harness's conversation id; mirror it now, and again after
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
            &HarnessEvent::new(EventKind::Status { state }),
        );
        Ok(())
    }

    fn mirror_resume_token(
        &self,
        id: &str,
        handle: &dyn HarnessSession,
    ) -> Result<(), SessionError> {
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

    fn adapter(&self, harness: &str) -> Result<Arc<dyn HarnessAdapter>, SessionError> {
        self.adapters
            .iter()
            .find(|adapter| adapter.id().as_str() == harness)
            .cloned()
            .ok_or_else(|| SessionError::UnknownHarness(harness.to_string()))
    }
}

/// The visible system note for a context event, or `None` when there is nothing worth recording.
///
/// A start or resume with no pins and no plan stays silent (the header shows an empty context); an
/// explicit refresh or a harness switch always says what happened, because the user asked for it.
fn context_event(
    session: &Session,
    reason: ContextReason,
    assembly: &ContextAssembly,
    switch: Option<(&str, &str)>,
) -> Option<HarnessEvent> {
    let explicit = matches!(
        reason,
        ContextReason::Refresh | ContextReason::HarnessSwitch
    );
    if assembly.docs.is_empty() && !explicit {
        return None;
    }
    Some(HarnessEvent::new(EventKind::ContextRefreshed {
        reason,
        harness: session.harness.clone(),
        documents: assembly.documents.clone(),
        note: context_note(reason, &session.harness, assembly, switch),
    }))
}

fn context_note(
    reason: ContextReason,
    harness: &str,
    assembly: &ContextAssembly,
    switch: Option<(&str, &str)>,
) -> String {
    let summary = context_summary(assembly);
    match reason {
        ContextReason::Start => format!("Context sent to {harness}: {summary}"),
        ContextReason::Resume => format!("Context re-sent on resume to {harness}: {summary}"),
        ContextReason::Refresh => format!("Context refreshed for {harness}: {summary}"),
        ContextReason::HarnessSwitch => {
            let (from, to) = switch.unwrap_or(("?", harness));
            format!("Harness switched from {from} to {to}; context re-sent: {summary}")
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
    event: HarnessEvent,
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
    use crate::adapters::Capabilities;
    use crate::adapters::Detection;
    use crate::adapters::HarnessId;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::sync::mpsc;

    /// An adapter that records how many `send` calls are ever in flight at once.
    struct SlowAdapter {
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl HarnessAdapter for SlowAdapter {
        fn id(&self) -> HarnessId {
            HarnessId("slow")
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                streaming_deltas: false,
                tool_calls: false,
                diffs: false,
                permissions: false,
                resume: false,
                images: false,
                mcp: false,
                usage: false,
            }
        }

        fn detect(&self, _deps: &AdapterDeps) -> Detection {
            Detection::installed("0.0.0", "builtin:slow")
        }

        async fn start(
            &self,
            _ctx: StartContext,
            _deps: AdapterDeps,
        ) -> Result<Box<dyn HarnessSession>, AdapterError> {
            let (keepalive, events) = mpsc::channel(4);
            Ok(Box::new(SlowSession {
                events: Some(events),
                _keepalive: keepalive,
                active: Arc::clone(&self.active),
                peak: Arc::clone(&self.peak),
            }))
        }
    }

    struct SlowSession {
        events: Option<mpsc::Receiver<HarnessEvent>>,
        _keepalive: mpsc::Sender<HarnessEvent>,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl HarnessSession for SlowSession {
        fn take_events(&mut self) -> mpsc::Receiver<HarnessEvent> {
            self.events.take().unwrap()
        }

        async fn send(&mut self, _input: UserInput) -> Result<(), AdapterError> {
            let in_flight = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(in_flight, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(25)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }

        async fn respond_permission(
            &mut self,
            _request_id: &str,
            _decision: Decision,
        ) -> Result<(), AdapterError> {
            Ok(())
        }

        async fn cancel(&mut self) -> Result<(), AdapterError> {
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), AdapterError> {
            Ok(())
        }

        fn resume_token(&self) -> Option<String> {
            None
        }
    }

    /// An adapter that records the `StartContext` of every launch, so a test can see exactly what a
    /// session was started with.
    struct RecordingAdapter {
        contexts: Arc<std::sync::Mutex<Vec<StartContext>>>,
        permissions: Arc<std::sync::Mutex<Vec<(String, Decision)>>>,
    }

    #[async_trait::async_trait]
    impl HarnessAdapter for RecordingAdapter {
        fn id(&self) -> HarnessId {
            HarnessId("record")
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                streaming_deltas: false,
                tool_calls: false,
                diffs: false,
                permissions: false,
                resume: true,
                images: false,
                mcp: false,
                usage: false,
            }
        }

        fn detect(&self, _deps: &AdapterDeps) -> Detection {
            Detection::installed("0.0.0", "builtin:record")
        }

        async fn start(
            &self,
            ctx: StartContext,
            _deps: AdapterDeps,
        ) -> Result<Box<dyn HarnessSession>, AdapterError> {
            self.contexts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(ctx);
            let (keepalive, events) = mpsc::channel(4);
            Ok(Box::new(QuietSession {
                events: Some(events),
                _keepalive: keepalive,
                permissions: Arc::clone(&self.permissions),
            }))
        }
    }

    /// Starts and does nothing: enough to observe the launch without noise in the transcript.
    struct QuietSession {
        events: Option<mpsc::Receiver<HarnessEvent>>,
        _keepalive: mpsc::Sender<HarnessEvent>,
        permissions: Arc<std::sync::Mutex<Vec<(String, Decision)>>>,
    }

    #[async_trait::async_trait]
    impl HarnessSession for QuietSession {
        fn take_events(&mut self) -> mpsc::Receiver<HarnessEvent> {
            self.events.take().unwrap()
        }

        async fn send(&mut self, _input: UserInput) -> Result<(), AdapterError> {
            Ok(())
        }

        async fn respond_permission(
            &mut self,
            request_id: &str,
            decision: Decision,
        ) -> Result<(), AdapterError> {
            self.permissions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push((request_id.to_string(), decision));
            Ok(())
        }

        async fn cancel(&mut self) -> Result<(), AdapterError> {
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), AdapterError> {
            Ok(())
        }

        fn resume_token(&self) -> Option<String> {
            None
        }
    }

    /// Settings stand-in: one harness has a configured binary.
    struct StubPaths {
        harness: &'static str,
        command: &'static str,
    }

    impl AdapterConfigSource for StubPaths {
        fn command_for(&self, harness: &str) -> Option<String> {
            (harness == self.harness).then(|| self.command.to_string())
        }
    }

    fn recording() -> (SessionManager, Arc<std::sync::Mutex<Vec<StartContext>>>) {
        let (manager, contexts, _permissions) = recording_with_permissions();
        (manager, contexts)
    }

    /// What a recording adapter captured: the launch contexts, and every permission answer.
    type Recorded = (
        SessionManager,
        Arc<std::sync::Mutex<Vec<StartContext>>>,
        Arc<std::sync::Mutex<Vec<(String, Decision)>>>,
    );

    fn recording_with_permissions() -> Recorded {
        let contexts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let permissions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let adapter = Arc::new(RecordingAdapter {
            contexts: Arc::clone(&contexts),
            permissions: Arc::clone(&permissions),
        });
        (manager(adapter), contexts, permissions)
    }

    fn recorded(contexts: &Arc<std::sync::Mutex<Vec<StartContext>>>) -> Vec<StartContext> {
        contexts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    #[tokio::test]
    async fn a_permission_answer_reaches_the_live_session() {
        // The Tauri command is a one-line forward to this call, so proving the manager delivers the
        // answer to the adapter is what proves the button in the transcript can unblock a turn.
        let (manager, _contexts, permissions) = recording_with_permissions();
        let id = manager
            .create(CreateSessionRequest {
                harness: "record".into(),
                ..Default::default()
            })
            .await
            .expect("session is created")
            .session
            .id;

        manager
            .respond_permission(&id, "req-1", Decision::Approve)
            .await
            .expect("the answer is delivered");
        manager
            .respond_permission(&id, "req-2", Decision::Deny)
            .await
            .expect("the answer is delivered");

        assert_eq!(
            *permissions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            vec![
                ("req-1".to_string(), Decision::Approve),
                ("req-2".to_string(), Decision::Deny),
            ]
        );
    }

    #[tokio::test]
    async fn a_configured_harness_path_reaches_the_session_on_create_and_resume() {
        let (manager, contexts) = recording();
        manager.set_config_source(Arc::new(StubPaths {
            harness: "record",
            command: "C:/configured/record.exe",
        }));

        let id = manager
            .create(CreateSessionRequest {
                harness: "record".into(),
                ..Default::default()
            })
            .await
            .expect("session is created")
            .session
            .id;

        let launched = recorded(&contexts);
        assert_eq!(
            launched[0].config["command"], "C:/configured/record.exe",
            "a path configured in Settings is the command the session launches"
        );

        manager.close(&id).await.unwrap();
        manager.resume(&id).await.unwrap();

        let launched = recorded(&contexts);
        assert_eq!(
            launched.len(),
            2,
            "create and resume both launch the adapter"
        );
        assert_eq!(
            launched[1].config["command"], "C:/configured/record.exe",
            "a resume re-resolves the configured path instead of using a stale one"
        );
    }

    #[tokio::test]
    async fn an_explicit_request_config_beats_the_configured_harness_path() {
        let (manager, contexts) = recording();
        manager.set_config_source(Arc::new(StubPaths {
            harness: "record",
            command: "C:/configured/record.exe",
        }));

        manager
            .create(CreateSessionRequest {
                harness: "record".into(),
                config: Some(serde_json::json!({
                    "command": "C:/fixtures/node.exe",
                    "args": ["fixture.mjs"],
                })),
                ..Default::default()
            })
            .await
            .expect("session is created");

        let launched = recorded(&contexts);
        assert_eq!(
            launched[0].config["command"], "C:/fixtures/node.exe",
            "the more specific request config wins"
        );
        assert_eq!(launched[0].config["args"][0], "fixture.mjs");
    }

    #[tokio::test]
    async fn without_a_configured_path_the_request_config_is_passed_through_untouched() {
        let (manager, contexts) = recording();

        manager
            .create(CreateSessionRequest {
                harness: "record".into(),
                config: Some(serde_json::json!({ "chunkDelayMs": 5 })),
                ..Default::default()
            })
            .await
            .expect("session is created");

        let launched = recorded(&contexts);
        assert_eq!(
            launched[0].config,
            serde_json::json!({ "chunkDelayMs": 5 }),
            "no settings, no injected keys: the adapter keeps its own defaults"
        );
    }

    fn manager(adapter: Arc<dyn HarnessAdapter>) -> SessionManager {
        let paths = AppPaths::new(std::env::temp_dir().join("grove-manager-tests"));
        let deps = AdapterDeps {
            paths,
            http: reqwest::Client::new(),
            secrets: Arc::new(MemorySecretStore::new()),
        };
        SessionManager::new(
            Arc::new(Store::open_in_memory().unwrap()),
            deps,
            vec![adapter],
        )
    }

    fn slow() -> (SessionManager, Arc<AtomicUsize>) {
        let peak = Arc::new(AtomicUsize::new(0));
        let manager = manager(Arc::new(SlowAdapter {
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::clone(&peak),
        }));
        (manager, peak)
    }

    async fn create(manager: &SessionManager) -> String {
        manager
            .create(CreateSessionRequest {
                harness: "slow".into(),
                ..Default::default()
            })
            .await
            .expect("session is created")
            .session
            .id
    }

    #[test]
    fn detect_all_asks_every_adapter_in_registry_order() {
        let (manager, _peak) = slow();
        let harnesses = manager.harnesses();
        let detections = manager.detect_all();
        assert_eq!(harnesses.len(), detections.len());
        assert_eq!(detections[0], Detection::installed("0.0.0", "builtin:slow"));
    }

    #[tokio::test]
    async fn concurrent_commands_for_one_session_are_serialized() {
        let (manager, peak) = slow();
        let id = create(&manager).await;

        let (first, second) = tokio::join!(manager.send(&id, "one"), manager.send(&id, "two"));
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
        let (manager, _peak) = slow();
        let id = create(&manager).await;

        manager.send(&id, "  hello  ").await.unwrap();

        let detail = manager.get(&id).unwrap();
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
        let (manager, _peak) = slow();

        let missing = manager.send("missing", "hi").await.unwrap_err();
        assert!(matches!(missing, SessionError::NotFound(_)));

        let id = create(&manager).await;
        manager.close(&id).await.unwrap();

        let stopped = manager.send(&id, "hi").await.unwrap_err();
        assert!(matches!(stopped, SessionError::NotRunning(_)));

        let detail = manager.get(&id).unwrap();
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
    async fn unknown_harness_is_rejected_without_a_row() {
        let (manager, _peak) = slow();
        let error = manager
            .create(CreateSessionRequest {
                harness: "nope".into(),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert!(matches!(error, SessionError::UnknownHarness(_)));
        assert!(manager.list().unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_messages_are_rejected() {
        let (manager, _peak) = slow();
        let id = create(&manager).await;
        let error = manager.send(&id, "   ").await.unwrap_err();
        assert!(matches!(error, SessionError::Invalid(_)));
    }
}
