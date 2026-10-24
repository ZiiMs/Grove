//! Command surface, one module per domain.
//!
//! Each domain owns its handlers; adding one means adding a line to [`handlers`]. Keep that list to
//! one line per domain so parallel work merges cleanly (see `architecture.md` §Extension seams).

pub mod app;
pub mod git;

use tauri::ipc::Invoke;

pub fn handlers<R: tauri::Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app::app_info,
        git::git_status,
        git::git_branches,
        git::git_worktrees,
        git::git_diff,
        git::git_commit,
        git::git_open_pr
    ]
}
