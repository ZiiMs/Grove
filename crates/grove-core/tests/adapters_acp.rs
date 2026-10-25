//! The generic ACP adapter against `fixtures/harnesses/acp_fake.mjs`: no real agent, no network, one
//! test per fixture mode plus the answers, the resume roundtrip, detection and the config override.
//! The shared conformance suite is in `tests/adapters_conformance.rs`.

pub mod adapter_support;

use adapter_support::acp_fixture::{config, context};
use adapter_support::runtime::{deps, drain_until, drain_until_state, next_event, tags};
use grove_core::adapters::{
    AcpAdapter, AdapterError, HarnessAdapter, HarnessSession, StartContext, UserInput,
};
use grove_core::harness_doctor::{DoctorConfig, HarnessDoctor};
use grove_core::ipc::{CreateSessionRequest, ErrorCode, IpcError};
use grove_core::paths::AppPaths;
use grove_core::sessions::event::{
    Block, Decision, EventKind, HarnessEvent, Role, SessionState, ToolStatus,
};
use grove_core::sessions::manager::{SessionError, SessionManager};
use grove_core::settings::SettingsService;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

async fn start(mode: &str, extra: &[&str]) -> Box<dyn HarnessSession> {
    AcpAdapter::default()
        .start(context(mode, extra), deps())
        .await
        .expect("the adapter starts: a configured command is all an ACP session needs")
}

/// Every `Delta` and assistant `Message` text, so a test can assert what the harness really said.
fn reply_text(events: &[HarnessEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Delta { text } => Some(text.clone()),
            EventKind::Message {
                role: Role::Assistant,
                blocks,
            } => Some(
                blocks
                    .iter()
                    .map(Block::as_text)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn permission_ids(events: &[HarnessEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::PermissionRequest { request_id, .. } => Some(request_id.clone()),
            _ => None,
        })
        .collect()
}

fn resolved(events: &[HarnessEvent]) -> Vec<(String, Decision)> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::PermissionResolved {
                request_id,
                decision,
            } => Some((request_id.clone(), *decision)),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn happy_mode_maps_chunks_tools_and_ignores_unknown_updates() {
    let mut session = start("happy", &[]).await;
    let mut rx = session.take_events();
    assert!(
        session.resume_token().is_none(),
        "a fresh session has no conversation id until the agent answers session/new"
    );

    session
        .send(UserInput::text("hello acp"))
        .await
        .expect("send reaches the fixture");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert_eq!(
        events[0].kind,
        EventKind::SessionStarted {
            harness: "acp".into(),
            model: Some("acp-fake-1".into()),
            resumed: false,
        }
    );
    assert!(kinds.contains(&"delta"), "{kinds:?}");
    assert!(kinds.contains(&"message"), "{kinds:?}");
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        }),
        "the turn ends idle: {kinds:?}"
    );

    let text = reply_text(&events);
    assert!(
        text.contains("Working on it: "),
        "agent_message_chunk becomes a delta: {text:?}"
    );
    assert!(
        text.contains("Fake reply: hello acp"),
        "the prompt reached the agent as a text block: {text:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Message { blocks, .. }
                if blocks.iter().any(|block| matches!(block, Block::Thinking { .. }))
        )),
        "agent_thought_chunk becomes a thinking block: {kinds:?}"
    );

    let calls: Vec<(String, String, String)> = events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::ToolCall {
                call_id,
                name,
                title,
            } => Some((call_id.clone(), name.clone(), title.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 2, "two tool calls: {calls:?}");
    assert_eq!(calls[0].0, "call_001");
    assert_eq!(calls[0].1, "read_file");
    assert_eq!(calls[0].2, "Reading the configuration file");
    assert_eq!(calls[1].1, "bash");
    assert_eq!(calls[1].2, "Run: cargo test -p grove-core");

    let results: Vec<(String, ToolStatus, String)> = events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::ToolResult {
                call_id,
                status,
                summary,
            } => Some((call_id.clone(), *status, summary.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 2, "two terminal tool updates: {results:?}");
    assert_eq!(results[0].0, "call_001");
    assert_eq!(results[0].1, ToolStatus::Ok);
    assert!(results[0].2.contains("config loaded"), "{}", results[0].2);
    assert_eq!(results[1].0, "call_002");
    assert_eq!(results[1].1, ToolStatus::Error);
    assert!(results[1].2.contains("one test failed"), "{}", results[1].2);

    assert!(
        !kinds.contains(&"diff"),
        "diffs are not claimed and not synthesized: {kinds:?}"
    );
    assert!(
        !kinds.contains(&"usage"),
        "usage_update reports context-window fill, not token counts: {kinds:?}"
    );
    assert!(
        !kinds.contains(&"error"),
        "unknown update variants are logged, never fatal: {kinds:?}"
    );

    let token = session
        .resume_token()
        .expect("the ACP session id is captured as the resume token");
    assert!(token.starts_with("fake-acp-session-"), "{token}");
}

#[tokio::test]
async fn a_permission_request_is_answered_with_the_selected_option() {
    let mut session = start("permission", &[]).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("run a command"))
        .await
        .unwrap();

    let before = drain_until(&mut rx, |event| {
        matches!(event.kind, EventKind::PermissionRequest { .. })
    })
    .await;
    let (request_id, title, detail) = before
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::PermissionRequest {
                request_id,
                title,
                detail,
            } => Some((request_id.clone(), title.clone(), detail.clone())),
            _ => None,
        })
        .expect("the permission request reaches the transcript");
    assert_eq!(title, "Run: rm -rf build");
    assert!(
        detail.contains("Allow once") && detail.contains("Reject"),
        "the options are described for the user: {detail}"
    );

    session
        .respond_permission(&request_id, Decision::Approve)
        .await
        .expect("an answer is accepted while the request is pending");

    let after = drain_until_state(&mut rx, SessionState::Idle).await;
    assert_eq!(
        resolved(&after),
        vec![(request_id.clone(), Decision::Approve)],
        "the answer is recorded: {:?}",
        tags(&after)
    );
    // The fixture only continues when the answer really is
    // `{"outcome":{"outcome":"selected","optionId":"allow-once"}}`; it exits 5 otherwise.
    let text = reply_text(&after);
    assert!(
        text.contains("Selected allow-once."),
        "the agent saw the selected option id: {text:?}"
    );
    assert!(
        after.iter().any(|event| matches!(
            &event.kind,
            EventKind::ToolResult { call_id, status: ToolStatus::Ok, .. } if call_id == "call_perm"
        )),
        "the approved tool call runs: {:?}",
        tags(&after)
    );
    assert_eq!(
        after.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        })
    );

    let unknown = session
        .respond_permission("no-such-request", Decision::Approve)
        .await
        .expect_err("an answer without a pending request is rejected, never a panic");
    assert!(matches!(unknown, AdapterError::Other(_)), "got {unknown:?}");
}

