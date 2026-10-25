//! Session knowledge integration (ticket 14): pinned specs plus the current plan become the
//! `StartContext.context_files` a harness receives, the context is a snapshot, a refresh pushes the
//! current knowledge into the running session, and switching harness keeps the transcript.
//!
//! Nothing here shells out: a test-only recording adapter captures the exact `StartContext` and every
//! `UserInput` the manager produced, which is how "the fake harness receives this context" is proven.

use grove_core::adapters::{
    AdapterDeps, AdapterError, Capabilities, ContextDoc, Detection, HarnessAdapter, HarnessId,
    HarnessSession, StartContext, UserInput,
};
use grove_core::ipc::{CreateSessionRequest, ErrorCode, IpcError, Session};
use grove_core::paths::AppPaths;
use grove_core::plan::{PlanService, PlanUpdate};
use grove_core::secrets::MemorySecretStore;
use grove_core::sessions::context::{
    ContextAssembly, ContextLimits, ContextSource, SessionContextService,
};
use grove_core::sessions::event::{
    Block, ContextDocument, ContextReason, EventKind, HarnessEvent, Role, SessionState,
};
use grove_core::sessions::manager::{SessionError, SessionManager};
use grove_core::store::Store;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

// ---------- the recording adapter ----------

/// What the harness was launched with, and what it was told afterwards.
#[derive(Clone, Default)]
struct Recorder {
    starts: Arc<Mutex<Vec<StartContext>>>,
    inputs: Arc<Mutex<Vec<String>>>,
}

impl Recorder {
    fn starts(&self) -> Vec<StartContext> {
        self.starts.lock().unwrap().clone()
    }

    fn inputs(&self) -> Vec<String> {
        self.inputs.lock().unwrap().clone()
    }
}

struct RecordingAdapter {
    id: &'static str,
    recorder: Recorder,
}

#[async_trait::async_trait]
impl HarnessAdapter for RecordingAdapter {
    fn id(&self) -> HarnessId {
        HarnessId(self.id)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            streaming_deltas: false,
            tool_calls: false,
            diffs: false,
            permissions: false,
            // No resume: the manager replays the transcript tail instead, which is what a harness
            // switch has to do too.
            resume: false,
            images: false,
            mcp: false,
            usage: false,
        }
    }

    fn detect(&self, _deps: &AdapterDeps) -> Detection {
        Detection::installed("1.0.0", format!("builtin:{}", self.id))
    }

    async fn start(
        &self,
        ctx: StartContext,
        _deps: AdapterDeps,
    ) -> Result<Box<dyn HarnessSession>, AdapterError> {
        self.recorder.starts.lock().unwrap().push(ctx.clone());
        let (events, receiver) = mpsc::channel(32);
        events
            .try_send(HarnessEvent::new(EventKind::SessionStarted {
                harness: self.id.to_string(),
                model: ctx.model.clone(),
                resumed: ctx.resume_token.is_some(),
            }))
            .expect("the buffer holds the startup events");
        events
            .try_send(HarnessEvent::new(EventKind::Status {
                state: SessionState::Ready,
            }))
            .expect("the buffer holds the startup events");
        Ok(Box::new(RecordingSession {
            events: Some(receiver),
            _keepalive: events,
            recorder: self.recorder.clone(),
        }))
    }
}

struct RecordingSession {
    events: Option<mpsc::Receiver<HarnessEvent>>,
    /// Keeps the pump's receiver open: a closed channel means "the harness exited".
    _keepalive: mpsc::Sender<HarnessEvent>,
    recorder: Recorder,
}

#[async_trait::async_trait]
impl HarnessSession for RecordingSession {
    fn take_events(&mut self) -> mpsc::Receiver<HarnessEvent> {
        self.events.take().expect("take_events is called once")
    }

    async fn send(&mut self, input: UserInput) -> Result<(), AdapterError> {
        self.recorder.inputs.lock().unwrap().push(input.text);
        Ok(())
    }

