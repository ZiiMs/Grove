//! The Codex adapter against `fixtures/harnesses/codex_fake.mjs`: no real CLI, no network, one test
//! per fixture mode. The shared conformance suite is in `tests/adapters_conformance.rs`.

mod adapter_support;

use adapter_support::codex_fixture::{config, context};
use adapter_support::runtime::{deps, drain_until, drain_until_state, is_state, next_event, tags};
use grove_core::adapters::{CodexAdapter, HarnessAdapter, HarnessSession, UserInput};
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
use std::time::Duration;

async fn start(mode: &str, extra: &[&str]) -> Box<dyn HarnessSession> {
    CodexAdapter::default()
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

fn tool_calls(events: &[HarnessEvent]) -> Vec<(String, String, String)> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::ToolCall {
                call_id,
                name,
                title,
            } => Some((call_id.clone(), name.clone(), title.clone())),
            _ => None,
        })
        .collect()
}

fn tool_results(events: &[HarnessEvent]) -> Vec<(String, ToolStatus, String)> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::ToolResult {
                call_id,
                status,
                summary,
            } => Some((call_id.clone(), *status, summary.clone())),
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

#[tokio::test]
async fn happy_mode_streams_messages_command_executions_a_diff_and_usage() {
    let mut session = start("happy", &[]).await;
    let mut rx = session.take_events();
    assert!(
        session.resume_token().is_none(),
        "a fresh session has no thread id until the handshake answers"
    );

    session
        .send(UserInput::text("hello codex"))
        .await
        .expect("send reaches the app-server");
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert_eq!(kinds.first(), Some(&"session_started"));
    assert_eq!(
        events[0].kind,
        EventKind::SessionStarted {
            harness: "codex".into(),
            model: Some("codex-fake-1".into()),
            resumed: false,
        }
    );
    assert!(kinds.contains(&"delta"), "{kinds:?}");
    assert!(kinds.contains(&"message"), "{kinds:?}");
    assert!(kinds.contains(&"tool_call"), "{kinds:?}");
    assert!(kinds.contains(&"tool_result"), "{kinds:?}");
    assert!(kinds.contains(&"diff"), "{kinds:?}");
    assert!(kinds.contains(&"usage"), "{kinds:?}");
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        }),
        "turn/completed ends the turn idle: {kinds:?}"
    );

    // The fixture refuses a turn whose input is not `[{type:"text", text, text_elements: []}]`, and
    // echoes the prompt back: the reply proves the documented shape reached the harness.
    let text = message_text(&events);
    assert!(
        text.contains("Fake reply: hello codex"),
        "the prompt reached the harness in the documented input shape: {text}"
    );

    let calls = tool_calls(&events);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].1, "exec", "the command execution is named exec");
    assert_eq!(
        calls[0].2, "cargo test -p grove-core",
        "the title is the command that runs"
    );

    let results = tool_results(&events);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].0, calls[0].0, "the result answers the call");
    assert_eq!(results[0].1, ToolStatus::Ok, "exit code 0 is ok");
    assert!(
        results[0].2.contains("all 24 passed"),
        "the aggregated output is the summary: {}",
        results[0].2
    );
    let call_at = events
        .iter()
        .position(|event| matches!(&event.kind, EventKind::ToolCall { .. }))
        .expect("a tool call");
    let result_at = events
        .iter()
        .position(|event| matches!(&event.kind, EventKind::ToolResult { .. }))
        .expect("a tool result");
    assert!(call_at < result_at, "the call precedes its result");

    let diffs = diffs(&events);
    assert_eq!(diffs.len(), 1, "{diffs:?}");
    assert_eq!(
        diffs[0].0, "crates/grove-core/src/lib.rs",
        "the path comes from the aggregated patch"
    );
    assert!(diffs[0].1.contains("+pub mod codex;"), "{}", diffs[0].1);
    assert_eq!(
        (diffs[0].2, diffs[0].3),
        (1, 0),
        "line counts come from the patch"
    );

    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Usage {
                input_tokens: 40,
                output_tokens: 15
            }
        )),
        "thread/tokenUsage/updated becomes usage: {kinds:?}"
    );

    let token = session.resume_token().expect("the thread id is captured");
    assert!(
        token.starts_with("fake-thread-"),
        "the resume token is the app-server thread id: {token}"
    );
}