#[tokio::test]
async fn a_cancelled_turn_answers_pending_permissions_with_cancelled() {
    let mut session = start("cancel", &["--delay-ms", "25"]).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("a long slow answer"))
        .await
        .unwrap();

    let startup = drain_until(&mut rx, |event| {
        matches!(event.kind, EventKind::PermissionRequest { .. })
    })
    .await;
    let request_id = permission_ids(&startup)
        .pop()
        .expect("the cancel mode asks before it acts");

    session.cancel().await.expect("cancel is delivered");
    let after = drain_until_state(&mut rx, SessionState::Idle).await;

    assert_eq!(
        resolved(&after),
        vec![(request_id, Decision::Deny)],
        "a cancelled turn resolves the open request: {:?}",
        tags(&after)
    );
    // The fixture waits for `{"outcome":{"outcome":"cancelled"}}` and exits 5 for anything else.
    let text = reply_text(&after);
    assert!(
        text.contains("Permission cancelled."),
        "the agent saw the cancelled outcome: {text:?}"
    );
    assert!(
        !after
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "a cancel is not an error: {:?}",
        tags(&after)
    );
    assert!(
        !startup.iter().chain(after.iter()).any(|event| matches!(
            event.kind,
            EventKind::Message {
                role: Role::Assistant,
                ..
            }
        )),
        "the cancelled turn produced no assistant message"
    );
    assert_eq!(
        after.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        })
    );

    // And the session keeps working afterwards.
    session
        .send(UserInput::text("after the cancel"))
        .await
        .unwrap();
    let resumed = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        reply_text(&resumed).contains("After the cancel: after the cancel"),
        "the agent answers the next prompt: {:?}",
        tags(&resumed)
    );
}

