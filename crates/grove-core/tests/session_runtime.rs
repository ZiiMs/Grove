//! End-to-end session runtime against the offline provider: no Tauri, no network, no keys.

use grove_core::ipc::{CreateSessionRequest, EventEnvelope};
use grove_core::paths::AppPaths;
use grove_core::sessions::event::{EventKind, Role, SessionEvent, SessionState, ToolStatus};
use grove_core::sessions::manager::SessionManager;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(5);

fn manager(dir: &tempfile::TempDir) -> SessionManager {
    SessionManager::open_offline(AppPaths::new(dir.path())).expect("the store opens")
}

fn mock_session(chunk_delay_ms: u64) -> CreateSessionRequest {
    CreateSessionRequest {
        provider: "mock".into(),
        project_id: None,
        worktree_path: None,
        config: Some(serde_json::json!({ "chunkDelayMs": chunk_delay_ms })),
        ..Default::default()
    }
}

fn is_state(kind: &EventKind, state: SessionState) -> bool {
    matches!(kind, EventKind::Status { state: current } if *current == state)
}

async fn next_event(bus: &mut broadcast::Receiver<EventEnvelope<SessionEvent>>) -> SessionEvent {
    timeout(WAIT, bus.recv())
        .await
        .expect("an event arrives in time")
        .expect("the bus is open")
        .payload
}

#[tokio::test]
async fn a_mock_session_streams_deltas_a_message_and_a_tool_call() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(&dir);
    let mut bus = manager.subscribe();

    let detail = manager.create(mock_session(1)).await.unwrap();
    let id = detail.session.id.clone();
    assert_eq!(detail.session.provider, "mock");
    assert_eq!(detail.session.status, SessionState::Ready);

    manager.send(&id, "hello grove").await.unwrap();

    let mut streamed = Vec::new();
    loop {
        let event = next_event(&mut bus).await;
        let done = is_state(&event.kind, SessionState::Idle);
        streamed.push(event);
        if done {
            break;
        }
    }

    // Persist-then-broadcast means everything the UI saw is already in the log, in the same order.
    let persisted = manager.get(&id).unwrap().events;
    assert_eq!(persisted, streamed);

    let tags: Vec<&str> = persisted.iter().map(|event| event.kind.tag()).collect();
    assert_eq!(tags.last(), Some(&"status"));
    for required in [
        "session_started",
        "delta",
        "message",
        "tool_call",
        "tool_result",
        "usage",
    ] {
        assert!(tags.contains(&required), "missing {required} in {tags:?}");
    }

    // The user's own message is in the log before the reply.
    let user = persisted
        .iter()
        .position(|event| {
            matches!(
                &event.kind,
                EventKind::Message {
                    role: Role::User,
                    ..
                }
            )
        })
        .expect("the user message is persisted");
    let assistant = persisted
        .iter()
        .position(|event| {
            matches!(
                &event.kind,
                EventKind::Message {
                    role: Role::Assistant,
                    ..
                }
            )
        })
        .expect("the assistant message is persisted");
    assert!(user < assistant, "the user message comes first");

    let call = persisted
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::ToolCall { call_id, name, .. } => Some((call_id.clone(), name.clone())),
            _ => None,
        })
        .expect("a tool call is emitted");
    assert_eq!(call.1, "echo");
    assert!(
        persisted.iter().any(|event| matches!(
            &event.kind,
            EventKind::ToolResult { call_id, status: ToolStatus::Ok, .. } if *call_id == call.0
        )),
        "the tool call is answered by a result"
    );

    let session = manager.get(&id).unwrap().session;
    assert_eq!(session.status, SessionState::Idle);
    assert_eq!(session.provider, "mock");
    assert_eq!(
        session.title, "hello grove",
        "an untitled session takes its first message as the placeholder"
    );
    assert_eq!(
        session.title_source,
        grove_core::sessions::event::TitleSource::Auto
    );

    let summary = manager.list().unwrap();
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].event_count as usize, persisted.len());
}

#[tokio::test]
async fn cancel_stops_a_stream_and_returns_the_session_to_idle() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(&dir);
    let mut bus = manager.subscribe();

    let detail = manager.create(mock_session(100)).await.unwrap();
    let id = detail.session.id.clone();
    manager
        .send(&id, "a message that streams slowly")
        .await
        .unwrap();

    // Wait for the reply to be visibly streaming, then stop it.
    loop {
        let event = next_event(&mut bus).await;
        if matches!(event.kind, EventKind::Delta { .. }) {
            break;
        }
    }
    manager.cancel(&id).await.unwrap();

    loop {
        let event = next_event(&mut bus).await;
        if is_state(&event.kind, SessionState::Idle) {
            break;
        }
    }

    let detail = manager.get(&id).unwrap();
    assert_eq!(detail.session.status, SessionState::Idle);
    assert!(
        !detail.events.iter().any(|event| matches!(
            &event.kind,
            EventKind::Message {
                role: Role::Assistant,
                ..
            }
        )),
        "a cancelled reply never produces an assistant message"
    );
    assert!(
        detail
            .events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Delta { .. })),
        "the partial reply that was shown stays in the log"
    );
}

#[tokio::test]
async fn an_unknown_provider_cannot_be_started() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager(&dir);
    let error = manager
        .create(CreateSessionRequest {
            // Ticket 08 registered `claude`; this name must never be a provider.
            provider: "not-a-provider".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(
        grove_core::ipc::IpcError::from(error).code,
        grove_core::ipc::ErrorCode::Invalid
    );
    assert!(manager.list().unwrap().is_empty());
}
