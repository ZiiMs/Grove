//! Event fan-out. Every `grove://` event carries an [`EventEnvelope`]; the webview validates and
//! ignores kinds it does not know, so a new event kind is additive.

use grove_core::ipc::EventEnvelope;
use grove_core::specgraph::SpecChange;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const SESSION_EVENT: &str = "grove://session-event";
pub const SPEC_CHANGED: &str = "grove://spec-changed";

pub fn emit_session_event<T>(app: &AppHandle, envelope: EventEnvelope<T>)
where
    T: Serialize + Clone,
{
    if let Err(err) = app.emit(SESSION_EVENT, envelope) {
        tracing::warn!(target: "grove::events", "failed to emit {SESSION_EVENT}: {err}");
    }
}

/// Coalesced spec-file changes from the core's watcher. The webview re-indexes on this event; it is
/// global, so the envelope carries no session id.
pub fn emit_spec_changed(app: &AppHandle, change: SpecChange) {
    if let Err(err) = app.emit(SPEC_CHANGED, EventEnvelope::global(change)) {
        tracing::warn!(target: "grove::events", "failed to emit {SPEC_CHANGED}: {err}");
    }
}
