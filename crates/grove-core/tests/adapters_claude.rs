//! The Claude Code adapter against `fixtures/harnesses/claude_fake.mjs`: no real CLI, no network, one
//! test per fixture mode. The shared conformance suite is in `tests/adapters_conformance.rs`.

pub mod adapter_support;

use adapter_support::claude_fixture::{config, context};
use adapter_support::runtime::{deps, drain_until, drain_until_state, is_state, tags};
use grove_core::adapters::{
    AdapterError, ClaudeAdapter, HarnessAdapter, HarnessSession, UserInput,
};
use grove_core::ipc::CreateSessionRequest;
use grove_core::paths::AppPaths;
use grove_core::sessions::event::{
    Block, Decision, EventKind, HarnessEvent, Role, SessionState, ToolStatus,
};
use grove_core::sessions::manager::SessionManager;
use serde_json::json;
use std::time::Duration;

async fn start(mode: &str, extra: &[&str]) -> Box<dyn HarnessSession> {
    ClaudeAdapter::default()
        .start(context(mode, extra), deps())
        .await
        .expect("the adapter starts: detection never blocks a session")
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
async fn happy_mode_streams_text_a_message_and_usage() {
    let mut session = start("happy", &[]).await;
    let mut rx = session.take_events();
    assert!(
        session.resume_token().is_none(),
        "a fresh session has no conversation id until the harness speaks"
    );

    session
        .send(UserInput::text("hello claude"))
        .await
        .expect("send reaches the fixture");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert_eq!(kinds.first(), Some(&"session_started"));
    assert_eq!(
        events[0].kind,
        EventKind::SessionStarted {
            harness: "claude".into(),
            model: Some("claude-fake-1".into()),
            resumed: false,
        }
    );
    assert!(kinds.contains(&"delta"), "{kinds:?}");
    assert!(kinds.contains(&"message"), "{kinds:?}");
    assert!(kinds.contains(&"usage"), "{kinds:?}");
    assert_eq!(kinds.last(), Some(&"status"));
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        })
    );

    let text = message_text(&events);
    assert!(
        text.contains("Fake reply: hello claude"),
        "the prompt reached the harness on stdin: {text}"
    );
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Message { blocks, .. } if blocks.iter().any(|block| matches!(block, Block::Thinking { .. }))
        )),
        "thinking blocks survive the mapping"
    );
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        EventKind::Usage {
            input_tokens: 12,
            output_tokens: 7
        }
    )));

    let token = session.resume_token().expect("the session id is captured");
    assert!(
        token.starts_with("fake-session-"),
        "the resume token is the harness's session id: {token}"
    );
}

#[tokio::test]
async fn tool_use_becomes_a_tool_call_and_a_tool_result() {
    let mut session = start("tools", &[]).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("read the readme"))
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
    assert_eq!(calls[0].1, "Read");
    assert_eq!(
        calls[0].2, "C:/work/grove/README.md",
        "title uses file_path"
    );
    assert_eq!(calls[1].1, "Bash");
    assert_eq!(calls[1].2, "cargo test -p grove-core", "title uses command");

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
    assert!(results[0].2.contains("# Grove"), "{}", results[0].2);
    assert_eq!(results[1].0, calls[1].0);
    assert_eq!(results[1].1, ToolStatus::Error);
    assert!(results[1].2.contains("one test failed"), "{}", results[1].2);

    // The call is recorded before its result, like any transcript.
    let call_at = events
        .iter()
        .position(|event| matches!(&event.kind, EventKind::ToolCall { call_id, .. } if *call_id == calls[0].0))
        .unwrap();
    let result_at = events
        .iter()
        .position(|event| matches!(&event.kind, EventKind::ToolResult { call_id, .. } if *call_id == calls[0].0))
        .unwrap();
    assert!(call_at < result_at);
}

#[tokio::test]
async fn permissions_are_honestly_unavailable() {
    let adapter = ClaudeAdapter::default();
    assert!(
        !adapter.capabilities().permissions,
        "a one-shot `claude -p` run has no interactive permission channel"
    );

    let mut session = start("permission", &[]).await;
    let error = session
        .respond_permission("perm_1", Decision::Approve)
        .await
        .expect_err("there is nobody to answer");
    assert!(
        matches!(error, AdapterError::Unsupported(_)),
        "an unsupported operation is reported as Unsupported, got {error:?}"
    );

    let mut rx = session.take_events();
    session
        .send(UserInput::text("run a command"))
        .await
        .unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);
    assert!(
        !kinds.contains(&"permission_request"),
        "the adapter never claims a permission flow it cannot answer: {kinds:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event.kind,
            EventKind::Error {
                retryable: false,
                ..
            }
        )),
        "the denial is reported, not swallowed: {kinds:?}"
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        }),
        "the turn still terminates cleanly"
    );
}

#[tokio::test]
async fn a_failed_result_is_a_non_retryable_error_and_error_status() {
    let mut session = start("error", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("go")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Error).await;

    let error = events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::Error { message, retryable } => Some((message.clone(), *retryable)),
            _ => None,
        })
        .expect("the failing result becomes an error event");
    assert!(
        !error.1,
        "a harness-reported failure is not retryable as-is"
    );
    assert!(error.0.contains("error_max_turns"), "{}", error.0);
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Usage {
                input_tokens: 9,
                output_tokens: 2
            }
        )),
        "usage is reported even for a failed turn: {:?}",
        tags(&events)
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Error
        })
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::Error { .. }))
            .count(),
        1,
        "the exit after a reported failure is not a second error"
    );
}

