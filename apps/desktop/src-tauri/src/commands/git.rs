//! Git panel commands. Behaviour lives in `grove-core::gitops`; these handlers only unwrap command
//! arguments and keep the blocking `git`/`gh` calls off the async runtime.

use std::path::{Path, PathBuf};

use crate::events;
use crate::state::AppState;
use grove_core::gitops::{
    DiffRequest, GitBranch, GitDiff, GitStatus, GitWorktree, PrRequest, PrResult,
};
use grove_core::ipc::{IpcError, IpcResult};

/// gitops is deliberately synchronous (one testable `CommandRunner` seam); run it on a worker thread
/// so a slow diff cannot stall the webview's command queue.
async fn blocking<T, F>(work: F) -> IpcResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> IpcResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| IpcError::io(format!("git task failed: {err}")))?
}

#[tauri::command]
pub async fn git_status(
    state: tauri::State<'_, AppState>,
    repo_path: Option<String>,
) -> IpcResult<GitStatus> {
    let core = state.core.clone();
    let repo = repo_path.map(PathBuf::from);
    blocking(move || core.git.status(repo.as_deref())).await
}

#[tauri::command]
pub async fn git_branches(
    state: tauri::State<'_, AppState>,
    repo_path: Option<String>,
) -> IpcResult<Vec<GitBranch>> {
    let core = state.core.clone();
    let repo = repo_path.map(PathBuf::from);
    blocking(move || core.git.branches(repo.as_deref())).await
}

#[tauri::command]
pub async fn git_worktrees(
    state: tauri::State<'_, AppState>,
    repo_path: Option<String>,
) -> IpcResult<Vec<GitWorktree>> {
    let core = state.core.clone();
    let repo = repo_path.map(PathBuf::from);
    blocking(move || core.git.worktrees(repo.as_deref())).await
}

#[tauri::command]
pub async fn git_diff(
    state: tauri::State<'_, AppState>,
    repo_path: Option<String>,
    path: Option<String>,
    branch: Option<String>,
    base: Option<String>,
    staged: Option<bool>,
) -> IpcResult<GitDiff> {
    let core = state.core.clone();
    let repo = repo_path.map(PathBuf::from);
    let request = DiffRequest::from_parts(path, branch, base, staged.unwrap_or(false))?;
    blocking(move || core.git.diff(repo.as_deref(), &request)).await
}

#[tauri::command]
pub async fn git_commit<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    repo_path: Option<String>,
    message: String,
    paths: Vec<String>,
) -> IpcResult<GitStatus> {
    let core = state.core.clone();
    let repo = repo_path.map(PathBuf::from);
    let status = blocking(move || core.git.commit(repo.as_deref(), &message, &paths)).await?;
    // The diff/status of every open view is now stale; one event refreshes them.
    events::emit_git_changed(&app, status.clone());
    Ok(status)
}

/// Stage or unstage the selected paths. The changes list reads `staged` from git, not from a
/// client-side set, so a reload shows the same checkboxes the user left.
#[tauri::command]
pub async fn git_stage<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    repo_path: Option<String>,
    paths: Vec<String>,
    staged: bool,
) -> IpcResult<GitStatus> {
    let core = state.core.clone();
    let repo = repo_path.map(PathBuf::from);
    let status = blocking(move || core.git.stage(repo.as_deref(), &paths, staged)).await?;
    events::emit_git_changed(&app, status.clone());
    Ok(status)
}

/// Create a branch plus a worktree checkout for it, then return the new checkout.
#[tauri::command]
pub async fn git_worktree_create(
    state: tauri::State<'_, AppState>,
    repo_path: String,
    branch: String,
    base: String,
    path: String,
) -> IpcResult<GitWorktree> {
    let core = state.core.clone();
    blocking(move || {
        Ok(core
            .projects
            .create_worktree(&repo_path, &branch, &base, &path)?)
    })
    .await
}

/// Remove a linked worktree, optionally deleting its local branch.
#[tauri::command]
pub async fn git_worktree_remove(
    state: tauri::State<'_, AppState>,
    repo_path: String,
    worktree_path: String,
    delete_branch: bool,
    force: bool,
    force_branch: bool,
) -> IpcResult<()> {
    let core = state.core.clone();
    blocking(move || {
        core.git.remove_worktree(
            Some(Path::new(&repo_path)),
            &worktree_path,
            delete_branch,
            force,
            force_branch,
        )
    })
    .await
}

#[tauri::command]
pub async fn git_open_pr(
    state: tauri::State<'_, AppState>,
    repo_path: Option<String>,
    title: String,
    body: String,
    base: Option<String>,
    head: Option<String>,
    draft: Option<bool>,
) -> IpcResult<PrResult> {
    let core = state.core.clone();
    let repo = repo_path.map(PathBuf::from);
    let mut request = PrRequest::new(title, body);
    request.base = base;
    request.head = head;
    request.draft = draft.unwrap_or(false);
    blocking(move || core.git.open_pr(repo.as_deref(), &request)).await
}
