//! Restart survival: sessions are rows, not processes. Quit, reopen the store at the same path, and
//! find the transcript intact — then resume the conversation and keep going.

use grove_core::ipc::CreateSessionRequest;
use grove_core::paths::AppPaths;
use grove_core::sessions::event::{Block, EventKind, HarnessEvent, Role, SessionState};
use grove_core::sessions::manager::SessionManager;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(5);

fn paths(dir: &tempfile::TempDir) -> AppPaths {
    AppPaths::new(dir.path())
}

fn is_state(kind: &EventKind, state: SessionState) -> bool {
    matches!(kind, EventKind::Status { state: current } if *current == state)
}

fn idle_count(events: &[HarnessEvent]) -> usize {
    events
        .iter()
        .filter(|event| is_state(&event.kind, SessionState::Idle))
        .count()
}

async fn wait_for(
    manager: &SessionManager,
    id: &str,
    done: impl Fn(&[HarnessEvent]) -> bool,
) -> Vec<HarnessEvent> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let events = manager.get(id).expect("the session exists").events;
        if done(&events) {
            return events;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the transcript never reached the expected state; last events: {events:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
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

#[tokio::test]
async fn a_session_survives_a_restart_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    let database = paths(&dir).database;

    let (session_id, before) = {
        let manager = SessionManager::open(paths(&dir)).unwrap();
        let detail = manager
            .create(CreateSessionRequest {
                harness: "mock".into(),
                config: Some(serde_json::json!({ "chunkDelayMs": 1 })),
                ..Default::default()
            })
            .await
            .unwrap();
        let session_id = detail.session.id.clone();

        manager.send(&session_id, "first").await.unwrap();
        let before = wait_for(&manager, &session_id, |events| idle_count(events) >= 1).await;
        assert!(user_texts(&before).contains(&"first"));
        // The manager (and its pump) go away here: this is the app quitting.
        (session_id, before)
    };

    assert!(database.exists(), "the transcript lives in one file");

    let manager = SessionManager::open(paths(&dir)).unwrap();

    // The reopen is independent of the old pump's last write, so wait for it to settle.
    let restored = wait_for(&manager, &session_id, |events| {
        events
            .last()
            .is_some_and(|event| is_state(&event.kind, SessionState::Exited))
    })
    .await;

    assert_eq!(
        &restored[..before.len()],
        &before[..],
        "the transcript comes back exactly as it was written"
    );
    assert_eq!(
        restored.len(),
        before.len() + 1,
        "the only appended event is the exit recorded when the process went away"
    );
    assert_eq!(
        manager.get(&session_id).unwrap().session.status,
        SessionState::Exited
    );
    assert_eq!(
        manager.list().unwrap()[0].event_count as usize,
        restored.len()
    );

    // Resume the same conversation: the mock's stored resume token is handed back to it.
    let resumed = manager.resume(&session_id).await.unwrap();
    assert_eq!(resumed.id, session_id);
    assert_eq!(resumed.harness, "mock");

    manager.send(&session_id, "second").await.unwrap();
    let after = wait_for(&manager, &session_id, |events| idle_count(events) >= 2).await;

    assert!(
        after
            .iter()
            .any(|event| matches!(&event.kind, EventKind::SessionStarted { resumed: true, .. })),
        "the adapter was restarted with its resume token"
    );
    assert_eq!(user_texts(&after), vec!["first", "second"]);
    assert_eq!(
        manager.get(&session_id).unwrap().session.status,
        SessionState::Idle
    );
    assert!(after.len() > restored.len());
}