    async fn respond_permission(
        &mut self,
        _request_id: &str,
        _decision: grove_core::sessions::event::Decision,
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

/// A source the test drives by hand: proves the manager's half of the contract without touching the
/// spec graph.
struct StubSource(ContextAssembly);

impl ContextSource for StubSource {
    fn documents(&self, _session_id: &str) -> ContextAssembly {
        self.0.clone()
    }
}

// ---------- fixture ----------

struct Harness {
    dir: tempfile::TempDir,
    store: Arc<Store>,
    plan: Arc<PlanService>,
    context: Arc<SessionContextService>,
    manager: SessionManager,
    recorder: Recorder,
}

fn deps(paths: AppPaths) -> AdapterDeps {
    AdapterDeps {
        paths,
        http: reqwest::Client::new(),
        secrets: Arc::new(MemorySecretStore::new()),
    }
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open_in_memory().unwrap());
    let plan = Arc::new(PlanService::new(
        Arc::clone(&store),
        dir.path().to_path_buf(),
    ));
    let context = Arc::new(SessionContextService::new(
        Arc::clone(&store),
        Arc::clone(&plan),
        dir.path().to_path_buf(),
    ));
    let recorder = Recorder::default();
    let manager = SessionManager::new(
        Arc::clone(&store),
        deps(AppPaths::new(dir.path())),
        vec![
            Arc::new(RecordingAdapter {
                id: "recorder-a",
                recorder: recorder.clone(),
            }),
            Arc::new(RecordingAdapter {
                id: "recorder-b",
                recorder: recorder.clone(),
            }),
        ],
    );
    manager.set_context_source(Arc::clone(&context) as Arc<dyn ContextSource>);
    Harness {
        dir,
        store,
        plan,
        context,
        manager,
        recorder,
    }
}

fn write_spec(root: &Path, file: &str, id: &str, title: &str, body: &str) {
    let source = format!("---\nid: {id}\ntype: module-design\ntitle: {title}\n---\n\n{body}\n");
    std::fs::write(root.join(file), source).unwrap();
}

fn session_row(id: &str, project: &Path, harness: &str) -> Session {
    Session {
        id: id.into(),
        harness: harness.into(),
        cwd: grove_core::paths::display_path(project),
        model: None,
        title: "Knowledge".into(),
        status: SessionState::Idle,
        created_at: "2025-01-01T00:00:00.000Z".into(),
        updated_at: "2025-01-01T00:00:00.000Z".into(),
    }
}

fn seed_session(harness: &Harness, id: &str, adapter: &str) {
    harness
        .store
        .insert_session(
            &session_row(id, harness.dir.path(), adapter),
            &serde_json::json!({}),
        )
        .unwrap();
}

async fn wait_for_event(
    manager: &SessionManager,
    id: &str,
    done: impl Fn(&EventKind) -> bool,
) -> Vec<HarnessEvent> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let events = manager.get(id).expect("the session exists").events;
        if events.iter().any(|event| done(&event.kind)) {
            return events;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the transcript never showed the expected event; last events: {events:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

fn context_events(events: &[HarnessEvent]) -> Vec<(ContextReason, String, Vec<ContextDocument>)> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::ContextRefreshed {
                reason,
                note,
                documents,
                ..
            } => Some((*reason, note.clone(), documents.clone())),
            _ => None,
        })
        .collect()
}

// ---------- tests ----------

