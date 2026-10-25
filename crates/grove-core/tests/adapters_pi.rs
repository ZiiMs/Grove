//! The `pi` adapter against `fixtures/harnesses/pi_sidecar_fake.mjs`: no Node sidecar build, no real
//! `pi`, no network, one test per fixture mode. The shared conformance suite is in
//! `tests/adapters_conformance.rs`.
//!
//! The fixture speaks Grove Sidecar Protocol v1 (`docs/sidecar-protocol.md`); `pi` itself is behind
//! that boundary, and the sidecar's own mapping is covered by `sidecar/test/pi_host.test.ts`.

pub mod adapter_support;

use adapter_support::pi_fixture::{config, context};
use adapter_support::runtime::{
    deps, drain_until, drain_until_state, fixture, is_state, next_event, tags,
};
use grove_core::adapters::{AdapterError, HarnessAdapter, HarnessSession, PiAdapter, UserInput};
use grove_core::harness_doctor::{DoctorConfig, HarnessDoctor};
use grove_core::ipc::CreateSessionRequest;
use grove_core::paths::AppPaths;
use grove_core::sessions::event::{
    Block, Decision, EventKind, HarnessEvent, Role, SessionState, ToolStatus,
};
use grove_core::sessions::manager::SessionManager;
use grove_core::settings::SettingsService;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

async fn start(mode: &str, extra: &[&str]) -> Box<dyn HarnessSession> {
    PiAdapter::default()
        .start(context(mode, extra), deps())
        .await
        .expect("the adapter starts: the fixture sidecar is available")
}

fn message_text(events: &[HarnessEvent]) -> String {
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

#[tokio::test]
async fn a_happy_turn_streams_deltas_a_message_usage_and_the_resume_token() {
    let mut session = start("happy", &[]).await;
    let mut rx = session.take_events();
    let token = session
        .resume_token()
        .expect("the token arrives with the sidecar's session.start reply");
    assert!(
        token.ends_with(".jsonl") && token.contains("session-"),
        "pi's resumable handle is a session file path: {token}"
    );

    session
        .send(UserInput::text("hello pi"))
        .await
        .expect("send reaches the sidecar");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert_eq!(kinds.first(), Some(&"session_started"));
    assert_eq!(
        events[0].kind,
        EventKind::SessionStarted {
            harness: "pi".into(),
            model: Some("pi-fake-1".into()),
            resumed: false,
        }
    );
    assert!(kinds.contains(&"delta"), "{kinds:?}");
    assert!(kinds.contains(&"message"), "{kinds:?}");
    assert!(kinds.contains(&"usage"), "{kinds:?}");
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        })
    );
    assert!(
        !kinds.contains(&"exited"),
        "a completed turn is idle, not exited: {kinds:?}"
    );

    let text = message_text(&events);
    assert!(
        text.contains("Fake reply: hello pi"),
        "the prompt reached the sidecar: {text}"
    );
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Message { blocks, .. }
                if blocks.iter().any(|block| matches!(block, Block::Thinking { .. }))
        )),
        "thinking blocks survive the boundary: {kinds:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Usage {
                input_tokens: 42,
                output_tokens: 11
            }
        )),
        "usage is forwarded: {kinds:?}"
    );

    assert_eq!(
        session.resume_token().as_deref(),
        Some(token.as_str()),
        "a turn does not change the session file"
    );
}

#[tokio::test]
async fn tool_calls_and_results_cross_the_boundary_in_order() {
    let mut session = start("tools", &[]).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("inspect the repo"))
        .await
        .unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;

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
    assert_eq!(calls[0].1, "bash");
    assert_eq!(calls[0].2, "ls -la", "the title is the command");
    assert_eq!(
        calls[1].2, "C:/work/grove/README.md",
        "the title is the path"
    );

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
    assert_eq!(results.len(), 2, "two results: {results:?}");
    assert_eq!(results[0].0, calls[0].0);
    assert_eq!(results[0].1, ToolStatus::Ok);
    assert!(results[0].2.contains("README.md"), "{}", results[0].2);
    assert_eq!(results[1].0, calls[1].0);
    assert_eq!(results[1].1, ToolStatus::Error);

    let call_at = events
        .iter()
        .position(|event| matches!(&event.kind, EventKind::ToolCall { call_id, .. } if *call_id == calls[0].0))
        .unwrap();
    let result_at = events
        .iter()
        .position(|event| matches!(&event.kind, EventKind::ToolResult { call_id, .. } if *call_id == calls[0].0))
        .unwrap();
    assert!(call_at < result_at, "the call precedes its result");
}

