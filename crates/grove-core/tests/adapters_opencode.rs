//! The OpenCode adapter against `fixtures/harnesses/opencode_fake.mjs`: a real local HTTP + SSE
//! server, no real `opencode` install and no external network. One test per fixture mode, plus the
//! modes only this adapter has: attach, readiness failure, reconnect, resume and orphan reaping.
//!
//! The shared conformance suite is in `tests/adapters_conformance.rs`.

pub mod adapter_support;

use adapter_support::opencode_fixture::{config, context, FIXTURE};
use adapter_support::runtime::{
    deps, drain_until, drain_until_state, fixture, is_state, next_event, tags,
};
use grove_core::adapters::registry::builtin_registry;
use grove_core::adapters::{
    AdapterError, HarnessAdapter, HarnessSession, OpenCodeAdapter, StartContext, UserInput,
};
use grove_core::harness_doctor::HarnessDoctor;
use grove_core::ipc::CreateSessionRequest;
use grove_core::paths::AppPaths;
use grove_core::sessions::event::{
    Block, Decision, EventKind, HarnessEvent, Role, SessionState, ToolStatus,
};
use grove_core::sessions::manager::SessionManager;
use grove_core::settings::SettingsService;
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Start a session against a fixture mode. Detection never gates starting, so the adapter's own
/// command is never used: `config.command` points at the fixture.
async fn start(config: Value) -> Box<dyn HarnessSession> {
    OpenCodeAdapter::default()
        .start(
            StartContext {
                cwd: std::env::temp_dir(),
                model: Some("anthropic/claude-sonnet-4".into()),
                prompt: None,
                resume_token: None,
                config,
                context_files: Vec::new(),
            },
            deps(),
        )
        .await
        .expect("start never blocks on detection: the server is brought up in the actor")
}

fn assistant_text(events: &[HarnessEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match &event.kind {
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

fn deltas(events: &[HarnessEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Delta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn diffs(events: &[HarnessEvent]) -> Vec<(String, String, u32, u32)> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Diff {
                path,
                patch,
                added,
                removed,
            } => Some((path.clone(), patch.clone(), *added, *removed)),
            _ => None,
        })
        .collect()
}

fn errors(events: &[HarnessEvent]) -> Vec<(String, bool)> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Error { message, retryable } => Some((message.clone(), *retryable)),
            _ => None,
        })
        .collect()
}