#[tokio::test]
async fn approvals_become_requests_that_resolve_with_the_answer() {
    assert!(
        CodexAdapter::default().capabilities().permissions,
        "app-server has a real approval channel, so Grove claims it"
    );

    for (decision, expected_status, expected_text) in [
        (Decision::Approve, ToolStatus::Ok, "approved"),
        (Decision::Deny, ToolStatus::Error, "denied"),
    ] {
        let mut session = start("approval", &[]).await;
        let mut rx = session.take_events();
        session
            .send(UserInput::text("clean the build"))
            .await
            .expect("send");

        let events = drain_until(&mut rx, |event| {
            matches!(event.kind, EventKind::PermissionRequest { .. })
        })
        .await;
        let (request_id, title, detail, detail_ok) = events
            .iter()
            .find_map(|event| match &event.kind {
                EventKind::PermissionRequest {
                    request_id,
                    title,
                    detail,
                } => Some((
                    request_id.clone(),
                    title.clone(),
                    detail.clone(),
                    detail.contains("deletes the build directory"),
                )),
                _ => None,
            })
            .expect("an approval request reaches the transcript");
        assert!(!request_id.is_empty(), "the UI needs an id to answer");
        assert_eq!(title, "rm -rf build", "the title is what will run");
        assert!(detail_ok, "the request explains itself: {detail}");

        session
            .respond_permission(&request_id, decision)
            .await
            .expect("the decision is delivered to the app-server");

        let after = drain_until_state(&mut rx, SessionState::Idle).await;
        let kinds = tags(&after);
        assert!(
            after.iter().any(|event| matches!(
                &event.kind,
                EventKind::PermissionResolved {
                    request_id: resolved,
                    decision: resolved_decision,
                } if *resolved == request_id && *resolved_decision == decision
            )),
            "the answer resolves the same request: {kinds:?}"
        );
        assert!(
            !after
                .iter()
                .any(|event| matches!(event.kind, EventKind::Error { .. })),
            "an answered approval is not an error: {kinds:?}"
        );
        let results = tool_results(&after);
        assert_eq!(
            results.len(),
            1,
            "the command finishes after the answer: {results:?}"
        );
        assert_eq!(
            results[0].1, expected_status,
            "the exit code the harness reports picks the status"
        );
        assert!(
            results[0].2.contains(expected_text),
            "the harness acted on the answer: {}",
            results[0].2
        );

        let _ = session.shutdown().await;
    }
}

#[tokio::test]
async fn an_unknown_request_id_is_rejected_instead_of_faking_a_resolution() {
    let mut session = start("approval", &[]).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("clean the build"))
        .await
        .unwrap();
    let events = drain_until(&mut rx, |event| {
        matches!(event.kind, EventKind::PermissionRequest { .. })
    })
    .await;
    let request_id = events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::PermissionRequest { request_id, .. } => Some(request_id.clone()),
            _ => None,
        })
        .expect("an approval request reaches the transcript");

    // Answering an id the harness never sent is an error, not a silent fake resolution.
    let error = session
        .respond_permission("424242", Decision::Approve)
        .await
        .expect_err("there is no such request");
    assert!(
        error.to_string().contains("424242"),
        "the message names the unknown request: {error}"
    );
    let quiet = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(quiet.is_err(), "a rejected answer emits nothing: {quiet:?}");

    // The real request is still answerable, and only it resolves.
    session
        .respond_permission(&request_id, Decision::Deny)
        .await
        .expect("the real request is still waiting");
    let after = drain_until_state(&mut rx, SessionState::Idle).await;
    let resolved: Vec<String> = after
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::PermissionResolved { request_id, .. } => Some(request_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        resolved,
        vec![request_id],
        "exactly the answered request resolves"
    );
}

