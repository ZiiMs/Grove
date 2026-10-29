//! Session commands. Thin by design: every decision lives in `grove_core::sessions`, and the shell
//! only maps arguments to manager calls. Events flow back over `grove://session-event`.

use crate::commands::app::adopt;
use crate::state::AppState;
use grove_core::ipc::{CreateSessionRequest, IpcResult, Session, SessionDetail, SessionSummary};
use grove_core::providers::ProviderSpec;
use grove_core::providers::ThinkingLevel;
use grove_core::sessions::context::SessionContext;
use grove_core::sessions::event::{Decision, QuestionAnswer};

/// Every registered provider, with models and auth, for the picker and settings.
#[tauri::command]
pub fn provider_list(state: tauri::State<'_, AppState>) -> IpcResult<Vec<ProviderSpec>> {
    Ok(state.core.sessions.providers())
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
    let detail = state.core.sessions.create(req).await?;
    // A session's working directory is the project the rest of the app should be looking at.
    adopt(&state, detail.session.cwd.clone());
    Ok(detail)
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

/// Answer a pending `permission_request` for one session.
///
/// The adapter owns the protocol (Codex's approval vocabulary, ACP's `session/request_permission`
/// outcome); the manager serializes the answer under the session's command lock, so an approval can
/// never interleave with a send. The resolution comes back as a `permission_resolved` event, which is
/// what clears the buttons in the transcript — the command itself returns nothing.
#[tauri::command]
pub async fn session_respond_permission(
    state: tauri::State<'_, AppState>,
    id: String,
    request_id: String,
    decision: Decision,
) -> IpcResult<()> {
    Ok(state
        .core
        .sessions
        .respond_permission(&id, &request_id, decision)
        .await?)
}

/// Answer a pending `question_request` for one session. Like a permission answer, the command is a
/// thin forwarder; the accepted answer comes back as `question_resolved`.
#[tauri::command]
pub async fn session_respond_question(
    state: tauri::State<'_, AppState>,
    id: String,
    request_id: String,
    answers: Vec<QuestionAnswer>,
) -> IpcResult<()> {
    Ok(state
        .core
        .sessions
        .respond_question(&id, &request_id, answers)
        .await?)
}

/// Change the reasoning level for a session. The runtime applies it to the next turn; the stored
/// config keeps it across resumes.
#[tauri::command]
pub async fn session_set_thinking(
    state: tauri::State<'_, AppState>,
    id: String,
    level: ThinkingLevel,
) -> IpcResult<SessionDetail> {
    Ok(state.core.sessions.set_thinking(&id, level).await?)
}

#[tauri::command]
pub async fn session_close(state: tauri::State<'_, AppState>, id: String) -> IpcResult<()> {
    Ok(state.core.sessions.close(&id).await?)
}

/// Hide a session from the active tree. A running agent is stopped first; the transcript stays.
#[tauri::command]
pub async fn session_archive(state: tauri::State<'_, AppState>, id: String) -> IpcResult<Session> {
    Ok(state.core.sessions.archive(&id).await?)
}

/// Put an archived session back in the active tree. Its agent stays stopped until the next send.
#[tauri::command]
pub async fn session_restore(state: tauri::State<'_, AppState>, id: String) -> IpcResult<Session> {
    Ok(state.core.sessions.restore(&id).await?)
}

/// Delete a session permanently, cascading its events, plan, pins and workers.
#[tauri::command]
pub async fn session_delete(state: tauri::State<'_, AppState>, id: String) -> IpcResult<()> {
    Ok(state.core.sessions.delete(&id).await?)
}

#[tauri::command]
pub async fn session_resume(state: tauri::State<'_, AppState>, id: String) -> IpcResult<Session> {
    let session = state.core.sessions.resume(&id).await?;
    adopt(&state, session.cwd.clone());
    Ok(session)
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

/// Re-read the pins and the plan and push them into the running session: the provider receives the
/// rendered documents as a turn, and the transcript gains a visible note. Returns the new snapshot.
#[tauri::command]
pub async fn session_refresh_context(
    state: tauri::State<'_, AppState>,
    id: String,
) -> IpcResult<SessionContext> {
    state.core.sessions.refresh_context(&id).await?;
    state.core.context.snapshot(&id)
}

/// Re-launch the same conversation on another registered provider, with the same context documents and
/// a summary of the transcript. The transcript itself is never rewritten.
#[tauri::command]
pub async fn session_switch_provider(
    state: tauri::State<'_, AppState>,
    id: String,
    provider: String,
    model: Option<String>,
) -> IpcResult<SessionDetail> {
    Ok(state
        .core
        .sessions
        .switch_provider(&id, &provider, model)
        .await?)
}