#[tokio::test]
async fn a_new_session_starts_with_the_documents_the_context_source_provides() {
    let harness = harness();
    let assembly = ContextAssembly {
        docs: vec![ContextDoc {
            id: "spec-a".into(),
            title: "Spec A".into(),
            body: "# Spec A\n\nThe body.".into(),
        }],
        documents: vec![ContextDocument {
            id: "spec-a".into(),
            title: "Spec A".into(),
            chars: 20,
            truncated: false,
        }],
        omitted: 0,
        total_chars: 20,
    };
    harness
        .manager
        .set_context_source(Arc::new(StubSource(assembly.clone())));

    let detail = harness
        .manager
        .create(CreateSessionRequest {
            harness: "recorder-a".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let starts = harness.recorder.starts();
    assert_eq!(starts.len(), 1);
    assert_eq!(
        starts[0].context_files, assembly.docs,
        "the adapter receives exactly the assembled documents"
    );

    let recorded = context_events(&detail.events);
    assert_eq!(recorded.len(), 1, "the start is recorded in the transcript");
    assert_eq!(recorded[0].0, ContextReason::Start);
    assert_eq!(recorded[0].2, assembly.documents);
    assert!(recorded[0].1.contains("recorder-a"), "{}", recorded[0].1);
}

#[tokio::test]
async fn resume_assembles_pinned_specs_and_the_plan_into_the_start_context() {
    let harness = harness();
    write_spec(
        harness.dir.path(),
        "a.md",
        "spec-a",
        "Spec A",
        "# Spec A\n\nThe first pinned spec.",
    );
    seed_session(&harness, "session-1", "recorder-a");
    harness.context.pin_spec("session-1", "spec-a").unwrap();
    harness
        .plan
        .set(
            "session-1",
            PlanUpdate::new("# Plan\n\n- ship ticket 14\n", 0),
        )
        .unwrap();

    harness.manager.resume("session-1").await.unwrap();

    let starts = harness.recorder.starts();
    assert_eq!(starts.len(), 1);
    let docs = &starts[0].context_files;
    assert_eq!(
        docs.iter().map(|doc| doc.id.as_str()).collect::<Vec<_>>(),
        vec!["spec-a", "plan"],
        "pinned specs first, then the plan"
    );
    assert_eq!(docs[0].title, "Spec A");
    assert!(docs[0].body.contains("The first pinned spec."));
    assert_eq!(docs[1].title, "Plan");
    assert!(docs[1].body.contains("ship ticket 14"));

    let events = wait_for_event(&harness.manager, "session-1", |kind| {
        matches!(kind, EventKind::ContextRefreshed { .. })
    })
    .await;
    let recorded = context_events(&events);
    assert_eq!(recorded[0].0, ContextReason::Resume);
    assert_eq!(
        recorded[0]
            .2
            .iter()
            .map(|doc| doc.id.as_str())
            .collect::<Vec<_>>(),
        vec!["spec-a", "plan"]
    );
}

#[tokio::test]
async fn the_context_is_a_snapshot_until_it_is_refreshed() {
    let harness = harness();
    write_spec(
        harness.dir.path(),
        "a.md",
        "spec-a",
        "Spec A",
        "# Spec A\n\noriginal",
    );
    seed_session(&harness, "session-1", "recorder-a");
    harness.context.pin_spec("session-1", "spec-a").unwrap();
    harness.manager.resume("session-1").await.unwrap();

    // The agent or the user edits the spec while the session runs. Nothing is re-sent.
    write_spec(
        harness.dir.path(),
        "a.md",
        "spec-a",
        "Spec A",
        "# Spec A\n\nedited",
    );
    assert_eq!(
        harness.recorder.starts().len(),
        1,
        "editing a spec must not restart a session"
    );
    assert!(harness.recorder.starts()[0].context_files[0]
        .body
        .contains("original"));

    // An explicit refresh is the only thing that moves the context.
    harness.manager.refresh_context("session-1").await.unwrap();
    assert_eq!(
        harness.recorder.starts().len(),
        1,
        "a refresh pushes the context into the running session instead of relaunching it"
    );
    let sent = harness.recorder.inputs();
    assert_eq!(sent.len(), 1, "one turn carries the whole context");
    assert!(sent[0].contains("## Spec A"), "{}", sent[0]);
    assert!(sent[0].contains("edited"), "{}", sent[0]);
    assert!(
        !sent[0].contains("original"),
        "the refreshed context is the current one: {}",
        sent[0]
    );

    let events = wait_for_event(&harness.manager, "session-1", |kind| {
        matches!(
            kind,
            EventKind::ContextRefreshed {
                reason: ContextReason::Refresh,
                ..
            }
        )
    })
    .await;
    let recorded = context_events(&events);
    assert_eq!(recorded.len(), 2, "start and refresh are both recorded");
    assert!(
        recorded[1].1.contains("Context refreshed"),
        "{}",
        recorded[1].1
    );
}

#[tokio::test]
async fn switching_harness_reloads_the_same_context_with_a_transcript_summary() {
    let harness = harness();
    write_spec(
        harness.dir.path(),
        "a.md",
        "spec-a",
        "Spec A",
        "# Spec A\n\nThe spec.",
    );
    let detail = harness
        .manager
        .create(CreateSessionRequest {
            harness: "recorder-a".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let id = detail.session.id.clone();
    harness.context.pin_spec(&id, "spec-a").unwrap();
    harness.manager.send(&id, "first task").await.unwrap();
    let expected = harness.context.assembly(&id).unwrap().docs;

    let switched = harness
        .manager
        .switch_harness(&id, "recorder-b")
        .await
        .unwrap();

    assert_eq!(switched.session.id, id, "the session keeps its identity");
    assert_eq!(switched.session.harness, "recorder-b");
    let starts = harness.recorder.starts();
    assert_eq!(starts.len(), 2, "the session was relaunched once");
    assert_eq!(
        starts[1].context_files, expected,
        "the same context documents, reassembled from the pins"
    );
    assert!(
        starts[1].resume_token.is_none(),
        "a token is harness-specific"
    );
    let prompt = starts[1].prompt.as_deref().expect("a transcript summary");
    assert!(prompt.contains("user: first task"), "{prompt}");

    // The transcript is intact and gained one visible note.
    let events = harness.manager.get(&id).unwrap().events;
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Message { role: Role::User, blocks }
                if blocks == &vec![Block::Text { text: "first task".into() }]
        )),
        "the conversation so far is still there: {events:?}"
    );
    let recorded = context_events(&events);
    let (reason, note, documents) = recorded
        .iter()
        .find(|(reason, _, _)| *reason == ContextReason::HarnessSwitch)
        .expect("the switch is visible in the transcript");
    assert_eq!(*reason, ContextReason::HarnessSwitch);
    assert!(note.contains("recorder-a"), "{note}");
    assert!(note.contains("recorder-b"), "{note}");
    assert_eq!(
        documents
            .iter()
            .map(|doc| doc.id.as_str())
            .collect::<Vec<_>>(),
        vec!["spec-a"]
    );
}

#[tokio::test]
async fn pins_survive_a_restart_and_come_back_in_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("grove.db");
    write_spec(
        dir.path(),
        "a.md",
        "spec-a",
        "Spec A",
        "# Spec A\n\nThe spec.",
    );

    {
        let store = Arc::new(Store::open(&database).unwrap());
        store
            .insert_session(
                &session_row("session-1", dir.path(), "recorder-a"),
                &serde_json::json!({}),
            )
            .unwrap();
        let plan = Arc::new(PlanService::new(
            Arc::clone(&store),
            dir.path().to_path_buf(),
        ));
        let context =
            SessionContextService::new(Arc::clone(&store), plan, dir.path().to_path_buf());
        context.pin_spec("session-1", "spec-a").unwrap();
        // The manager (and the app) go away here.
    }

    let store = Arc::new(Store::open(&database).unwrap());
    let plan = Arc::new(PlanService::new(
        Arc::clone(&store),
        dir.path().to_path_buf(),
    ));
    let context = Arc::new(SessionContextService::new(
        Arc::clone(&store),
        plan,
        dir.path().to_path_buf(),
    ));
    let recorder = Recorder::default();
    let manager = SessionManager::new(
        Arc::clone(&store),
        deps(AppPaths::new(dir.path())),
        vec![Arc::new(RecordingAdapter {
            id: "recorder-a",
            recorder: recorder.clone(),
        })],
    );
    manager.set_context_source(context as Arc<dyn ContextSource>);

    manager.resume("session-1").await.unwrap();

    let starts = recorder.starts();
    assert_eq!(
        starts[0]
            .context_files
            .iter()
            .map(|doc| doc.id.as_str())
            .collect::<Vec<_>>(),
        vec!["spec-a"],
        "the pin was durable, so the restart starts with the spec"
    );
}