fn read_log(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn log_entries(path: &Path, event: &str) -> Vec<Value> {
    read_log(path)
        .into_iter()
        .filter(|entry| entry["event"] == event)
        .collect()
}

fn read_heartbeat(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn arg(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// A fake OpenCode server started *outside* the adapter: the attach-mode peer. It is killed when the
/// guard drops, never by the adapter.
struct Attached {
    child: std::process::Child,
    base_url: String,
    heartbeat: PathBuf,
}

impl Attached {
    fn start(dir: &Path, mode: &str, delay_ms: &str) -> Self {
        let port_file = dir.join("attach-port.txt");
        let heartbeat = dir.join("attach-heartbeat.txt");
        let child = std::process::Command::new("node")
            .arg(fixture(FIXTURE))
            .args([
                "serve",
                "--mode",
                mode,
                "--port",
                "0",
                "--delay-ms",
                delay_ms,
                "--heartbeat-ms",
                "50",
            ])
            .arg("--port-file")
            .arg(&port_file)
            .arg("--heartbeat-file")
            .arg(&heartbeat)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the fake server starts");

        let deadline = Instant::now() + Duration::from_secs(20);
        let port = loop {
            if let Ok(raw) = std::fs::read_to_string(&port_file) {
                if let Ok(port) = raw.trim().parse::<u16>() {
                    break port;
                }
            }
            assert!(
                Instant::now() < deadline,
                "the fixture never wrote its port to {port_file:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        };

        Self {
            child,
            base_url: format!("http://127.0.0.1:{port}"),
            heartbeat,
        }
    }

    fn heartbeat(&self) -> String {
        read_heartbeat(&self.heartbeat)
    }
}

impl Drop for Attached {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn wait_for(
    manager: &SessionManager,
    id: &str,
    done: impl Fn(&[HarnessEvent]) -> bool,
) -> Vec<HarnessEvent> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let events = manager.get(id).expect("the session exists").events;
        if done(&events) {
            return events;
        }
        assert!(
            Instant::now() < deadline,
            "the transcript never reached the expected state: {:?}",
            tags(&events)
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn happy_mode_maps_text_tools_a_diff_and_usage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("requests.jsonl");
    let mut config = config(
        "happy",
        dir.path(),
        &["--delay-ms", "5", "--log-file", &arg(&log)],
    );
    config["readyTimeoutMs"] = json!(15_000);

    let mut session = start(config).await;
    let mut rx = session.take_events();

    let refused = session
        .respond_permission("perm-1", Decision::Approve)
        .await
        .expect_err("permission prompts have no answer channel in this adapter");
    assert!(
        matches!(refused, AdapterError::Unsupported(_)),
        "an unsupported operation is reported as Unsupported, got {refused:?}"
    );

    session
        .send(UserInput::text("hello opencode"))
        .await
        .expect("the prompt reaches the server");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert_eq!(
        kinds.first(),
        Some(&"session_started"),
        "the transcript opens with session_started: {kinds:?}"
    );
    match &events[0].kind {
        EventKind::SessionStarted {
            harness,
            model,
            resumed,
        } => {
            assert_eq!(harness, "opencode");
            assert_eq!(model.as_deref(), Some("anthropic/claude-sonnet-4"));
            assert!(!resumed);
        }
        other => panic!("the first event is session_started: {other:?}"),
    }
    for expected in [
        "delta",
        "message",
        "tool_call",
        "tool_result",
        "diff",
        "usage",
    ] {
        assert!(
            kinds.contains(&expected),
            "the happy turn emits {expected}: {kinds:?}"
        );
    }
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        }),
        "the turn terminates idle: {kinds:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| is_state(event, SessionState::Exited)),
        "a completed turn is idle, never exited: {kinds:?}"
    );

    assert_eq!(deltas(&events), "Hello there", "deltas stream in order");
    let text = assistant_text(&events);
    assert!(text.contains("Hello there"), "{text}");
    assert!(
        !text.contains("ECHO:"),
        "the user's own message part is not rendered as an assistant turn: {text}"
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
    assert_eq!(
        calls.len(),
        2,
        "one call per tool, not one per update: {calls:?}"
    );
    assert_eq!(
        calls[0],
        ("call_read".into(), "read".into(), "Read README.md".into())
    );
    assert_eq!(calls[1].0, "call_edit");

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
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(results[0].0, "call_read");
    assert_eq!(results[0].1, ToolStatus::Ok);
    assert!(results[0].2.contains("# Grove"), "{}", results[0].2);

    let found = diffs(&events);
    assert!(
        found.iter().any(|(path, patch, added, removed)| {
            path == "src/main.rs"
                && patch.contains("+    let answer = 42;")
                && *added == 1
                && *removed == 0
        }),
        "the edit tool's patch becomes a diff: {found:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event.kind,
            EventKind::Usage {
                input_tokens: 37,
                output_tokens: 12
            }
        )),
        "step-finish tokens become usage: {kinds:?}"
    );

    let token = session
        .resume_token()
        .expect("the server's session id is the resume token");
    assert!(token.starts_with("ses_"), "{token}");

    // The fixture is the oracle for what the adapter actually did on the wire.
    let listening = log_entries(&log, "listening");
    assert_eq!(listening.len(), 1, "{listening:?}");
    assert_eq!(listening[0]["hostname"], "127.0.0.1", "loopback only");
    let port = listening[0]["port"].as_u64().expect("a chosen port");
    assert!(port > 0);
    assert_ne!(port, 4096, "the port is chosen dynamically, never assumed");

    let prompts = log_entries(&log, "prompt");
    assert_eq!(prompts.len(), 1, "{prompts:?}");
    assert_eq!(prompts[0]["text"], "hello opencode");
    assert_eq!(
        prompts[0]["model"]["providerID"], "anthropic",
        "the model is sent as provider/model: {prompts:?}"
    );
    assert_eq!(prompts[0]["model"]["modelID"], "claude-sonnet-4");

    let _ = session.shutdown().await;
}

#[tokio::test]
async fn session_diff_entries_become_diff_events_with_a_real_patch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut config = config("diff", dir.path(), &["--delay-ms", "5"]);
    config["readyTimeoutMs"] = json!(15_000);

    let mut session = start(config).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("edit two files"))
        .await
        .expect("send");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let found = diffs(&events);
    assert_eq!(
        found.len(),
        3,
        "two session.diff entries plus the edit tool: {found:?}"
    );

    let edited = found
        .iter()
        .find(|(path, patch, ..)| path == "src/app.rs" && patch.contains("+    run_twice();"))
        .expect("a before/after entry becomes a patch");
    assert!(
        edited.1.starts_with("--- a/src/app.rs") && edited.1.contains("+++ b/src/app.rs"),
        "the patch has a diff header: {}",
        edited.1
    );
    assert_eq!((edited.2, edited.3), (1, 0), "the server's counts are kept");

    let docs = found
        .iter()
        .find(|(path, ..)| path == "docs/README.md")
        .expect("an explicit patch is passed through");
    assert!(
        docs.1.starts_with("--- a/docs/README.md"),
        "the server's patch wins over a reconstruction: {}",
        docs.1
    );
    assert_eq!((docs.2, docs.3), (1, 2));

    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "a diff is content, not an error: {:?}",
        tags(&events)
    );
    let _ = session.shutdown().await;
}

