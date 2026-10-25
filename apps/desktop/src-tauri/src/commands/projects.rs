//! Workspace project commands. The policy lives in `grove_core::projects`; these handlers adapt
//! arguments and keep blocking filesystem/git work off the async runtime.

use crate::commands::app::adopt;
use crate::state::AppState;
use grove_core::ipc::{DirListing, IpcError, IpcResult, Project, ProjectDetection};
use grove_core::projects::AddProjectRequest;

#[tauri::command]
pub async fn project_list(state: tauri::State<'_, AppState>) -> IpcResult<Vec<Project>> {
    Ok(state.core.projects.list()?)
}

#[tauri::command]
pub async fn project_detect(
    state: tauri::State<'_, AppState>,
    path: String,
) -> IpcResult<ProjectDetection> {
    let core = state.core.clone();
    tauri::async_runtime::spawn_blocking(move || -> IpcResult<ProjectDetection> {
        Ok(core.projects.detect(&path)?)
    })
    .await
    .map_err(|error| IpcError::io(format!("project task failed: {error}")))?
}

#[tauri::command]
pub async fn project_add(
    state: tauri::State<'_, AppState>,
    req: AddProjectRequest,
) -> IpcResult<Project> {
    let core = state.core.clone();
    let project = tauri::async_runtime::spawn_blocking(move || -> IpcResult<Project> {
        Ok(core.projects.add(req)?)
    })
    .await
    .map_err(|error| IpcError::io(format!("project task failed: {error}")))??;
    // Adding a project is choosing it: the spec graph, MCP discovery and the project-local skill
    // target should follow immediately, even before the first session exists.
    adopt(&state, &project.path);
    Ok(project)
}

#[tauri::command]
pub async fn fs_list_dirs(path: Option<String>) -> IpcResult<DirListing> {
    tauri::async_runtime::spawn_blocking(move || -> IpcResult<DirListing> {
        Ok(grove_core::projects::list_dirs(path.as_deref())?)
    })
    .await
    .map_err(|error| IpcError::io(format!("folder task failed: {error}")))?
}