#[tokio::test]
async fn the_budgets_are_applied_to_the_starting_context() {
    let mut harness = harness();
    let store = Arc::clone(&harness.store);
    let plan = Arc::clone(&harness.plan);
    let limits = ContextLimits {
        per_doc_chars: 200,
        total_chars: 400,
    };
    let context = Arc::new(
        SessionContextService::new(store, plan, harness.dir.path().to_path_buf())
            .with_limits(limits),
    );
    harness
        .manager
        .set_context_source(Arc::clone(&context) as Arc<dyn ContextSource>);
    harness.context = context;

    write_spec(
        harness.dir.path(),
        "a.md",
        "spec-a",
        "Spec A",
        &"a".repeat(5_000),
    );
    seed_session(&harness, "session-1", "recorder-a");
    harness.context.pin_spec("session-1", "spec-a").unwrap();
    harness.manager.resume("session-1").await.unwrap();

    let body = &harness.recorder.starts()[0].context_files[0].body;
    assert!(body.chars().count() <= 200, "{}", body.chars().count());
    assert!(body.ends_with(" chars omitted]"), "the marker is explicit");
}

#[tokio::test]
async fn refreshing_a_session_that_is_not_running_is_unavailable() {
    let harness = harness();
    seed_session(&harness, "session-1", "recorder-a");

    let error = harness
        .manager
        .refresh_context("session-1")
        .await
        .unwrap_err();
    assert_eq!(IpcError::from(error).code, ErrorCode::Unavailable);
    assert!(matches!(
        harness
            .manager
            .refresh_context("missing")
            .await
            .unwrap_err(),
        SessionError::NotFound(_)
    ));
}

