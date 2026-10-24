use grove_core::ipc::{
    AppInfo, ErrorCode, EventEnvelope, IpcError, Session, SessionDetail, SessionSummary,
};
use grove_core::sessions::event::{
    Block, Decision, EventKind, HarnessEvent, Role, SessionState, ToolStatus,
};

/// Each fixture is canonical JSON produced by the Rust side. A change to a DTO without a matching
/// fixture change fails here, and the TS side fails too — that is the contract test.
fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract/fixtures")
        .join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read fixture {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("fixture is valid JSON")
}

#[test]
fn app_info_matches_fixture() {
    let sample = AppInfo {
        name: "Grove".into(),
        version: "0.1.0".into(),
        app_data_dir: "C:/Users/demo/AppData/Roaming/Grove".into(),
        platform: "windows".into(),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("app-info.json")
    );
}

#[test]
fn event_envelope_matches_fixture() {
    let sample = EventEnvelope::session(
        "session-1",
        serde_json::json!({ "kind": "status", "state": "idle" }),
    );
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("event-envelope.json")
    );
}

#[test]
fn ipc_error_matches_fixture() {
    let sample = IpcError {
        code: ErrorCode::Conflict,
        message: "plan changed on disk".into(),
        detail: Some("expected version 3".into()),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("ipc-error.json")
    );
}

// ---------- sessions (ticket 02) ----------
//
// One fixture per session DTO, plus one covering every `HarnessEvent` payload variant. The same files
// are parsed by the zod schemas in `packages/contract`, so either side drifting breaks a test.

fn sample_session() -> Session {
    Session {
        id: "session-1".into(),
        harness: "mock".into(),
        cwd: "C:/projects/grove".into(),
        model: Some("mock-1".into()),
        title: "Refactor the store".into(),
        status: SessionState::Idle,
        created_at: "2025-01-01T00:00:00.000Z".into(),
        updated_at: "2025-01-01T00:00:05.000Z".into(),
    }
}

fn sample_events() -> Vec<HarnessEvent> {
    vec![
        event(
            EventKind::SessionStarted {
                harness: "mock".into(),
                model: Some("mock-1".into()),
                resumed: false,
            },
            "2025-01-01T00:00:00.500Z",
        ),
        event(
            EventKind::Status {
                state: SessionState::Ready,
            },
            "2025-01-01T00:00:00.600Z",
        ),
        event(
            EventKind::Delta {
                text: "Mock ".into(),
            },
            "2025-01-01T00:00:01.000Z",
        ),
        event(
            EventKind::Message {
                role: Role::Assistant,
                blocks: vec![
                    Block::Text {
                        text: "Mock reply: hello".into(),
                    },
                    Block::Thinking {
                        text: "the mock has nothing to think about".into(),
                    },
                    Block::Code {
                        language: "rust".into(),
                        text: "fn main() {}".into(),
                    },
                ],
            },
            "2025-01-01T00:00:01.500Z",
        ),
        event(
            EventKind::ToolCall {
                call_id: "mock-call-1".into(),
                name: "echo".into(),
                title: "echo hello".into(),
            },
            "2025-01-01T00:00:02.000Z",
        ),
        event(
            EventKind::ToolResult {
                call_id: "mock-call-1".into(),
                status: ToolStatus::Ok,
                summary: "echoed 5 chars".into(),
            },
            "2025-01-01T00:00:02.500Z",
        ),
        event(
            EventKind::Diff {
                path: "src/main.rs".into(),
                patch: "@@ -1 +1 @@\n-old\n+new".into(),
                added: 1,
                removed: 1,
            },
            "2025-01-01T00:00:03.000Z",
        ),
        event(
            EventKind::PermissionRequest {
                request_id: "req-1".into(),
                title: "Run a command".into(),
                detail: "rm -rf build".into(),
            },
            "2025-01-01T00:00:03.500Z",
        ),
        event(
            EventKind::PermissionResolved {
                request_id: "req-1".into(),
                decision: Decision::Deny,
            },
            "2025-01-01T00:00:03.600Z",
        ),
        event(
            EventKind::Usage {
                input_tokens: 5,
                output_tokens: 4,
            },
            "2025-01-01T00:00:04.000Z",
        ),
        event(
            EventKind::Status {
                state: SessionState::Idle,
            },
            "2025-01-01T00:00:04.500Z",
        ),
        event(
            EventKind::Error {
                message: "harness exited".into(),
                retryable: true,
            },
            "2025-01-01T00:00:05.000Z",
        ),
    ]
}

fn event(kind: EventKind, at: &str) -> HarnessEvent {
    HarnessEvent::at(kind, at)
}

#[test]
fn session_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_session()).unwrap(),
        fixture("session.json")
    );
}

#[test]
fn session_summary_matches_fixture() {
    let sample = SessionSummary {
        session: sample_session(),
        event_count: 12,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("session-summary.json")
    );
}

#[test]
fn session_detail_matches_fixture() {
    let events = sample_events();
    let sample = SessionDetail {
        session: sample_session(),
        events: vec![events[0].clone(), events[3].clone()],
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("session-detail.json")
    );
}

#[test]
fn every_harness_event_variant_matches_the_fixture() {
    let events = sample_events();
    assert_eq!(
        serde_json::to_value(&events).unwrap(),
        fixture("session-events.json")
    );

    let tags: Vec<&str> = events.iter().map(|event| event.kind.tag()).collect();
    for required in [
        "session_started",
        "delta",
        "message",
        "tool_call",
        "tool_result",
        "diff",
        "permission_request",
        "permission_resolved",
        "usage",
        "status",
        "error",
    ] {
        assert!(tags.contains(&required), "fixture misses {required}");
    }
}