#[tokio::test]
async fn a_version_mismatch_is_a_clear_error_and_error_status_not_a_hang() {
    let mut session = start("version-mismatch", &[]).await;
    let mut rx = session.take_events();
    let events = drain_until_state(&mut rx, SessionState::Error).await;
    let kinds = tags(&events);

    let message = events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::Error { message, retryable } => Some((message.clone(), *retryable)),
            _ => None,
        })
        .expect("a rejected protocol version is an error event");
    assert!(
        message.0.contains("version 2") && message.0.contains("version 1"),
        "the error names both versions: {}",
        message.0
    );
    assert!(!message.1, "a version mismatch cannot be retried as-is");
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Error
        }),
        "the session ends in `error` instead of hanging: {kinds:?}"
    );

    // The session is really over: further commands are rejected, and the agent process is gone.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if session.send(UserInput::text("too late")).await.is_err() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "a session that ended in `error` must reject commands rather than wait forever"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_dying_agent_is_a_retryable_error_and_exited_status() {
    let mut session = start("crash", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("go")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Exited).await;

    let message = events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::Error { message, retryable } => Some((message.clone(), *retryable)),
            _ => None,
        })
        .expect("an agent that dies mid-turn is an error event");
    assert!(
        message.1,
        "the captured session id lets a resume continue: {}",
        message.0
    );
    assert!(
        message.0.contains("exited"),
        "the message explains what happened: {}",
        message.0
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Exited
        }),
        "a dead agent is `exited`, not left busy: {:?}",
        tags(&events)
    );
}

#[tokio::test]
async fn unknown_lines_and_unhandled_requests_are_skipped_not_fatal() {
    let mut session = start("unknown-line", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("hello")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "junk lines, unknown notifications and unknown variants are skipped: {kinds:?}"
    );
    assert!(
        reply_text(&events).contains("Still here: hello"),
        "the valid frames still reach the transcript: {:?}",
        reply_text(&events)
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        }),
        "the fixture only answers the prompt once its unknown request was answered: {kinds:?}"
    );

    session.send(UserInput::text("still alive")).await.unwrap();
    let more = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        reply_text(&more).contains("Still here: still alive"),
        "the session still works after malformed input: {:?}",
        tags(&more)
    );
}

#[tokio::test]
async fn the_acp_session_id_is_the_resume_token_and_survives_a_restart() {
    let mut first = start("happy", &[]).await;
    let mut first_rx = first.take_events();
    first
        .send(UserInput::text("first turn"))
        .await
        .expect("send");
    drain_until_state(&mut first_rx, SessionState::Idle).await;
    let token = first
        .resume_token()
        .expect("the ACP session id becomes the resume token");
    assert!(token.starts_with("fake-acp-session-"), "{token}");
    let _ = first.shutdown().await;

    let mut context = context("happy", &[]);
    context.resume_token = Some(token.clone());
    let mut second = AcpAdapter::default()
        .start(context, deps())
        .await
        .expect("a resumed session starts");
    let mut second_rx = second.take_events();
    let started = next_event(&mut second_rx).await;
    assert_eq!(
        started.kind,
        EventKind::SessionStarted {
            harness: "acp".into(),
            model: Some("acp-fake-1".into()),
            resumed: true,
        }
    );
    assert_eq!(
        second.resume_token().as_deref(),
        Some(token.as_str()),
        "the stored session id is available before the first turn"
    );

    second
        .send(UserInput::text("resumed turn"))
        .await
        .expect("send");
    let events = drain_until_state(&mut second_rx, SessionState::Idle).await;
    assert!(
        reply_text(&events).contains("Fake reply: resumed turn"),
        "the resumed conversation answers: {:?}",
        tags(&events)
    );
    assert_eq!(
        second.resume_token().as_deref(),
        Some(token.as_str()),
        "session/resume keeps the same id; session/load replays and is rejected by the fixture"
    );
    let _ = second.shutdown().await;
}

#[test]
fn detection_reports_a_missing_command_without_spawning_anything() {
    let missing = AcpAdapter::default().detect(&deps());
    assert!(
        !missing.installed,
        "a generic adapter has no binary to auto-detect: {missing:?}"
    );
    assert!(
        missing.version.is_none(),
        "detection never runs a binary: {missing:?}"
    );
    let detail = missing.detail.unwrap_or_default();
    assert!(
        detail.contains("command") && detail.contains("Settings"),
        "the reason says how to configure it: {detail}"
    );

    let node = AcpAdapter::with_command("node").detect(&deps());
    assert!(
        node.installed,
        "an explicit command resolves on PATH: {node:?}"
    );
    assert!(node.version.is_none());
    assert!(node.path.is_some());

    let wrong = AcpAdapter::with_command("grove-definitely-not-a-real-binary").detect(&deps());
    assert!(!wrong.installed);
    assert!(
        wrong
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("grove-definitely-not-a-real-binary")),
        "the reason names the missing command: {wrong:?}"
    );
}