#[tokio::test]
async fn a_dropped_event_stream_reconnects_and_finishes_the_turn() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("requests.jsonl");
    let mut config = config(
        "disconnect",
        dir.path(),
        &["--delay-ms", "5", "--log-file", &arg(&log)],
    );
    config["reconnectDelayMs"] = json!(20);
    config["readyTimeoutMs"] = json!(15_000);

    let mut session = start(config).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("survive a dropped stream"))
        .await
        .expect("send");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert!(
        errors(&events).is_empty(),
        "a reconnect is not an error: {kinds:?}"
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        }),
        "the turn finishes after the reconnect: {kinds:?}"
    );
    let streamed = deltas(&events);
    assert!(
        streamed.contains("Partial") && streamed.contains("and the rest."),
        "the pre- and post-reconnect deltas are both in the transcript: {streamed:?}"
    );
    assert!(
        assistant_text(&events).contains("and the rest."),
        "the post-reconnect message lands: {kinds:?}"
    );

    let connects = log_entries(&log, "sse-connect");
    assert!(
        connects.len() >= 2,
        "the reader opened a new /event connection: {connects:?}"
    );
    assert!(
        log_entries(&log, "sse-drop").len() == 1,
        "the server dropped the stream exactly once: {:?}",
        log_entries(&log, "sse-drop")
    );
    let _ = session.shutdown().await;
}

#[tokio::test]
async fn cancel_aborts_the_in_flight_turn_and_the_session_keeps_working() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("requests.jsonl");
    let mut config = config(
        "cancel",
        dir.path(),
        &["--delay-ms", "40", "--log-file", &arg(&log)],
    );
    config["readyTimeoutMs"] = json!(15_000);

    let mut session = start(config).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("a long slow answer"))
        .await
        .expect("send");

    let startup = drain_until(&mut rx, |event| {
        matches!(event.kind, EventKind::Delta { .. })
    })
    .await;
    assert!(
        startup
            .iter()
            .any(|event| matches!(event.kind, EventKind::Delta { .. })),
        "the slow mode streams before the cancel: {:?}",
        tags(&startup)
    );

    session.cancel().await.expect("cancel is delivered");
    let after = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        errors(&after).is_empty(),
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
        "the cancelled turn emits no assistant message: {:?}",
        tags(&after)
    );
    assert_eq!(
        log_entries(&log, "abort").len(),
        1,
        "the abort went over HTTP"
    );

    session
        .send(UserInput::text("after the cancel"))
        .await
        .expect("send after cancel");
    let next = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        next.iter()
            .any(|event| matches!(event.kind, EventKind::Delta { .. }))
            && assistant_text(&next).contains("Still here."),
        "the conversation keeps working after an abort: {:?}",
        tags(&next)
    );
    let _ = session.shutdown().await;
}