#[tokio::test]
async fn cancelling_a_turn_that_waits_for_an_approval_answers_the_request() {
    let mut session = start("approval", &[]).await;
    let mut rx = session.take_events();
    session
        .send(UserInput::text("clean the build"))
        .await
        .unwrap();
    let events = drain_until(&mut rx, |event| {
        matches!(event.kind, EventKind::PermissionRequest { .. })
    })
    .await;
    let request_id = events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::PermissionRequest { request_id, .. } => Some(request_id.clone()),
            _ => None,
        })
        .expect("an approval request reaches the transcript");

    // Interrupting a turn that is blocked on an approval must answer the request, or the harness
    // waits for a reply that will never come.
    session.cancel().await.expect("the turn is interrupted");
    let after = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&after);
    assert!(
        !after
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "a cancel is not an error: {kinds:?}"
    );
    assert!(
        after.iter().any(|event| matches!(
            &event.kind,
            EventKind::PermissionResolved {
                request_id: resolved,
                decision: Decision::Deny,
            } if *resolved == request_id
        )),
        "the abandoned approval is resolved as denied: {kinds:?}"
    );
    assert!(
        !after.iter().any(|event| matches!(
            event.kind,
            EventKind::Message {
                role: Role::Assistant,
                ..
            }
        )),
        "the interrupted turn produced no assistant message: {kinds:?}"
    );

    // The approval channel still works for the next turn.
    session
        .send(UserInput::text("clean the build again"))
        .await
        .unwrap();
    let next = drain_until(&mut rx, |event| {
        matches!(event.kind, EventKind::PermissionRequest { .. })
    })
    .await;
    let next_id = next
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::PermissionRequest { request_id, .. } => Some(request_id.clone()),
            _ => None,
        })
        .expect("the next turn asks again");
    session
        .respond_permission(&next_id, Decision::Approve)
        .await
        .expect("the new request is answerable");
    let done = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        tool_results(&done)
            .iter()
            .any(|(_, status, summary)| *status == ToolStatus::Ok && summary.contains("approved")),
        "the session keeps working after the interrupted approval: {:?}",
        tags(&done)
    );
}

#[tokio::test]
async fn an_aggregated_turn_diff_is_reported_once_with_line_counts() {
    let mut session = start("diff", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("touch lib.rs")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    let diffs = diffs(&events);
    assert_eq!(
        diffs.len(),
        1,
        "a re-sent turn/diff/updated is de-duplicated, not double-rendered: {kinds:?}"
    );
    assert_eq!(diffs[0].0, "crates/grove-core/src/lib.rs");
    assert_eq!((diffs[0].2, diffs[0].3), (1, 0));
    assert!(diffs[0].1.contains("@@ -1,3 +1,4 @@"), "{}", diffs[0].1);
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Usage {
                input_tokens: 12,
                output_tokens: 3
            }
        )),
        "{kinds:?}"
    );
    assert!(message_text(&events).contains("Diff for: touch lib.rs"));
}

#[tokio::test]
async fn a_turn_error_is_reported_without_ending_the_session() {
    let mut session = start("error", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("first turn")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    let error = events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::Error { message, retryable } => Some((message.clone(), *retryable)),
            _ => None,
        })
        .expect("the structured JSON-RPC error is reported: {kinds:?}");
    assert!(
        !error.1,
        "a harness-reported turn failure is not retryable as-is"
    );
    assert!(
        error.0.contains("529 overloaded") || error.0.contains("model request failed"),
        "the harness's own message survives: {}",
        error.0
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::Error { .. }))
            .count(),
        1,
        "one failure is one error: {kinds:?}"
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        }),
        "a failed turn returns to idle instead of ending the session: {kinds:?}"
    );

    // The app-server process is still alive and still serving the thread.
    session.send(UserInput::text("try again")).await.unwrap();
    let more = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        message_text(&more).contains("Recovered: try again"),
        "the next turn runs normally: {:?}",
        tags(&more)
    );
    assert!(!more
        .iter()
        .any(|event| matches!(event.kind, EventKind::Error { .. })));
}

#[tokio::test]
async fn a_mid_turn_crash_is_a_retryable_error_and_exited_status() {
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
        .expect("a crash before turn/completed is an error event: {kinds:?}");
    assert!(
        error.1,
        "an unexpected exit is retryable (the thread id lets a resume continue): {}",
        error.0
    );
    assert!(
        error.0.contains("before the turn completed"),
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
    assert!(
        !events
            .iter()
            .any(|event| is_state(event, SessionState::Idle)),
        "a crash is not a clean turn end: {kinds:?}"
    );
    assert!(
        session.resume_token().is_some(),
        "the thread id was captured during the handshake, so a resume can continue the conversation"
    );
}

#[tokio::test]
async fn unknown_and_malformed_frames_are_skipped() {
    let mut session = start("unknown-line", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("hello")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    let kinds = tags(&events);

    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "unknown methods, junk lines and unknown response ids are logged and skipped, never fatal: \
         {kinds:?}"
    );
    assert!(
        message_text(&events).contains("Hello from the fixture."),
        "the valid frames still reach the transcript: {kinds:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Delta { .. })),
        "the deltas still stream: {kinds:?}"
    );
    assert!(
        !kinds.contains(&"tool_call"),
        "unknown frames produce no invented events: {kinds:?}"
    );
    assert_eq!(
        events.last().map(|event| &event.kind),
        Some(&EventKind::Status {
            state: SessionState::Idle
        })
    );
}

#[tokio::test]
async fn cancel_interrupts_the_turn_and_the_session_survives() {
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
    session.cancel().await.expect("turn/interrupt is delivered");

    let after = drain_until_state(&mut rx, SessionState::Idle).await;
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

    // An interrupt is acknowledged, not a kill: the same app-server keeps serving the same thread.
    let token = session.resume_token().expect("the thread id");
    session
        .send(UserInput::text("after the cancel"))
        .await
        .unwrap();
    let resumed = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        message_text(&resumed).contains("A slow answer finished")
            || resumed
                .iter()
                .any(|event| matches!(event.kind, EventKind::Delta { .. })),
        "the session keeps working after a cancel: {:?}",
        tags(&resumed)
    );
    assert_eq!(
        session.resume_token().as_deref(),
        Some(token.as_str()),
        "a cancel does not change the conversation"
    );
}

