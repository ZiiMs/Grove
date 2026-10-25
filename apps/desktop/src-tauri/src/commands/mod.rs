//! Command surface, one module per domain.
//!
//! Each domain owns its handlers; adding one means adding a line to [`handlers`]. Keep that list to
//! one line per domain so parallel work merges cleanly (see `architecture.md` §Extension seams).

pub mod app;
pub mod git;
pub mod mcp;
pub mod plan;
pub mod projects;
pub mod sessions;
pub mod settings;
pub mod skills;
pub mod specs;

use tauri::ipc::Invoke;

pub fn handlers<R: tauri::Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app::app_info,
        app::app_set_project_root,
        git::git_status,
        git::git_branches,
        git::git_worktrees,
        git::git_diff,
        git::git_commit,
        git::git_stage,
        git::git_open_pr,
        git::git_worktree_create,
        mcp::mcp_servers,
        mcp::mcp_probe,
        plan::plan_get,
        plan::plan_set,
        projects::project_list,
        projects::project_detect,
        projects::project_add,
        projects::fs_list_dirs,
        sessions::provider_list,
        sessions::session_list,
        sessions::session_get,
        sessions::session_create,
        sessions::session_send,
        sessions::session_cancel,
        sessions::session_respond_permission,
        sessions::session_respond_question,
        sessions::session_close,
        sessions::session_resume,
        sessions::session_context_get,
        sessions::session_pin,
        sessions::session_refresh_context,
        sessions::session_switch_provider,
        settings::settings_get,
        settings::settings_set,
        settings::secret_status,
        settings::secret_set,
        settings::secret_delete,
        settings::provider_statuses,
        settings::provider_key_set,
        settings::provider_key_delete,
        settings::provider_models,
        skills::skill_catalog,
        skills::skill_preview,
        skills::skill_targets,
        skills::skill_install,
        skills::skill_installs,
        skills::skill_remove,
        specs::spec_index,
        specs::spec_read,
        specs::spec_write
    ]
}
