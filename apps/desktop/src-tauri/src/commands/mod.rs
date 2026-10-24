//! Command surface, one module per domain.
//!
//! Each domain owns its handlers; adding one means adding a line to [`handlers`]. Keep that list to
//! one line per domain so parallel work merges cleanly (see `architecture.md` §Extension seams).

pub mod app;
pub mod git;
pub mod plan;
pub mod sessions;
pub mod skills;
pub mod specs;

use tauri::ipc::Invoke;

pub fn handlers<R: tauri::Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app::app_info,
        git::git_status,
        git::git_branches,
        git::git_worktrees,
        git::git_diff,
        git::git_commit,
        git::git_open_pr,
        sessions::harness_list,
        sessions::session_list,
        sessions::session_get,
        sessions::session_create,
        sessions::session_send,
        sessions::session_cancel,
        sessions::session_close,
        sessions::session_resume,
        plan::plan_get,
        plan::plan_set,
        skills::skill_catalog,
        skills::skill_preview,
        skills::skill_targets,
        skills::skill_install,
        specs::spec_index,
        specs::spec_read,
        specs::spec_write
    ]
}