#[tokio::test]
async fn a_restart_continues_the_same_thread() {
    let mut first = start("happy", &[]).await;
    let mut rx = first.take_events();
    first.send(UserInput::text("first turn")).await.unwrap();
    drain_until_state(&mut rx, SessionState::Idle).await;
    let token = first
        .resume_token()
        .expect("the thread id is the resume token");
    first.shutdown().await.expect("the process is closed");

    let mut resumed_context = context("happy", &[]);
    resumed_context.resume_token = Some(token.clone());
    let mut second = CodexAdapter::default()
        .start(resumed_context, deps())
        .await
        .expect("resume starts the adapter again");

    let mut rx = second.take_events();
    let started = next_event(&mut rx).await;
    match &started.kind {
        EventKind::SessionStarted { resumed, .. } => {
            assert!(
                *resumed,
                "a resumed session reports resumed: true: {started:?}"
            )
        }
        other => panic!("the first event is session_started: {other:?}"),
    }
    assert_eq!(
        second.resume_token().as_deref(),
        Some(token.as_str()),
        "the token is available before the first turn"
    );

    second.send(UserInput::text("second turn")).await.unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        message_text(&events).contains("Fake reply: second turn"),
        "the resumed thread answers: {:?}",
        tags(&events)
    );
    // The fixture echoes the id it was given to `thread/resume`, so an unchanged token proves the
    // adapter resumed instead of starting a new thread.
    assert_eq!(
        second.resume_token().as_deref(),
        Some(token.as_str()),
        "thread/resume kept the same conversation id"
    );
}

#[tokio::test]
async fn an_expired_thread_is_replaced_instead_of_bricking_the_session() {
    // Protocol drift, defensively: a stored thread the server no longer knows must not end the world.
    let mut expired = context("happy", &[]);
    expired.resume_token = Some("expired-thread-1".into());
    let mut session = CodexAdapter::default()
        .start(expired, deps())
        .await
        .expect("start never fails on a stale token");

    let mut rx = session.take_events();
    session
        .send(UserInput::text("after the restart"))
        .await
        .unwrap();
    let events = drain_until_state(&mut rx, SessionState::Idle).await;
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "a refused resume falls back to a new thread: {:?}",
        tags(&events)
    );
    assert!(
        message_text(&events).contains("Fake reply: after the restart"),
        "{:?}",
        tags(&events)
    );
    let token = session
        .resume_token()
        .expect("a fresh thread id replaced the stale one");
    assert_ne!(token, "expired-thread-1");
    assert!(token.starts_with("fake-thread-"), "{token}");
}

#[tokio::test]
async fn the_config_command_and_args_override_any_binary() {
    // The adapter's own command is deliberately useless: config.command/config.args must win, which
    // is what lets users point Grove at a wrapper and tests at a fixture.
    let adapter = CodexAdapter::with_command("grove-definitely-not-a-real-binary");
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
    let adapter = CodexAdapter::with_command("grove-definitely-not-a-real-binary");
    let mut broken = context("happy", &[]);
    broken.config = json!({ "command": "grove-definitely-not-a-real-binary" });

    let mut session = adapter
        .start(broken, deps())
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
    assert!(
        error.0.contains("grove-definitely-not-a-real-binary"),
        "the message names the binary: {}",
        error.0
    );
}

#[tokio::test]
async fn shutdown_closes_the_app_server_process() {
    let mut session = start("happy", &[]).await;
    let mut rx = session.take_events();
    session.send(UserInput::text("hello")).await.unwrap();
    drain_until_state(&mut rx, SessionState::Idle).await;

    session.shutdown().await.expect("shutdown");
    // The actor ends only after the child process is closed, and it drops the event sender with it.
    let closed = tokio::time::timeout(Duration::from_secs(10), async {
        while rx.recv().await.is_some() {}
    })
    .await;
    assert!(
        closed.is_ok(),
        "the session ends after shutdown: no app-server process is left behind"
    );
    assert!(
        session.send(UserInput::text("too late")).await.is_err(),
        "a shut-down session rejects commands instead of panicking"
    );
}