#[tokio::test]
async fn cancel_returns_to_idle_and_the_session_stays_usable() {
    let mut session = start("slow", &["--delay-ms", "20"]).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("a long slow answer"))
        .await
        .unwrap();

    let startup = drain_until(&mut rx, |event| {
        matches!(event.kind, EventKind::Delta { .. })
    })
    .await;
    assert!(!startup.is_empty());
    session.cancel().await.expect("cancel is delivered");

    let after = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        !after
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "a cancel is not an error: {:?}",
        tags(&after)
    );
    assert!(
        !startup
            .iter()
            .chain(after.iter())
            .any(|event| match &event.kind {
                EventKind::Message {
                    role: Role::Assistant,
                    blocks,
                } => blocks
                    .iter()
                    .any(|block| matches!(block, Block::Text { .. })),
                _ => false,
            }),
        "a cancelled turn emits no assistant text message"
    );

    session
        .send(UserInput::text("after the cancel"))
        .await
        .expect("the session still accepts a prompt");
    let resumed = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        message_text(&resumed).contains("Fake reply: after the cancel"),
        "the session keeps working after a cancel: {:?}",
        tags(&resumed)
    );
}

#[tokio::test]
async fn malformed_sidecar_output_is_logged_and_dropped() {
    let mut session = start("unknown", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("still works")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "junk lines and unknown frames are dropped, never fatal: {kinds:?}"
    );
    assert!(kinds.contains(&"delta"), "{kinds:?}");
    assert!(
        message_text(&events).contains("Fake reply: still works"),
        "the valid frames still reach the transcript: {kinds:?}"
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        })
    );
}

#[tokio::test]
async fn a_dead_sidecar_is_a_retryable_error_and_exited_status() {
    let mut session = start("crash", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("go")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Exited).await;
    let kinds = tags(&events);

    let error = events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::Error { message, retryable } => Some((message.clone(), *retryable)),
            _ => None,
        })
        .expect("a dead sidecar is an error event");
    assert!(
        error.1,
        "an unexpected sidecar exit is retryable (a resume can continue): {}",
        error.0
    );
    assert!(
        error.0.contains("pi sidecar"),
        "the message names the dead process: {}",
        error.0
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Exited
        }),
        "the session is exited, not idle: {kinds:?}"
    );

    // The actor is gone, so a command that can no longer be delivered is an error, not a panic.
    let _ = session.send(UserInput::text("too late")).await;
}

#[tokio::test]
async fn an_unavailable_pi_binary_is_reported_as_unavailable() {
    let error = PiAdapter::default()
        .start(context("unavailable", &[]), deps())
        .await
        .err()
        .expect("the sidecar reports pi as unavailable");
    match error {
        AdapterError::Unavailable(reason) => assert!(
            reason.contains("pi is not installed"),
            "the reason comes from the sidecar: {reason}"
        ),
        other => panic!("a missing pi is unavailable, got {other:?}"),
    }
}

#[tokio::test]
async fn a_broken_handshake_is_reported_as_unavailable() {
    let error = PiAdapter::default()
        .start(context("handshake", &[]), deps())
        .await
        .err()
        .expect("a sidecar that dies during the handshake cannot start a session");
    match error {
        AdapterError::Unavailable(reason) => assert!(
            reason.contains("handshake") || reason.contains("exited"),
            "the reason explains the failed handshake: {reason}"
        ),
        other => panic!("a failed handshake is unavailable, got {other:?}"),
    }
}

#[tokio::test]
async fn a_missing_sidecar_binary_is_reported_as_unavailable() {
    let mut ctx = context("happy", &[]);
    // An explicit entry point keeps the test about the launcher, not about whether `npm run build`
    // has already written sidecar/dist/main.js in this checkout.
    ctx.config = json!({
        "command": "grove-definitely-not-a-real-binary",
        "args": [fixture("pi_sidecar_fake.mjs").to_string_lossy()],
    });
    let error = PiAdapter::default()
        .start(ctx, deps())
        .await
        .err()
        .expect("the launcher cannot be spawned");
    match error {
        AdapterError::Unavailable(reason) => assert!(
            reason.contains("grove-definitely-not-a-real-binary"),
            "the reason names the launcher: {reason}"
        ),
        other => panic!("a missing launcher is unavailable, got {other:?}"),
    }
}

