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
use crate::sessions::event::{
    now, render_tail, Block, Decision, EventKind, HarnessEvent, Role, SessionState,
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

/// Owns every live session and the store they persist into. One instance per application.
pub struct SessionManager {
    store: Arc<Store>,
    deps: AdapterDeps,
    adapters: Vec<Arc<dyn HarnessAdapter>>,
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
            live: Mutex::new(HashMap::new()),
            bus,
        }
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
        let config = request.config.unwrap_or_else(|| serde_json::json!({}));

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

        let context = StartContext {
            cwd,
            model,
            prompt: None,
            resume_token: None,
            config,
            context_files: Vec::new(),
        };
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
        let config = self.store.adapter_config(id)?;
        let events = self.store.session_events(id)?;
        self.stop(id).await;

        let mut context = StartContext {
            cwd: PathBuf::from(&session.cwd),
            model: session.model.clone(),
            prompt: None,
            resume_token: None,
            config,
            context_files: Vec::new(),
        };
        if adapter.capabilities().resume && token.is_some() {
            context.resume_token = token;
        } else if !events.is_empty() {
            context.prompt = Some(render_tail(&events, 12));
        }

        self.mark(id, SessionState::Starting)?;
        self.launch(&adapter, &session, context).await?;
        Ok(self.store.session(id)?.unwrap_or(session))
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
        let (inject, injected) = tokio::sync::mpsc::unbounded_channel();
        let pump = pump::spawn(
            Arc::clone(&self.store),
            self.bus.clone(),
            session.id.clone(),
            events,
            injected,
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
