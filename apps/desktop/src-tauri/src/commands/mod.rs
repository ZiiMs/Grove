//! Command surface, one module per domain.
//!
//! Each domain owns its handlers; adding one means adding a line to [`handlers`]. Keep that list to
//! one line per domain so parallel work merges cleanly (see `architecture.md` §Extension seams).

pub mod app;
pub mod sessions;

use tauri::ipc::Invoke;

pub fn handlers<R: tauri::Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app::app_info,
        sessions::harness_list,
        sessions::session_list,
        sessions::session_get,
        sessions::session_create,
        sessions::session_send,
        sessions::session_cancel,
        sessions::session_close,
        sessions::session_resume
    ]
}