#[tokio::test]
async fn the_resume_token_round_trips_through_session_start() {
    let mut first = start("happy", &[]).await;
    let mut first_rx = first.take_events();
    first.send(UserInput::text("first turn")).await.unwrap();
    drain_until_state(&mut first_rx, SessionState::Idle).await;
    let token = first
        .resume_token()
        .expect("the sidecar reported the session file path");
    let _ = first.shutdown().await;

    let mut ctx = context("happy", &[]);
    ctx.resume_token = Some(token.clone());
    let mut second = PiAdapter::default()
        .start(ctx, deps())
        .await
        .expect("a resumed session starts");
    assert_eq!(
        second.resume_token().as_deref(),
        Some(token.as_str()),
        "the token is available before the first turn of the resumed session"
    );

    let mut second_rx = second.take_events();
    let started = next_event(&mut second_rx).await;
    match &started.kind {
        EventKind::SessionStarted { resumed, .. } => {
            assert!(*resumed, "a resumed session reports resumed: {started:?}")
        }
        other => panic!("the first event is session_started: {other:?}"),
    }

    second.send(UserInput::text("resumed turn")).await.unwrap();
    drain_until_state(&mut second_rx, SessionState::Idle).await;
    assert_eq!(
        second.resume_token().as_deref(),
        Some(token.as_str()),
        "the harness keeps the same session file after resuming"
    );
}

#[tokio::test]
async fn shutdown_kills_the_sidecar_child() {
    let dir = tempfile::tempdir().expect("tempdir");
    let heartbeat = dir.path().join("beat.txt");
    let mut session = start(
        "happy",
        &["--heartbeat", heartbeat.to_string_lossy().as_ref()],
    )
    .await;
    let mut rx = session.take_events();
    session.send(UserInput::text("hello")).await.unwrap();
    drain_until_state(&mut rx, SessionState::Idle).await;

    wait_for_file(&heartbeat).await;
    session.shutdown().await.expect("shutdown is delivered");
    assert!(
        stops_heartbeating(&heartbeat).await,
        "a shut-down session leaves no sidecar process behind: {}",
        heartbeat.display()
    );
}

#[tokio::test]
async fn the_config_command_and_args_override_the_default_launch() {
    // The adapter's own launcher is deliberately useless: config.command/config.args must win, which
    // is what lets a user point Grove at a wrapper and a test at a fixture.
    let adapter = PiAdapter::with_command("grove-definitely-not-a-real-binary");
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
        message_text(&events).contains("Fake reply: via the fixture"),
        "{:?}",
        tags(&events)
    );
}

#[tokio::test]
async fn permissions_are_honestly_unavailable() {
    let adapter = PiAdapter::default();
    assert!(
        !adapter.capabilities().permissions,
        "pi has no tool-approval gate in its wire protocol"
    );
    let mut session = start("happy", &[]).await;
    let error = session
        .respond_permission("perm-1", Decision::Approve)
        .await
        .expect_err("there is nobody to answer");
    assert!(
        matches!(error, AdapterError::Unsupported(_)),
        "an unsupported operation is reported as Unsupported, got {error:?}"
    );
    let _ = session.shutdown().await;
}

#[test]
fn detection_reports_node_and_the_sidecar_build_without_spawning() {
    let missing = PiAdapter::with_command("grove-definitely-not-a-real-binary").detect(&deps());
    assert!(!missing.installed);
    assert!(missing.version.is_none(), "detection never runs a binary");
    assert!(
        missing
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("grove-definitely-not-a-real-binary")),
        "the reason names the missing launcher: {missing:?}"
    );

    // With Node present the answer is exactly "is the sidecar build there" — which is what the doctor
    // shows. The test never assumes the build ran (`npm run build` writes sidecar/dist/main.js).
    let node = PiAdapter::with_command("node").detect(&deps());
    assert!(node.version.is_none(), "detection never runs a binary");
    match PiAdapter::default_sidecar_entry() {
        Some(entry) => {
            assert!(
                entry.ends_with(Path::new("sidecar").join("dist").join("main.js")),
                "the sidecar entry is the workspace build: {}",
                entry.display()
            );
            assert!(
                node.installed,
                "node + a sidecar build is installed: {node:?}"
            );
            assert!(
                node.detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("sidecar")),
                "the detail says what the adapter launches: {node:?}"
            );
        }
        None => {
            assert!(!node.installed, "{node:?}");
            assert!(
                node.detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("npm run build")),
                "the reason says how to build the sidecar: {node:?}"
            );
        }
    }
}