fn no_command_context() -> StartContext {
    StartContext {
        cwd: std::env::temp_dir(),
        model: None,
        prompt: None,
        resume_token: None,
        config: json!({}),
        context_files: Vec::new(),
    }
}

#[tokio::test]
async fn starting_without_a_configured_command_is_unavailable_not_a_panic() {
    let error = match AcpAdapter::default()
        .start(no_command_context(), deps())
        .await
    {
        Ok(_) => panic!("there is no agent to launch"),
        Err(error) => error,
    };
    assert!(
        matches!(error, AdapterError::Unavailable(_)),
        "an unconfigured agent is unavailable, got {error:?}"
    );
    assert!(
        error.to_string().contains("command"),
        "the message names the missing setting: {error}"
    );

    // Through the real runtime the failure is an `unavailable` IPC error, and the row says so.
    let dir = tempfile::tempdir().expect("temp dir");
    let manager = SessionManager::open(AppPaths::new(dir.path())).expect("the store opens");
    let error = manager
        .create(CreateSessionRequest {
            harness: "acp".into(),
            cwd: Some(std::env::temp_dir().to_string_lossy().to_string()),
            model: None,
            title: None,
            config: Some(json!({})),
        })
        .await
        .expect_err("a session cannot start without an agent");
    assert!(
        matches!(error, SessionError::Adapter(AdapterError::Unavailable(_))),
        "got {error:?}"
    );
    let ipc: IpcError = error.into();
    assert_eq!(ipc.code, ErrorCode::Unavailable);
    let sessions = manager.list().expect("the row is recorded");
    assert_eq!(
        sessions[0].session.status,
        SessionState::Error,
        "the row is marked error, not left starting"
    );
}

#[tokio::test]
async fn the_config_command_and_args_override_the_adapter_defaults() {
    // The adapter's own detection hint is deliberately useless: config.command is what launches.
    let adapter = AcpAdapter::with_command("grove-definitely-not-a-real-binary");
    let mut session = adapter
        .start(context("happy", &[]), deps())
        .await
        .expect("start never depends on detection");
    let mut rx = session.take_events();
    session
        .send(UserInput::text("via the fixture"))
        .await
        .unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        reply_text(&events).contains("Fake reply: via the fixture"),
        "{:?}",
        tags(&events)
    );
}

/// The argv the adapter passes is exactly `config.command` + `config.args`: ACP says nothing about
/// an agent's own flags, so Grove must not invent any.
#[test]
fn the_config_is_the_whole_command_line() {
    let config = config("happy", &["--delay-ms", "5"]);
    let command = config
        .get("command")
        .and_then(|value| value.as_str())
        .unwrap();
    let args: Vec<&str> = config
        .get("args")
        .and_then(|value| value.as_array())
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(command, "node");
    assert_eq!(args.len(), 5, "{args:?}");
    assert!(args[0].ends_with("acp_fake.mjs"), "{args:?}");
    assert_eq!(&args[1..], ["--mode", "happy", "--delay-ms", "5"]);
}

/// Ticket 11's doctor line: a generic adapter is configured, never auto-detected.
#[tokio::test]
async fn the_doctor_reports_acp_as_configurable_rather_than_installed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let empty = dir.path().join("empty-path");
    std::fs::create_dir_all(&empty).expect("empty PATH dir");
    let manager =
        Arc::new(SessionManager::open(AppPaths::new(dir.path())).expect("the store opens"));
    let settings = Arc::new(SettingsService::new(manager.store()));
    let doctor = HarnessDoctor::with_config(
        settings,
        Arc::clone(&manager),
        DoctorConfig {
            path_env: Some(empty.to_string_lossy().to_string()),
            path_extensions: None,
            timeout: Duration::from_millis(500),
        },
    );

    let report = doctor.report().await;
    let acp = report
        .harnesses
        .iter()
        .find(|harness| harness.id == "acp")
        .expect("the ACP adapter is registered");
    assert!(!acp.installed, "no ACP agent is guessed at: {acp:?}");
    assert!(
        acp.detail
            .as_deref()
            .is_some_and(|detail| detail.contains("command") && detail.contains("Settings")),
        "the doctor explains how to configure it: {acp:?}"
    );
}
