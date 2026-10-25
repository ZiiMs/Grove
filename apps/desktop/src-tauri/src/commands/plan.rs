//! Plan commands. The dual-edit rules live in `grove_core::plan`; the shell only maps arguments.
//!
//! `plan_get` re-checks the mirror before answering, so an agent's external edit is imported and
//! announced over `grove://plan-changed`. `plan_set` carries the version the editor was showing; a
//! stale save is rejected with `conflict` and the panel reloads instead of clobbering.

use crate::state::AppState;
use grove_core::ipc::IpcResult;
use grove_core::plan::{Plan, PlanUpdate};

/// One session's plan, after importing any external edit.
#[tauri::command]
pub fn plan_get(state: tauri::State<'_, AppState>, id: String) -> IpcResult<Plan> {
    state.core.plan.get(&id)
}

/// Save the human's edit at `update.expected_version`.
#[tauri::command]
pub fn plan_set(
    state: tauri::State<'_, AppState>,
    id: String,
    update: PlanUpdate,
) -> IpcResult<Plan> {
    state.core.plan.set(&id, update)
}
