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
            let state = AppState::new(core);
            state.set_app_handle(app.handle().clone());

            // Forward the session bus to the webview. `grove-core` never imports tauri: it publishes
            // every persisted event on a broadcast channel and this loop is the only fan-out.
            let mut session_events = state.core.sessions.subscribe();
            let session_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(envelope) = session_events.recv().await {
                    events::emit_session_event(&session_handle, envelope);
                }
            });

            // Plans take the same route: `PlanService` announces an imported external edit on its
            // bus and the shell is the only place that knows about tauri events.
            let mut plan_changes = state.core.plan.subscribe();
            let plan_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(envelope) = plan_changes.recv().await {
                    events::emit_plan_changed(&plan_handle, envelope);
                }
            });

            // Watching is an enhancement: a missing/unwatchable project root must not stop the window
            // from opening. The panel still indexes on demand.
            //
            // The root is the startup one; `app_set_project_root` (and every session create/resume)
            // moves it and re-targets this watcher through the same helper.
            let project_root = state.core.project_root();
            commands::app::adopt(&state, project_root);

            app.manage(state);
            Ok(())
        })
        .invoke_handler(commands::handlers())
        .run(tauri::generate_context!())
        .expect("failed to start Grove");
}