#[tokio::test]
async fn switching_to_an_unknown_harness_leaves_the_session_alone() {
    let harness = harness();
    let detail = harness
        .manager
        .create(CreateSessionRequest {
            harness: "recorder-a".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let id = detail.session.id;

    let error = harness
        .manager
        .switch_harness(&id, "not-a-harness")
        .await
        .unwrap_err();
    assert_eq!(IpcError::from(error).code, ErrorCode::Invalid);
    assert_eq!(
        harness.manager.get(&id).unwrap().session.harness,
        "recorder-a",
        "a rejected switch changes nothing"
    );
    assert_eq!(harness.recorder.starts().len(), 1, "nothing was relaunched");

    let same = harness
        .manager
        .switch_harness(&id, "recorder-a")
        .await
        .unwrap_err();
    assert!(matches!(same, SessionError::Invalid(_)));
}

#[tokio::test]
async fn a_session_with_no_pins_and_no_plan_starts_with_an_empty_context() {
    let harness = harness();
    seed_session(&harness, "session-1", "recorder-a");
    harness.manager.resume("session-1").await.unwrap();

    assert!(harness.recorder.starts()[0].context_files.is_empty());
    let events = wait_for_event(&harness.manager, "session-1", |kind| {
        matches!(kind, EventKind::SessionStarted { .. })
    })
    .await;
    assert!(
        context_events(&events).is_empty(),
        "nothing to report means no note in the transcript"
    );
    let tags: Vec<&str> = events.iter().map(|event| event.kind.tag()).collect();
    assert!(
        tags.iter()
            .all(|tag| *tag == "status" || *tag == "session_started"),
        "the transcript gained no context note about nothing: {tags:?}"
    );
}