#[tokio::test]
async fn a_permanent_event_stream_failure_is_an_error_and_an_exited_status() {
    let dir = tempfile::tempdir().expect("tempdir");
    let heartbeat = dir.path().join("heartbeat.txt");
    let mut config = config(
        "permanent",
        dir.path(),
        &[
            "--delay-ms",
            "5",
            "--heartbeat-ms",
            "50",
            "--heartbeat-file",
            &arg(&heartbeat),
        ],
    );
    config["reconnectAttempts"] = json!(2);
    config["reconnectDelayMs"] = json!(20);
    config["readyTimeoutMs"] = json!(15_000);

    let mut session = start(config).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("the server dies mid-turn"))
        .await
        .expect("send");
    let events = drain_until_state(&mut rx, SessionState::Exited).await;
    let kinds = tags(&events);

    let failure = errors(&events)
        .into_iter()
        .find(|(message, _)| message.contains("event stream"))
        .unwrap_or_else(|| panic!("a dead event stream is an error: {kinds:?}"));
    assert!(
        !failure.1,
        "a stream that cannot be re-established is not retryable: {}",
        failure.0
    );
    assert!(
        failure.0.contains("failed permanently") || failure.0.contains("gave up"),
        "the message explains the permanent failure: {}",
        failure.0
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Exited
        }),
        "a permanent failure ends the session as exited: {kinds:?}"
    );

    // A spawned server that can never be used must not be left running.
    let _ = read_heartbeat(&heartbeat);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let stopped = read_heartbeat(&heartbeat);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        stopped,
        read_heartbeat(&heartbeat),
        "the unusable server was reaped: no orphan"
    );
    let _ = session.shutdown().await;
}

#[tokio::test]
async fn a_session_error_is_reported_and_the_turn_ends_idle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut config = config("error", dir.path(), &["--delay-ms", "5"]);
    config["readyTimeoutMs"] = json!(15_000);

    let mut session = start(config).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("fail")).await.expect("send");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let reported = errors(&events);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(!reported[0].1, "a provider error is not retryable as-is");
    assert!(
        reported[0].0.contains("ApiError"),
        "the harness's error name is preserved: {}",
        reported[0].0
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        }),
        "a session.error is followed by the server's idle: {:?}",
        tags(&events)
    );
    let _ = session.shutdown().await;
}

