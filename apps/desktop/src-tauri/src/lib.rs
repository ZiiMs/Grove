//! The Tauri shell: window, IPC command surface, event fan-out. Behaviour lives in `grove-core`;
//! if this file grows a policy, it is in the wrong place.

pub mod commands;
pub mod events;
pub mod state;

use grove_core::paths::AppPaths;
use state::AppState;
use tauri::Manager;

pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let paths = AppPaths::new(data_dir);
            paths.ensure()?;
            app.manage(AppState::new(grove_core::GroveCore::new(paths)));
            Ok(())
        })
        .invoke_handler(commands::handlers())
        .run(tauri::generate_context!())
        .expect("failed to start Grove");
}
