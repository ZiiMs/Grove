//! Restart survival, once per registered adapter.
//!
//! Every harness takes the same path through [`SessionManager`] against its own fixture: create a
//! session, send a turn, let the transcript land in SQLite, close the session (the app quitting),
//! reopen the store at the same path, resume, and keep talking. The transcript must come back exactly
//! as it was written and the adapter must be handed the resume token it stored — the capability test
//! below fails if an adapter claims `resume` without being able to reuse its token.
//!
//! The fixture logic lives in `adapter_support/*`; this file only supplies the path.

pub mod adapter_support;

use adapter_support::{acp_fixture, claude_fixture, codex_fixture, opencode_fixture, pi_fixture};
use grove_core::adapters::Capabilities;
use grove_core::ipc::CreateSessionRequest;
use grove_core::paths::AppPaths;
use grove_core::sessions::event::{Block, EventKind, HarnessEvent, Role, SessionState};
use grove_core::sessions::manager::SessionManager;
use serde_json::{json, Value};
use std::time::Duration;
use tempfile::TempDir;

/// Node startup plus an OpenCode server bring-up on a cold CI runner.
const WAIT: Duration = Duration::from_secs(30);
const FIRST: &str = "restart matrix: first turn";
const SECOND: &str = "restart matrix: second turn";

/// One adapter plus the fixture configuration a session stores in its row.
struct Case {
    harness: &'static str,
    config: Value,
    /// The OpenCode fixture's session store has to outlive the restarted server.
    _state: Option<TempDir>,
}

impl Case {
    fn new(harness: &'static str, config: Value) -> Self {
        Self {
            harness,
            config,
            _state: None,
        }
    }

    fn resumable(harness: &'static str, config: Value, state: TempDir) -> Self {
        Self {
            harness,
            config,
            _state: Some(state),
        }
    }
}

fn mock_case() -> Case {
    Case::new("mock", json!({ "chunkDelayMs": 1 }))
}

fn claude_case() -> Case {
    Case::new("claude", claude_fixture::config("tools", &[]))
}

fn acp_case() -> Case {
    Case::new("acp", acp_fixture::config("happy", &[]))
}

fn pi_case() -> Case {
    Case::new("pi", pi_fixture::config("tools", &[]))
}

fn codex_case() -> Case {
    Case::new("codex", codex_fixture::config("happy", &[]))
}

fn opencode_case() -> Case {
    let state = tempfile::tempdir().expect("a temp dir for the fixture's session store");
    Case::resumable(
        "opencode",
        opencode_fixture::config("happy", state.path(), &["--delay-ms", "2"]),
        state,
    )
}

fn is_state(event: &HarnessEvent, state: SessionState) -> bool {
    matches!(&event.kind, EventKind::Status { state: current } if *current == state)
}

fn idle_count(events: &[HarnessEvent]) -> usize {
    events
        .iter()
        .filter(|event| is_state(event, SessionState::Idle))
        .count()
}

fn assistant_messages(events: &[HarnessEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                &event.kind,
                EventKind::Message {
                    role: Role::Assistant,
                    ..
                }
            )
        })
        .count()
}

