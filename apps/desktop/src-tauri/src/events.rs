//! Event fan-out. Every `grove://` event carries an [`EventEnvelope`]; the webview validates and
//! ignores kinds it does not know, so a new event kind is additive.

use grove_core::ipc::EventEnvelope;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const SESSION_EVENT: &str = "grove://session-event";

pub fn emit_session_event<T>(app: &AppHandle, envelope: EventEnvelope<T>)
where
    T: Serialize + Clone,
{
    if let Err(err) = app.emit(SESSION_EVENT, envelope) {
        tracing::warn!(target: "grove::events", "failed to emit {SESSION_EVENT}: {err}");
    }
}