#[tokio::test]
async fn a_mid_stream_crash_is_a_retryable_error_and_exited_status() {
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
        .expect("a crash before `result` is an error event");
    assert!(
        error.1,
        "an unexpected exit is retryable (a resume can continue): {}",
        error.0
    );
    assert!(
        error.0.contains("before sending a result"),
        "the message explains the failure: {}",
        error.0
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Exited
        }),
        "the session is exited, not idle: {kinds:?}"
    );
}

#[tokio::test]
async fn unknown_and_malformed_lines_are_skipped() {
    let mut session = start("unknown", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("hello")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "unknown and malformed lines are logged and skipped, never fatal: {kinds:?}"
    );
    let deltas: String = events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Delta { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        deltas.contains("Hello"),
        "token deltas are mapped when the CLI sends them: {deltas:?}"
    );
    assert!(message_text(&events).contains("Hello"), "{kinds:?}");
    assert!(
        !kinds.contains(&"tool_call"),
        "the unknown frames produce no invented events: {kinds:?}"
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        })
    );
}

#[tokio::test]
async fn cancel_kills_the_turn_and_returns_to_idle() {
    let mut session = start("slow", &["--delay-ms", "50"]).await;
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
        "a cancel is not a crash: {:?}",
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

    // The child process tree is gone: nothing more arrives from the fixture.
    let quiet = tokio::time::timeout(Duration::from_millis(400), rx.recv()).await;
    assert!(
        quiet.is_err(),
        "the killed harness emitted no further events: {quiet:?}"
    );

    // And the session is still usable afterwards.
    session
        .send(UserInput::text("after the cancel"))
        .await
        .unwrap();
    let resumed = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        resumed.iter().any(|event| matches!(
            event.kind,
            EventKind::Message { .. } | EventKind::Delta { .. }
        )),
        "the session keeps working after a cancel: {:?}",
        tags(&resumed)
    );
}

#[tokio::test]
async fn the_config_command_and_args_override_any_binary() {
    // The adapter's own command is deliberately useless: config.command/config.args must win, which
    // is what lets users point Grove at a wrapper and tests at a fixture.
    let adapter = ClaudeAdapter::with_command("grove-definitely-not-a-real-binary");
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
async fn a_missing_binary_is_an_error_event_not_a_start_failure() {
    let adapter = ClaudeAdapter::with_command("grove-definitely-not-a-real-binary");
    let mut context = context("happy", &[]);
    context.config = json!({ "command": "grove-definitely-not-a-real-binary" });

    let mut session = adapter
        .start(context, deps())
        .await
        .expect("a session starts even when the binary is missing");
    let mut rx = session.take_events();
    session.send(UserInput::text("go")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Exited).await;

    let error = events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::Error { message, retryable } => Some((message.clone(), *retryable)),
            _ => None,
        })
        .expect("a launch failure is reported on the session");
    assert!(!error.1, "configuring the wrong command is not retryable");
    assert!(
        error.0.contains("could not launch"),
        "the message is actionable: {}",
        error.0
    );
}

#[test]
fn detection_reports_missing_installs_without_spawning() {
    let missing = ClaudeAdapter::with_command("grove-definitely-not-a-real-binary").detect(&deps());
    assert!(!missing.installed);
    assert!(missing.version.is_none(), "detection never runs the binary");
    assert!(
        missing
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("grove-definitely-not-a-real-binary")),
        "the reason names the missing binary: {missing:?}"
    );

    // The fixture runtime itself resolves, which is what the tests rely on.
    let node = ClaudeAdapter::with_command("node").detect(&deps());
    assert!(node.installed, "node resolves on PATH: {node:?}");
    assert!(node.version.is_none());
}

/// A shut-down session reports a closed channel instead of panicking on the next command.
#[tokio::test]
async fn a_closed_session_rejects_commands_instead_of_panicking() {
    let mut session = start("happy", &[]).await;
    session.shutdown().await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match session.send(UserInput::text("too late")).await {
            Err(_) => return,
            Ok(()) => assert!(
                tokio::time::Instant::now() < deadline,
                "a shut-down session eventually rejects commands"
            ),
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_for(
    manager: &SessionManager,
    id: &str,
    done: impl Fn(&[HarnessEvent]) -> bool,
) -> Vec<HarnessEvent> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let events = manager.get(id).expect("the session exists").events;
        if done(&events) {
            return events;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the transcript never reached the expected state: {:?}",
            tags(&events)
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The whole seam, through the real runtime: registry → picker → SQLite row → adapter → transcript →
/// resume from the mirrored token. No UI code takes part.
#[tokio::test]
async fn a_claude_session_runs_end_to_end_through_the_session_manager() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::open(AppPaths::new(dir.path())).expect("the store opens");

    // The picker is `harness_list`; it reuses HarnessInfo, so the UI needs no Claude-specific shape.
    let harnesses = manager.harnesses();
    let claude = harnesses
        .iter()
        .find(|info| info.id == "claude")
        .expect("claude is registered for the picker");
    assert!(claude.capabilities.resume);
    assert!(
        !claude.capabilities.permissions,
        "the picker shows the honest capability set"
    );

    let detail = manager
        .create(CreateSessionRequest {
            harness: "claude".into(),
            cwd: Some(std::env::temp_dir().to_string_lossy().to_string()),
            model: Some("claude-fake-1".into()),
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
        .expect("the session id is mirrored into sessions.resume_token");
    assert!(token.starts_with("fake-session-"), "{token}");

    // Restart survival: close the process, resume the conversation from the row, and the harness is
    // asked to continue the same conversation.
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
        "the conversation id survives the restart"
    );
}
