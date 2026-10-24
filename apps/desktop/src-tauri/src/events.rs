//! Event fan-out. Every `grove://` event carries an [`EventEnvelope`]; the webview validates and
//! ignores kinds it does not know, so a new event kind is additive.

use grove_core::gitops::GitStatus;
use grove_core::ipc::EventEnvelope;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const SESSION_EVENT: &str = "grove://session-event";
pub const GIT_CHANGED: &str = "grove://git-changed";

pub fn emit_session_event<T>(app: &AppHandle, envelope: EventEnvelope<T>)
where
    T: Serialize + Clone,
{
    if let Err(err) = app.emit(SESSION_EVENT, envelope) {
        tracing::warn!(target: "grove::events", "failed to emit {SESSION_EVENT}: {err}");
    }
}

/// A commit succeeded: the payload is the fresh status, so the panel can update without another call.
pub fn emit_git_changed<R: tauri::Runtime>(app: &AppHandle<R>, status: GitStatus) {
    let envelope = EventEnvelope::global(status);
    if let Err(err) = app.emit(GIT_CHANGED, envelope) {
        tracing::warn!(target: "grove::events", "failed to emit {GIT_CHANGED}: {err}");
    }
}