#[tokio::test]
async fn attach_mode_targets_a_running_server_and_never_kills_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let attached = Attached::start(dir.path(), "happy", "5");
    let manager = SessionManager::open(AppPaths::new(dir.path()))
        .expect("the store opens and the adapters wire");

    let harness = manager
        .harnesses()
        .into_iter()
        .find(|info| info.id == "opencode")
        .expect("opencode is registered for the picker");
    assert!(harness.capabilities.resume);
    assert!(harness.capabilities.diffs);
    assert!(
        !harness.capabilities.permissions,
        "the picker shows the honest capability set"
    );

    let detail = manager
        .create(CreateSessionRequest {
            harness: "opencode".into(),
            cwd: Some(std::env::temp_dir().to_string_lossy().to_string()),
            model: Some("anthropic/claude-sonnet-4".into()),
            title: None,
            config: Some(json!({
                "baseUrl": attached.base_url,
                "reconnectDelayMs": 20,
                "readyTimeoutMs": 15_000,
            })),
        })
        .await
        .expect("the session attaches");
    let id = detail.session.id;

    manager
        .send(&id, "hello from attach mode")
        .await
        .expect("send");
    let events = wait_for(&manager, &id, |events| {
        events
            .iter()
            .any(|event| is_state(event, SessionState::Idle))
    })
    .await;
    assert!(
        assistant_text(&events).contains("Hello there"),
        "the attached server answered: {:?}",
        tags(&events)
    );
    let token = manager
        .store()
        .resume_token(&id)
        .expect("the token column reads")
        .expect("the server session id is mirrored into sessions.resume_token");
    assert!(token.starts_with("ses_"), "{token}");

    // Restart survival: the attach URL is part of the session, and the conversation continues.
    manager.close(&id).await.expect("close");
    manager.resume(&id).await.expect("resume reattaches");
    let resumed = wait_for(&manager, &id, |events| {
        events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::SessionStarted { .. }))
            .count()
            == 2
    })
    .await;
    assert_eq!(
        resumed
            .iter()
            .filter(|event| matches!(event.kind, EventKind::SessionStarted { resumed: true, .. }))
            .count(),
        1,
        "the second start reports resumed: {:?}",
        tags(&resumed)
    );
    assert_eq!(
        manager.store().adapter_config(&id).expect("config reads")["baseUrl"],
        attached.base_url.as_str(),
        "the attach-mode URL is remembered for the session"
    );
    assert_eq!(
        manager.store().resume_token(&id).unwrap().as_deref(),
        Some(token.as_str()),
        "reattaching keeps the conversation id"
    );

    manager.close(&id).await.expect("close");
    let beats = attached.heartbeat();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_ne!(
        beats,
        attached.heartbeat(),
        "attach mode leaves the server it did not start running"
    );
    let health = reqwest::get(format!("{}/global/health", attached.base_url))
        .await
        .expect("the attached server still answers")
        .status();
    assert!(health.is_success(), "the attached server is untouched");
}

#[tokio::test]
async fn a_spawned_server_is_killed_on_shutdown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("requests.jsonl");
    let heartbeat = dir.path().join("heartbeat.txt");
    let mut config = config(
        "happy",
        dir.path(),
        &[
            "--delay-ms",
            "5",
            "--heartbeat-ms",
            "50",
            "--heartbeat-file",
            &arg(&heartbeat),
            "--log-file",
            &arg(&log),
        ],
    );
    config["readyTimeoutMs"] = json!(15_000);

    let mut session = start(config).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("something to run"))
        .await
        .expect("send");
    drain_until_state(&mut rx, SessionState::Idle).await;

    let alive = read_heartbeat(&heartbeat);
    assert!(!alive.is_empty(), "the fixture server was running");
    let port = log_entries(&log, "listening")[0]["port"]
        .as_u64()
        .expect("the chosen port") as u16;

    session.shutdown().await.expect("shutdown reaps the child");
    let stopped = read_heartbeat(&heartbeat);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        stopped,
        read_heartbeat(&heartbeat),
        "nothing writes after shutdown: the child is gone"
    );
    assert!(
        TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(300)
        )
        .is_err(),
        "the server's port is closed after shutdown"
    );
}

#[tokio::test]
async fn a_readiness_timeout_reports_the_stderr_tail_and_reaps_the_child() {
    let dir = tempfile::tempdir().expect("tempdir");
    let heartbeat = dir.path().join("heartbeat.txt");
    let mut config = config(
        "stalled",
        dir.path(),
        &["--heartbeat-ms", "50", "--heartbeat-file", &arg(&heartbeat)],
    );
    config["readyTimeoutMs"] = json!(400);
    config["healthIntervalMs"] = json!(25);

    let mut session = start(config).await;
    let mut rx = session.take_events();
    let events = drain_until_state(&mut rx, SessionState::Exited).await;
    let kinds = tags(&events);
    let failure = errors(&events)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("a server that never becomes ready is an error: {kinds:?}"));
    assert!(
        !failure.1,
        "a server that never comes up is not retryable: {}",
        failure.0
    );
    assert!(
        failure.0.contains("did not become healthy") && failure.0.contains("400"),
        "the message names the readiness timeout: {}",
        failure.0
    );
    assert!(
        failure.0.contains("vault is locked"),
        "the captured stderr is in the failure reason: {}",
        failure.0
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Exited
        }),
        "the failed start ends the session: {kinds:?}"
    );

    let stopped = read_heartbeat(&heartbeat);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        stopped,
        read_heartbeat(&heartbeat),
        "the child is reaped even when readiness fails: no orphan"
    );
    let _ = session.shutdown().await;
}

