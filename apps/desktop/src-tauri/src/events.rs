//! Event fan-out. Every `grove://` event carries an [`EventEnvelope`]; the webview validates and
//! ignores kinds it does not know, so a new event kind is additive.

use grove_core::gitops::GitStatus;
use grove_core::ipc::EventEnvelope;
use grove_core::plan::Plan;
use grove_core::specgraph::SpecChange;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const SESSION_EVENT: &str = "grove://session-event";
pub const SPEC_CHANGED: &str = "grove://spec-changed";
pub const GIT_CHANGED: &str = "grove://git-changed";
pub const PLAN_CHANGED: &str = "grove://plan-changed";

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

/// A commit succeeded: the payload is the fresh status, so the panel can update without another call.
pub fn emit_git_changed<R: tauri::Runtime>(app: &AppHandle<R>, status: GitStatus) {
    let envelope = EventEnvelope::global(status);
    if let Err(err) = app.emit(GIT_CHANGED, envelope) {
        tracing::warn!(target: "grove::events", "failed to emit {GIT_CHANGED}: {err}");
    }
}

/// An agent edited the plan mirror and `PlanService` imported it: the payload is the new plan, and
/// the session it belongs to rides in the envelope. The panel reloads only if the user has no
/// unsaved draft.
pub fn emit_plan_changed(app: &AppHandle, envelope: EventEnvelope<Plan>) {
    if let Err(err) = app.emit(PLAN_CHANGED, envelope) {
        tracing::warn!(target: "grove::events", "failed to emit {PLAN_CHANGED}: {err}");
    }
}
