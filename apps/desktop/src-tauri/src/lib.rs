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
            let core = grove_core::GroveCore::init(paths)?;

            // Forward the session bus to the webview. `grove-core` never imports tauri: it publishes
            // every persisted event on a broadcast channel and this loop is the only fan-out.
            let mut session_events = core.sessions.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(envelope) = session_events.recv().await {
                    events::emit_session_event(&handle, envelope);
                }
            });

            app.manage(AppState::new(core));
            Ok(())
        })
        .invoke_handler(commands::handlers())
        .run(tauri::generate_context!())
        .expect("failed to start Grove");
}
