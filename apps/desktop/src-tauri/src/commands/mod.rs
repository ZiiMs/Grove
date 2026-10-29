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

pub fn handlers() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app::app_info,
        app::app_set_project_root,
        app::app_reveal_path,
        app::app_window_minimize,
        app::app_window_toggle_maximize,
        app::app_window_is_maximized,
        app::app_window_close,
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
        projects::project_remove,
        projects::fs_list_dirs,
        projects::fs_list_roots,
        projects::fs_create_dir,
        sessions::provider_list,
        sessions::session_list,
        sessions::session_get,
        sessions::session_create,
        sessions::session_send,
        sessions::session_cancel,
        sessions::session_respond_permission,
        sessions::session_respond_question,
        sessions::session_set_thinking,
        sessions::session_rename,
        sessions::session_set_model,
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
        settings::provider_thinking,
        skills::skill_catalog,
        skills::skill_preview,
        skills::skill_targets,
        skills::skill_inventory,
        skills::skill_install,
        skills::skill_installs,
        skills::skill_remove,
        specs::spec_index,
        specs::spec_read,
        specs::spec_write
    ]
}