#[test]
fn detection_reports_missing_installs_without_spawning() {
    let missing = CodexAdapter::with_command("grove-definitely-not-a-real-binary").detect(&deps());
    assert!(!missing.installed);
    assert!(
        missing.version.is_none(),
        "detection never runs the binary, so it cannot report a version"
    );
    assert!(
        missing
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("grove-definitely-not-a-real-binary")),
        "the reason names the missing binary: {missing:?}"
    );

    // The fixture runtime itself resolves, which is what the tests rely on.
    let node = CodexAdapter::with_command("node").detect(&deps());
    assert!(node.installed, "node resolves on PATH: {node:?}");
    assert!(node.version.is_none());
}

#[test]
fn capabilities_are_honest() {
    let capabilities = CodexAdapter::default().capabilities();
    assert!(capabilities.streaming_deltas, "item/agentMessage/delta");
    assert!(capabilities.tool_calls, "commandExecution items");
    assert!(capabilities.diffs, "turn/diff/updated");
    assert!(
        capabilities.permissions,
        "server -> client approval requests"
    );
    assert!(capabilities.resume, "thread/resume");
    assert!(capabilities.usage, "thread/tokenUsage/updated");
    assert!(!capabilities.images, "no image mapping yet");
    assert!(
        !capabilities.mcp,
        "mcpToolCall items are logged, not mapped: do not claim mcp"
    );
}

fn write_fake_binary(dir: &Path, name: &str, script: &str) {
    #[cfg(windows)]
    {
        let path = dir.join(format!("{name}.cmd"));
        std::fs::write(
            &path,
            format!("@echo off\r\n{}\r\n", script.replace('\n', "\r\n")),
        )
        .expect("write the fake binary");
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("write the fake binary");
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
    }
}

#[tokio::test]
async fn the_doctor_recognises_the_codex_binary() {
    // A fake `codex` on PATH: the doctor probes `--version` and reports the Codex row.
    let dir = tempfile::tempdir().expect("tempdir");
    write_fake_binary(dir.path(), "codex", "echo codex-cli 0.9.0-fake");

    let paths = AppPaths::new(dir.path().join("app-data"));
    paths.ensure().expect("app paths");
    let manager = Arc::new(SessionManager::in_memory(&paths).expect("the store opens"));
    let settings = Arc::new(SettingsService::new(manager.store()));
    let doctor = HarnessDoctor::with_config(
        Arc::clone(&settings),
        Arc::clone(&manager),
        DoctorConfig {
            path_env: Some(dir.path().to_string_lossy().to_string()),
            path_extensions: None,
            timeout: Duration::from_secs(5),
        },
    );

    let report = doctor.report().await;
    let codex = report
        .harnesses
        .iter()
        .find(|harness| harness.id == "codex")
        .expect("the registered adapter is a doctor row");
    assert_eq!(codex.label, "Codex");
    assert!(codex.installed, "the fake codex binary is found: {codex:?}");
    assert_eq!(codex.version.as_deref(), Some("codex-cli 0.9.0-fake"));
    assert!(
        codex
            .path
            .as_deref()
            .is_some_and(|path| path.contains("codex")),
        "the doctor reports the resolved path: {codex:?}"
    );
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
/// resume from the mirrored thread id. No UI code takes part.
#[tokio::test]
async fn a_codex_session_runs_end_to_end_through_the_session_manager() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manager = SessionManager::open(AppPaths::new(dir.path())).expect("the store opens");

    let harnesses = manager.harnesses();
    let codex = harnesses
        .iter()
        .find(|info| info.id == "codex")
        .expect("codex is registered for the picker");
    assert!(codex.capabilities.resume);
    assert!(codex.capabilities.diffs);
    assert!(
        codex.capabilities.permissions,
        "the picker shows the honest capability set"
    );

    let detail = manager
        .create(CreateSessionRequest {
            harness: "codex".into(),
            cwd: Some(std::env::temp_dir().to_string_lossy().to_string()),
            model: Some("codex-fake-1".into()),
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
        .expect("the thread id is mirrored into sessions.resume_token");
    assert!(token.starts_with("fake-thread-"), "{token}");

    // Restart survival: close the process, resume from the row, and the same thread continues.
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
