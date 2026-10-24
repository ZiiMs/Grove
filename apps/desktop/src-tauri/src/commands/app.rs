use crate::state::AppState;
use grove_core::ipc::{AppInfo, IpcResult};

/// Header payload for the shell. Contract: `contract/fixtures/app-info.json`.
#[tauri::command]
pub async fn app_info(state: tauri::State<'_, AppState>) -> IpcResult<AppInfo> {
    Ok(state.core.info())
}