/// The doctor is where a user finds out *why* a harness will not run: the pi row must say which
/// sidecar the adapter launches (or that the build is missing), next to the Node tool row.
#[tokio::test]
async fn the_doctor_reports_the_sidecar_the_pi_adapter_launches() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manager =
        Arc::new(SessionManager::open(AppPaths::new(dir.path())).expect("the store opens"));
    let settings = Arc::new(SettingsService::new(manager.store()));
    // An empty PATH keeps the report deterministic: the pi row then comes from the adapter's own
    // detection instead of a `pi --version` probe on the machine running the tests.
    let doctor = HarnessDoctor::with_config(
        settings,
        Arc::clone(&manager),
        DoctorConfig {
            path_env: Some(String::new()),
            path_extensions: Some(vec![".cmd".into()]),
            timeout: Duration::from_secs(2),
        },
    );

    let report = doctor.report().await;
    let pi = report
        .harnesses
        .iter()
        .find(|harness| harness.id == "pi")
        .expect("pi is in the doctor table");
    assert!(
        report.tools.iter().any(|tool| tool.id == "node"),
        "the doctor reports Node presence for the sidecar"
    );
    match PiAdapter::default_sidecar_entry() {
        Some(_) => {
            assert!(pi.installed, "{pi:?}");
            assert!(
                pi.detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("sidecar")),
                "the doctor names the sidecar the adapter launches: {pi:?}"
            );
        }
        None => {
            assert!(!pi.installed, "{pi:?}");
            assert!(
                pi.detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("npm run build")),
                "the doctor explains the missing sidecar build: {pi:?}"
            );
        }
    }
}

async fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the fixture never wrote {}", path.display());
}

/// True once the heartbeat file stops growing: the process that appends to it is gone.
async fn stops_heartbeating(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        if size == last {
            return true;
        }
        last = size;
    }
    false
}

async fn wait_for(
    manager: &SessionManager,
    id: &str,
    done: impl Fn(&[HarnessEvent]) -> bool,
) -> Vec<HarnessEvent> {
    let deadline = Instant::now() + Duration::from_secs(20);
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

/// The whole seam, through the real runtime: registry → picker → SQLite row → adapter → transcript →
/// resume from the mirrored token. No UI code takes part.
#[tokio::test]
async fn a_pi_session_runs_end_to_end_through_the_session_manager() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::open(AppPaths::new(dir.path())).expect("the store opens");

    let harnesses = manager.harnesses();
    let pi = harnesses
        .iter()
        .find(|info| info.id == "pi")
        .expect("pi is registered for the picker");
    assert!(pi.capabilities.streaming_deltas);
    assert!(pi.capabilities.tool_calls);
    assert!(pi.capabilities.resume);
    assert!(
        !pi.capabilities.permissions,
        "the picker shows the honest capability set"
    );

    let detail = manager
        .create(CreateSessionRequest {
            harness: "pi".into(),
            cwd: Some(std::env::temp_dir().to_string_lossy().to_string()),
            model: Some("pi-fake-1".into()),
            title: None,
            config: Some(config("happy", &[])),
        })
        .await
        .expect("the session is created");
    let id = detail.session.id;
    assert_eq!(detail.session.status, SessionState::Starting);

    manager
        .send(&id, "hello from the manager")
        .await
        .expect("send");
    let events = wait_for(&manager, &id, |events| {
        events
            .iter()
            .any(|event| is_state(event, SessionState::Idle))
    })
    .await;
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Message { role: Role::Assistant, blocks }
                if blocks.iter().any(|block| block.as_text().contains("hello from the manager"))
        )),
        "the reply is in the durable transcript: {:?}",
        tags(&events)
    );
    let token = manager
        .store()
        .resume_token(&id)
        .expect("the token column reads")
        .expect("the session file path is mirrored into sessions.resume_token");
    assert!(token.ends_with(".jsonl"), "{token}");

    // Restart survival: close the process, resume the conversation from the row, and the sidecar is
    // asked to reopen the same session file.
    manager.close(&id).await.expect("close");
    manager
        .resume(&id)
        .await
        .expect("resume restarts the adapter");
    let resumed = wait_for(&manager, &id, |events| {
        events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::SessionStarted { .. }))
            .count()
            == 2
    })
    .await;
    let started: Vec<_> = resumed
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::SessionStarted { resumed, .. } => Some(*resumed),
            _ => None,
        })
        .collect();
    assert_eq!(started, vec![false, true], "the restart reports resumed");
    assert_eq!(
        manager.store().resume_token(&id).unwrap().as_deref(),
        Some(token.as_str()),
        "the conversation survives the restart"
    );
}
