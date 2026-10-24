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
use crate::sessions::event::{EventKind, HarnessEvent, SessionState};
use crate::store::Store;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};

/// The bus the Tauri shell subscribes to and forwards as `grove://session-event`.
pub type SessionBus = broadcast::Sender<EventEnvelope<HarnessEvent>>;

/// A manager-authored event for the pump, optionally acknowledged once it is persisted and broadcast.
pub struct InjectedEvent {
    pub event: HarnessEvent,
    pub ack: Option<oneshot::Sender<()>>,
}

impl InjectedEvent {
    pub fn new(event: HarnessEvent) -> Self {
        Self { event, ack: None }
    }

    pub fn acknowledged(event: HarnessEvent) -> (Self, oneshot::Receiver<()>) {
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
pub(crate) fn publish(store: &Store, bus: &SessionBus, session_id: &str, event: &HarnessEvent) {
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
    }
    // No receivers is normal (headless tests, a closed window); the event is still on disk.
    let _ = bus.send(EventEnvelope::session(session_id, event.clone()));
}

/// Drain adapter events and manager-injected events for the life of the session.
///
/// When the adapter's channel closes without the manager closing it, the harness exited on its own:
/// record `status exited` so the UI can offer Resume.
pub fn spawn(
    store: Arc<Store>,
    bus: SessionBus,
    session_id: String,
    mut events: mpsc::Receiver<HarnessEvent>,
    mut injected: mpsc::UnboundedReceiver<InjectedEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut injector_open = true;
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Some(event) => publish(&store, &bus, &session_id, &event),
                    None => break,
                },
                injected_event = injected.recv(), if injector_open => match injected_event {
                    Some(InjectedEvent { event, ack }) => {
                        publish(&store, &bus, &session_id, &event);
                        if let Some(ack) = ack {
                            let _ = ack.send(());
                        }
                    }
                    None => injector_open = false,
                },
            }
        }
        let exited = HarnessEvent::new(EventKind::Status {
            state: SessionState::Exited,
        });
        publish(&store, &bus, &session_id, &exited);
    })
}
