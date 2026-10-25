//! One pump task per live session.
//!
//! The pump is the **single writer** for a session: it drains the adapter's event receiver, appends
//! each event to the SQLite log, and broadcasts it on the in-process bus. Persist-then-broadcast
//! means the log on disk is always at least as complete as what the UI has painted, so a reload can
//! never drop an event the user already saw.
//!
//! Events the manager authors itself (the user's message, a close) are injected into the same pump
//! through [`InjectedEvent`], which is what keeps the log order and the stream order identical.

use crate::ipc::EventEnvelope;
use crate::providers::ResumeToken;
use crate::sessions::event::{EventKind, SessionEvent, SessionState};
use crate::store::Store;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};

/// The bus the Tauri shell subscribes to and forwards as `grove://session-event`.
pub type SessionBus = broadcast::Sender<EventEnvelope<SessionEvent>>;

/// A manager-authored event for the pump, optionally acknowledged once it is persisted and broadcast.
pub struct InjectedEvent {
    pub event: SessionEvent,
    pub ack: Option<oneshot::Sender<()>>,
}

impl InjectedEvent {
    pub fn new(event: SessionEvent) -> Self {
        Self { event, ack: None }
    }

    pub fn acknowledged(event: SessionEvent) -> (Self, oneshot::Receiver<()>) {
        let (ack, received) = oneshot::channel();
        (
            Self {
                event,
                ack: Some(ack),
            },
            received,
        )
    }
}

/// Persist one event, mirror its status, then broadcast it.
///
/// Call this directly only when the session has no pump (the manager records the final `exited`
/// after aborting it); otherwise it is the pump's private writer.
pub(crate) fn publish(store: &Store, bus: &SessionBus, session_id: &str, event: &SessionEvent) {
    if let Err(error) = store.append_event(session_id, event) {
        tracing::warn!(
            target: "grove::sessions",
            session = session_id,
            "failed to persist event: {error}"
        );
    }
    if let EventKind::Status { state } = &event.kind {
        if let Err(error) = store.update_session_status(session_id, *state) {
            tracing::warn!(
                target: "grove::sessions",
                session = session_id,
                "failed to persist status: {error}"
            );
        }
        // A session that stopped cannot still be waiting for an answer.
        if matches!(state, SessionState::Exited | SessionState::Error) {
            if let Err(error) = store.update_session_awaiting(session_id, false) {
                tracing::warn!(
                    target: "grove::sessions",
                    session = session_id,
                    "failed to clear awaiting: {error}"
                );
            }
        }
    }
    match &event.kind {
        EventKind::PermissionRequest { .. } => {
            if let Err(error) = store.update_session_awaiting(session_id, true) {
                tracing::warn!(
                    target: "grove::sessions",
                    session = session_id,
                    "failed to mirror awaiting: {error}"
                );
            }
        }
        EventKind::PermissionResolved { .. } => {
            if let Err(error) = store.update_session_awaiting(session_id, false) {
                tracing::warn!(
                    target: "grove::sessions",
                    session = session_id,
                    "failed to clear awaiting: {error}"
                );
            }
        }
        _ => {}
    }
    // No receivers is normal (headless tests, a closed window); the event is still on disk.
    let _ = bus.send(EventEnvelope::session(session_id, event.clone()));
}

/// Keep `sessions.resume_token` current for adapters that learn the id from the provider's first line.
///
/// Tokens change rarely (once per conversation, or once per resumed conversation), so the slot is
/// compared against the last value written and the store is only touched on a change.
fn mirror_resume_token(
    store: &Store,
    session_id: &str,
    slot: &Option<ResumeToken>,
    mirrored: &mut Option<String>,
) {
    let Some(slot) = slot else { return };
    let token = slot.lock().ok().and_then(|token| token.clone());
    let Some(token) = token else { return };
    if mirrored.as_deref() == Some(token.as_str()) {
        return;
    }
    match store.set_resume_token(session_id, Some(&token)) {
        Ok(()) => *mirrored = Some(token),
        Err(error) => tracing::warn!(
            target: "grove::sessions",
            session = session_id,
            "failed to mirror the resume token: {error}"
        ),
    }
}