#[tokio::test]
async fn readiness_is_polled_until_the_server_answers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("requests.jsonl");
    let mut config = config(
        "happy",
        dir.path(),
        &[
            "--delay-ms",
            "5",
            "--ready-delay-ms",
            "1500",
            "--log-file",
            &arg(&log),
        ],
    );
    config["readyTimeoutMs"] = json!(15_000);
    config["healthIntervalMs"] = json!(25);

    let mut session = start(config).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("hello after a slow start"))
        .await
        .expect("send");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        deltas(&events).contains("Hello there"),
        "the turn runs once the server answers: {:?}",
        tags(&events)
    );
    assert!(
        !log_entries(&log, "health-not-ready").is_empty(),
        "the fixture refused readiness on purpose"
    );
    let probes = log_entries(&log, "request")
        .into_iter()
        .filter(|entry| entry["path"] == "/global/health")
        .count();
    assert!(
        probes > 1,
        "readiness is polled, not guessed: {probes} probes"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "a slow start is not an error: {:?}",
        tags(&events)
    );
    let _ = session.shutdown().await;
}

#[tokio::test]
async fn the_opencode_session_id_is_the_resume_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("requests.jsonl");
    let mut config = config(
        "happy",
        dir.path(),
        &["--delay-ms", "5", "--log-file", &arg(&log)],
    );
    config["readyTimeoutMs"] = json!(15_000);

    let mut first = start(config).await;
    let mut first_rx = first.take_events();
    first
        .send(UserInput::text("the first turn"))
        .await
        .expect("send");
    drain_until_state(&mut first_rx, SessionState::Idle).await;
    let token = first
        .resume_token()
        .expect("the server's session id is the token");
    assert!(token.starts_with("ses_"), "{token}");
    let _ = first.shutdown().await;

    // The server persisted the conversation: the same id is still there after a restart.
    let state = std::fs::read_to_string(dir.path().join("sessions.json"))
        .expect("the fixture persists its sessions");
    assert!(state.contains(&token), "{state}");

    let mut resumed_context = context(
        "happy",
        dir.path(),
        &["--delay-ms", "5", "--log-file", &arg(&log)],
    );
    resumed_context.resume_token = Some(token.clone());
    let mut second = OpenCodeAdapter::default()
        .start(resumed_context, deps())
        .await
        .expect("start");
    let mut second_rx = second.take_events();
    let started = next_event(&mut second_rx).await;
    match &started.kind {
        EventKind::SessionStarted { resumed, .. } => {
            assert!(*resumed, "a resumed session reports resumed: true")
        }
        other => panic!("the first event is session_started: {other:?}"),
    }
    assert_eq!(
        second.resume_token().as_deref(),
        Some(token.as_str()),
        "the token is available before the first turn"
    );

    second
        .send(UserInput::text("the second turn"))
        .await
        .expect("send");
    let events = drain_until_state(&mut second_rx, SessionState::Idle).await;
    assert!(
        assistant_text(&events).contains("Hello there"),
        "the resumed conversation answers: {:?}",
        tags(&events)
    );
    assert_eq!(
        second.resume_token().as_deref(),
        Some(token.as_str()),
        "the harness kept the same conversation id"
    );
    assert_eq!(
        log_entries(&log, "session-created").len(),
        1,
        "resume reuses the server's session instead of minting another: {:?}",
        log_entries(&log, "session-created")
    );
    let _ = second.shutdown().await;
}

