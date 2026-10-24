//! The Tauri shell: window, IPC command surface, event fan-out. Behaviour lives in `grove-core`;
//! if this file grows a policy, it is in the wrong place.

pub mod commands;
pub mod events;
pub mod state;

use grove_core::paths::AppPaths;
use grove_core::specgraph::SpecWatcher;
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
            let state = AppState::new(core);

            // Forward the session bus to the webview. `grove-core` never imports tauri: it publishes
            // every persisted event on a broadcast channel and this loop is the only fan-out.
            let mut session_events = state.core.sessions.subscribe();
            let session_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(envelope) = session_events.recv().await {
                    events::emit_session_event(&session_handle, envelope);
                }
            });

            // Watching is an enhancement: a missing/unwatchable project root must not stop the window
            // from opening. The panel still indexes on demand.
            let watcher_handle = app.handle().clone();
            match SpecWatcher::start(&state.core.project_root, move |change| {
                events::emit_spec_changed(&watcher_handle, change);
            }) {
                Ok(watcher) => state.keep_spec_watcher(watcher),
                Err(err) => tracing::warn!(
                    target: "grove::specgraph",
                    "spec watcher unavailable for {}: {err}",
                    state.core.project_root.display()
                ),
            }

            app.manage(state);
            Ok(())
        })
        .invoke_handler(commands::handlers())
        .run(tauri::generate_context!())
        .expect("failed to start Grove");
}