fn user_texts(events: &[HarnessEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Message {
                role: Role::User,
                blocks,
            } => Some(
                blocks
                    .iter()
                    .filter_map(|block| match block {
                        Block::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect()
}

fn started_count(events: &[HarnessEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::SessionStarted { .. }))
        .count()
}

async fn wait_for(
    manager: &SessionManager,
    harness: &str,
    id: &str,
    done: impl Fn(&[HarnessEvent]) -> bool,
) -> Vec<HarnessEvent> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let events = manager
            .get(id)
            .unwrap_or_else(|error| panic!("[{harness}] session {id} is readable: {error}"))
            .events;
        if done(&events) {
            return events;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "[{harness}] the transcript never reached the expected state; last events: {:?}",
            events
                .iter()
                .map(|event| event.kind.tag())
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn restart_round_trip(case: Case) {
    let harness = case.harness;
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app-data"));
    paths.ensure().expect("create app data");

    let (session_id, token, before, closed) = {
        let manager = SessionManager::open(paths.clone()).expect("the store opens");
        let detail = manager
            .create(CreateSessionRequest {
                harness: harness.to_string(),
                config: Some(case.config.clone()),
                ..Default::default()
            })
            .await
            .unwrap_or_else(|error| panic!("[{harness}] create: {error}"));
        let id = detail.session.id;

        manager
            .send(&id, FIRST)
            .await
            .unwrap_or_else(|error| panic!("[{harness}] first send: {error}"));
        let before = wait_for(&manager, harness, &id, |events| {
            idle_count(events) >= 1
                && assistant_messages(events) >= 1
                && manager.store().resume_token(&id).unwrap().is_some()
        })
        .await;
        assert!(
            user_texts(&before).contains(&FIRST),
            "[{harness}] the first turn is in the transcript: {:?}",
            user_texts(&before)
        );
        let token = manager
            .store()
            .resume_token(&id)
            .expect("the token column reads")
            .unwrap_or_else(|| panic!("[{harness}] claims resume but stored no token"));

        // The app quits: the process goes away, the row and the transcript stay.
        manager
            .close(&id)
            .await
            .unwrap_or_else(|error| panic!("[{harness}] close: {error}"));
        let closed = manager.get(&id).expect("the closed session reads").events;
        assert_eq!(
            &closed[..before.len()],
            &before[..],
            "[{harness}] closing only appends to the transcript"
        );
        assert_eq!(
            closed.len(),
            before.len() + 1,
            "[{harness}] close records exactly one exit: {:?}",
            closed
                .iter()
                .map(|event| event.kind.tag())
                .collect::<Vec<_>>()
        );
        assert!(
            is_state(closed.last().unwrap(), SessionState::Exited),
            "[{harness}] the last event is the exit"
        );
        assert_eq!(
            manager.get(&id).unwrap().session.status,
            SessionState::Exited,
            "[{harness}] the row says the process is gone"
        );
        assert!(
            paths.database.exists(),
            "[{harness}] the transcript lives in one SQLite file"
        );
        (id, token, before, closed)
    };

    // Reopen the same file: this is the next launch of the app.
    let manager = SessionManager::open(paths).expect("the store reopens");
    let restored = manager
        .get(&session_id)
        .expect("the session survives")
        .events;
    assert_eq!(
        restored, closed,
        "[{harness}] the transcript comes back exactly as it was written"
    );
    assert_eq!(
        manager
            .store()
            .resume_token(&session_id)
            .unwrap()
            .as_deref(),
        Some(token.as_str()),
        "[{harness}] the resume token survives the restart"
    );
    assert!(before.len() < restored.len());

    // Resume: the adapter is handed the stored token and continues the same conversation.
    let resumed = manager
        .resume(&session_id)
        .await
        .unwrap_or_else(|error| panic!("[{harness}] resume: {error}"));
    assert_eq!(resumed.id, session_id);
    assert_eq!(resumed.harness, harness);

    let after_resume = wait_for(&manager, harness, &session_id, |events| {
        started_count(events) >= 2
    })
    .await;
    assert!(
        after_resume
            .iter()
            .any(|event| matches!(&event.kind, EventKind::SessionStarted { resumed: true, .. })),
        "[{harness}] the adapter was restarted with its resume token: {:?}",
        after_resume
            .iter()
            .map(|event| event.kind.tag())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        &after_resume[..restored.len()],
        &restored[..],
        "[{harness}] resume appends to the transcript instead of rewriting it"
    );
    assert_eq!(
        manager
            .store()
            .resume_token(&session_id)
            .unwrap()
            .as_deref(),
        Some(token.as_str()),
        "[{harness}] the resumed conversation keeps the harness's id"
    );

    manager
        .send(&session_id, SECOND)
        .await
        .unwrap_or_else(|error| panic!("[{harness}] second send: {error}"));
    let after = wait_for(&manager, harness, &session_id, |events| {
        idle_count(events) >= 2 && assistant_messages(events) >= 2
    })
    .await;

    assert_eq!(
        user_texts(&after),
        vec![FIRST, SECOND],
        "[{harness}] both turns are in one transcript"
    );
    assert_eq!(
        &after[..restored.len()],
        &restored[..],
        "[{harness}] the pre-restart transcript is intact after the second turn"
    );
    assert_eq!(
        manager.get(&session_id).unwrap().session.status,
        SessionState::Idle,
        "[{harness}] the session works after resume"
    );
}

macro_rules! restart_case {
    ($name:ident, $case:expr) => {
        #[tokio::test]
        async fn $name() {
            restart_round_trip($case).await;
        }
    };
}

restart_case!(mock_survives_a_restart_and_resumes, mock_case());
restart_case!(claude_survives_a_restart_and_resumes, claude_case());
restart_case!(acp_survives_a_restart_and_resumes, acp_case());
restart_case!(pi_survives_a_restart_and_resumes, pi_case());
restart_case!(opencode_survives_a_restart_and_resumes, opencode_case());
restart_case!(codex_survives_a_restart_and_resumes, codex_case());

/// The eight capability flags, in a fixed order, for a readable table below. A flag is named only
/// when it is true; the rest stay false, exactly as the adapter reported them.
#[derive(Debug, Default, PartialEq, Eq)]
struct Flags {
    streaming_deltas: bool,
    tool_calls: bool,
    diffs: bool,
    permissions: bool,
    resume: bool,
    images: bool,
    mcp: bool,
    usage: bool,
}

impl From<&Capabilities> for Flags {
    fn from(capabilities: &Capabilities) -> Self {
        Self {
            streaming_deltas: capabilities.streaming_deltas,
            tool_calls: capabilities.tool_calls,
            diffs: capabilities.diffs,
            permissions: capabilities.permissions,
            resume: capabilities.resume,
            images: capabilities.images,
            mcp: capabilities.mcp,
            usage: capabilities.usage,
        }
    }
}

macro_rules! flags {
    ($($flag:ident),* $(,)?) => {
        Flags { $($flag: true,)* ..Flags::default() }
    };
}

/// The picker is what the user sees: every registered harness, with the adapter's own capability
/// answers. This is the cross-adapter half of the per-adapter capability unit tests.
#[test]
fn the_picker_lists_every_harness_with_its_own_capabilities() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::in_memory(&AppPaths::new(dir.path())).expect("runtime");

    let picker = manager.harnesses();
    let ids: Vec<&str> = picker.iter().map(|info| info.id.as_str()).collect();
    assert_eq!(
        ids,
        ["mock", "claude", "acp", "pi", "opencode", "codex"],
        "the five harnesses stay selectable and the mock stays available for offline demos"
    );

    let capabilities: Vec<(&str, Flags)> = picker
        .iter()
        .map(|info| (info.id.as_str(), Flags::from(&info.capabilities)))
        .collect();
    // Each flag is the adapter's honest answer (tickets 08–13 record why the false ones are false).
    assert_eq!(
        capabilities,
        vec![
            ("mock", flags!(streaming_deltas, tool_calls, resume, usage)),
            (
                "claude",
                flags!(streaming_deltas, tool_calls, resume, usage)
            ),
            (
                "acp",
                flags!(streaming_deltas, tool_calls, permissions, resume)
            ),
            ("pi", flags!(streaming_deltas, tool_calls, resume, usage)),
            (
                "opencode",
                flags!(streaming_deltas, tool_calls, diffs, resume, usage)
            ),
            (
                "codex",
                flags!(
                    streaming_deltas,
                    tool_calls,
                    diffs,
                    permissions,
                    resume,
                    usage
                )
            ),
        ],
        "the picker must not advertise a capability the adapter does not emit"
    );
    assert_eq!(
        manager.detect_all().len(),
        picker.len(),
        "the doctor reports the same harnesses the picker offers"
    );
}
