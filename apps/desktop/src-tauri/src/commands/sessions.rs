//! Session commands. Thin by design: every decision lives in `grove_core::sessions`, and the shell
//! only maps arguments to manager calls. Events flow back over `grove://session-event`.

use crate::state::AppState;
use grove_core::ipc::{
    CreateSessionRequest, HarnessInfo, IpcResult, Session, SessionDetail, SessionSummary,
};
use grove_core::sessions::context::SessionContext;

/// Every registered harness, for the session picker.
#[tauri::command]
pub fn harness_list(state: tauri::State<'_, AppState>) -> IpcResult<Vec<HarnessInfo>> {
    Ok(state.core.sessions.harnesses())
}

#[tauri::command]
pub fn session_list(state: tauri::State<'_, AppState>) -> IpcResult<Vec<SessionSummary>> {
    Ok(state.core.sessions.list()?)
}

#[tauri::command]
pub fn session_get(state: tauri::State<'_, AppState>, id: String) -> IpcResult<SessionDetail> {
    Ok(state.core.sessions.get(&id)?)
}

#[tauri::command]
pub async fn session_create(
    state: tauri::State<'_, AppState>,
    req: CreateSessionRequest,
) -> IpcResult<SessionDetail> {
    Ok(state.core.sessions.create(req).await?)
}

#[tauri::command]
pub async fn session_send(
    state: tauri::State<'_, AppState>,
    id: String,
    text: String,
) -> IpcResult<()> {
    Ok(state.core.sessions.send(&id, &text).await?)
}

#[tauri::command]
pub async fn session_cancel(state: tauri::State<'_, AppState>, id: String) -> IpcResult<()> {
    Ok(state.core.sessions.cancel(&id).await?)
}

#[tauri::command]
pub async fn session_close(state: tauri::State<'_, AppState>, id: String) -> IpcResult<()> {
    Ok(state.core.sessions.close(&id).await?)
}

#[tauri::command]
pub async fn session_resume(state: tauri::State<'_, AppState>, id: String) -> IpcResult<Session> {
    Ok(state.core.sessions.resume(&id).await?)
}

/// A session's knowledge: what is pinned and what a start would send now. Reads the spec graph and the
/// plan, so the header can show the current context without a session running.
#[tauri::command]
pub fn session_context_get(
    state: tauri::State<'_, AppState>,
    id: String,
) -> IpcResult<SessionContext> {
    state.core.context.snapshot(&id)
}

/// Pin or unpin a spec for one session. Returns the refreshed snapshot so the panel needs no second
/// call; pins live in SQLite and survive a restart.
#[tauri::command]
pub fn session_pin(
    state: tauri::State<'_, AppState>,
    id: String,
    spec_id: String,
    pinned: bool,
) -> IpcResult<SessionContext> {
    if pinned {
        Ok(state.core.context.pin_spec(&id, &spec_id)?)
    } else {
        Ok(state.core.context.unpin_spec(&id, &spec_id)?)
    }
}

/// Re-read the pins and the plan and push them into the running session: the harness receives the
/// rendered documents as a turn, and the transcript gains a visible note. Returns the new snapshot.
#[tauri::command]
pub async fn session_refresh_context(
    state: tauri::State<'_, AppState>,
    id: String,
) -> IpcResult<SessionContext> {
    state.core.sessions.refresh_context(&id).await?;
    state.core.context.snapshot(&id)
}

/// Re-launch the same conversation on another registered harness, with the same context documents and
/// a summary of the transcript. The transcript itself is never rewritten.
#[tauri::command]
pub async fn session_switch_harness(
    state: tauri::State<'_, AppState>,
    id: String,
    harness: String,
) -> IpcResult<SessionDetail> {
    Ok(state.core.sessions.switch_harness(&id, &harness).await?)
}