#[tokio::test]
async fn unknown_frames_and_other_sessions_are_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut config = config("unknown", dir.path(), &["--delay-ms", "5"]);
    config["readyTimeoutMs"] = json!(15_000);

    let mut session = start(config).await;
    let mut rx = session.take_events();
    let junk = "line one\n{\"type\":\"from-the-future\"}\n\u{2028}odd\n";
    session.send(UserInput::text(junk)).await.expect("send");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert!(
        errors(&events).is_empty(),
        "unknown and malformed frames are skipped, never fatal: {kinds:?}"
    );
    let text = assistant_text(&events);
    assert!(text.contains("Hello"), "{kinds:?}");
    assert!(
        !text.contains("ECHO:"),
        "a user part is not an assistant message: {text}"
    );
    assert!(
        !text.contains("other session text") && !deltas(&events).contains("other session text"),
        "another session's frames are ignored: {text}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolCall { .. })),
        "no events are invented from unknown parts: {kinds:?}"
    );

    session
        .send(UserInput::text("still alive"))
        .await
        .expect("send after junk");
    let more = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        more.iter().any(|event| matches!(
            event.kind,
            EventKind::Message { .. } | EventKind::Delta { .. }
        )),
        "the session still works after malformed input: {:?}",
        tags(&more)
    );
    let _ = session.shutdown().await;
}

#[tokio::test]
async fn a_non_loopback_url_is_refused_before_any_request() {
    let mut session = start(json!({ "baseUrl": "http://example.com:4096" })).await;
    let mut rx = session.take_events();
    let events = drain_until_state(&mut rx, SessionState::Exited).await;
    let kinds = tags(&events);
    let failure = errors(&events)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("a non-loopback URL is refused: {kinds:?}"));
    assert!(!failure.1, "a config error is not retryable: {}", failure.0);
    assert!(
        failure.0.contains("loopback"),
        "the message explains the loopback rule: {}",
        failure.0
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Exited
        }),
        "nothing was started: {kinds:?}"
    );
    let _ = session.shutdown().await;
}

#[test]
fn detection_reports_presence_and_any_configured_url() {
    let installed = OpenCodeAdapter::with_command("node").detect(&deps());
    assert!(installed.installed, "node resolves: {installed:?}");
    assert!(
        installed
            .path
            .as_deref()
            .is_some_and(|path| path.to_ascii_lowercase().contains("node")),
        "{installed:?}"
    );
    assert!(
        installed.version.is_none(),
        "detection never runs the binary"
    );

    let attached = OpenCodeAdapter::with_base_url("http://127.0.0.1:4096").detect(&deps());
    assert!(
        attached.installed,
        "attach mode needs no local binary: {attached:?}"
    );
    assert!(
        attached
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("http://127.0.0.1:4096")),
        "the doctor reports the configured URL: {attached:?}"
    );

    let missing =
        OpenCodeAdapter::with_command("grove-definitely-not-a-real-binary").detect(&deps());
    assert!(!missing.installed);
    assert!(missing.version.is_none());
    assert!(
        missing
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("grove-definitely-not-a-real-binary")),
        "the reason names the missing binary: {missing:?}"
    );
}

#[tokio::test]
async fn the_doctor_lists_opencode_with_honest_capabilities() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path());
    paths.ensure().expect("app paths");
    let manager = Arc::new(SessionManager::in_memory(&paths).expect("in-memory runtime"));
    let settings = Arc::new(SettingsService::new(manager.store()));
    let doctor = HarnessDoctor::new(settings, Arc::clone(&manager));

    let report = doctor.report().await;
    let row = report
        .harnesses
        .iter()
        .find(|harness| harness.id == "opencode")
        .expect("the doctor lists opencode");
    assert_eq!(row.label, "OpenCode");
    if !row.installed {
        assert!(
            row.detail
                .as_deref()
                .is_some_and(|detail| !detail.is_empty()),
            "a missing install is explained: {row:?}"
        );
    }

    let adapter = builtin_registry()
        .into_iter()
        .find(|adapter| adapter.id().as_str() == "opencode")
        .expect("opencode is registered");
    let capabilities = adapter.capabilities();
    assert!(capabilities.streaming_deltas);
    assert!(capabilities.tool_calls);
    assert!(capabilities.diffs, "edit tools carry a patch");
    assert!(capabilities.resume);
    assert!(capabilities.usage);
    assert!(
        !capabilities.permissions,
        "permission prompts have no answer channel yet"
    );
    assert!(!capabilities.images);
    assert!(!capabilities.mcp);
}
