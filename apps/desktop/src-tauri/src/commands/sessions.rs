//! Session commands. Thin by design: every decision lives in `grove_core::sessions`, and the shell
//! only maps arguments to manager calls. Events flow back over `grove://session-event`.

use crate::state::AppState;
use grove_core::ipc::{
    CreateSessionRequest, HarnessInfo, IpcResult, Session, SessionDetail, SessionSummary,
};

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