/// Drain adapter events and manager-injected events for the life of the session.
///
/// When the adapter's channel closes without the manager closing it, the provider exited on its own:
/// record `status exited` so the UI can offer Resume.
pub fn spawn(
    store: Arc<Store>,
    bus: SessionBus,
    session_id: String,
    mut events: mpsc::Receiver<SessionEvent>,
    mut injected: mpsc::UnboundedReceiver<InjectedEvent>,
    resume_token: Option<ResumeToken>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut injector_open = true;
        let mut mirrored: Option<String> = None;
        // An adapter that knows its token at start (the mock) is mirrored before the first event.
        mirror_resume_token(&store, &session_id, &resume_token, &mut mirrored);
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Some(event) => {
                        mirror_resume_token(&store, &session_id, &resume_token, &mut mirrored);
                        publish(&store, &bus, &session_id, &event);
                    }
                    None => break,
                },
                injected_event = injected.recv(), if injector_open => match injected_event {
                    Some(InjectedEvent { event, ack }) => {
                        mirror_resume_token(&store, &session_id, &resume_token, &mut mirrored);
                        publish(&store, &bus, &session_id, &event);
                        if let Some(ack) = ack {
                            let _ = ack.send(());
                        }
                    }
                    None => injector_open = false,
                },
            }
        }
        let exited = SessionEvent::new(EventKind::Status {
            state: SessionState::Exited,
        });
        publish(&store, &bus, &session_id, &exited);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::Session;
    use crate::sessions::event::{Decision, SessionState};
    use std::sync::Arc;

    fn store_with_session() -> Arc<Store> {
        let store = Arc::new(Store::open_in_memory().expect("in-memory store"));
        store
            .insert_session(
                &Session {
                    id: "session-1".into(),
                    provider: "mock".into(),
                    cwd: "C:/projects/grove".into(),
                    model: None,
                    title: "Awaiting".into(),
                    status: SessionState::Busy,
                    project_id: None,
                    awaiting: false,
                    created_at: "2025-01-01T00:00:00.000Z".into(),
                    updated_at: "2025-01-01T00:00:00.000Z".into(),
                },
                &serde_json::json!({}),
            )
            .expect("insert");
        store
    }

    fn publish_kind(store: &Store, bus: &SessionBus, kind: EventKind) {
        publish(
            store,
            bus,
            "session-1",
            &SessionEvent::at(kind, "2025-01-01T00:00:01.000Z"),
        );
    }

    fn awaiting(store: &Store) -> bool {
        store
            .session("session-1")
            .expect("read")
            .expect("present")
            .awaiting
    }

    #[test]
    fn a_permission_request_sets_awaiting_until_it_is_resolved() {
        let store = store_with_session();
        let (bus, _) = broadcast::channel(8);

        publish_kind(
            &store,
            &bus,
            EventKind::PermissionRequest {
                request_id: "req-1".into(),
                title: "cargo test".into(),
                detail: "Runs the suite".into(),
            },
        );
        assert!(awaiting(&store));

        publish_kind(
            &store,
            &bus,
            EventKind::PermissionResolved {
                request_id: "req-1".into(),
                decision: Decision::Approve,
            },
        );
        assert!(!awaiting(&store));
    }

    #[test]
    fn a_stopped_session_is_never_still_awaiting() {
        let store = store_with_session();
        let (bus, _) = broadcast::channel(8);

        publish_kind(
            &store,
            &bus,
            EventKind::PermissionRequest {
                request_id: "req-1".into(),
                title: "cargo test".into(),
                detail: "Runs the suite".into(),
            },
        );
        assert!(awaiting(&store));

        publish_kind(
            &store,
            &bus,
            EventKind::Status {
                state: SessionState::Exited,
            },
        );
        assert!(
            !awaiting(&store),
            "a stopped session cannot still be waiting"
        );
    }
}
