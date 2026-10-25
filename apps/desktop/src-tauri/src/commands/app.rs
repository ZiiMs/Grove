use crate::events;
use crate::state::AppState;
use grove_core::ipc::{AppInfo, IpcError, IpcResult};
use grove_core::paths::display_path;
use grove_core::specgraph::{SpecChange, SpecWatcher};
use std::path::Path;
use tauri::Emitter;

/// Header payload for the shell. Contract: `contract/fixtures/app-info.json`.
#[tauri::command]
pub async fn app_info(state: tauri::State<'_, AppState>) -> IpcResult<AppInfo> {
    Ok(state.core.info())
}

/// Point Grove at the project the user is actually working in.
///
/// A session's `cwd` decides it, so the shell calls this when a session is created, resumed or its tab
/// is activated; the webview may also call it directly. Every project-scoped subsystem reads the same
/// [`grove_core::paths::ProjectRoot`], so one call moves the spec graph, the plan mirror's fallback,
/// MCP discovery and the project-local skills target together. Returns the root now in force.
///
/// `GROVE_PROJECT_ROOT` stays the *initial* value for headless runs and tests; this is what moves it
/// afterwards.
#[tauri::command]
pub async fn app_set_project_root(
    state: tauri::State<'_, AppState>,
    path: String,
) -> IpcResult<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(IpcError::invalid("a project root is required"));
    }
    Ok(adopt(&state, path))
}

/// Move the project root and everything that watches it. Shared by the command above and the session
/// commands, so creating a session and activating its tab cannot disagree.
pub(crate) fn adopt(state: &AppState, path: impl AsRef<Path>) -> String {
    let requested = path.as_ref();
    // A restored session can point at a worktree that has since been deleted. Moving the project
    // root there would break the spec graph and plan mirror for every panel, so keep the root that
    // works and let the session open with its transcript.
    if !requested.is_dir() {
        tracing::warn!(
            target: "grove::specgraph",
            "ignoring missing project root: {}",
            requested.display()
        );
        return display_path(&state.core.project_root());
    }
    let previous = state.core.project_root();
    let root = state.core.set_project_root(requested);
    let Some(app) = state.app_handle() else {
        // No window yet (a headless construction): the core already moved, nothing to tell.
        return display_path(&root);
    };
    if root == previous && state.has_spec_watcher() {
        // Re-adopting the project already in force (a resume, a tab clicked twice) is not a move:
        // the watcher and the panels' index stay as they are.
        return display_path(&root);
    }
    // The old watcher watched the old project; replace it (dropping the previous one stops it) so
    // `grove://spec-changed` keeps meaning "the project you are looking at changed".
    let handle = app.clone();
    match SpecWatcher::start(&root, move |change| {
        events::emit_spec_changed(&handle, change)
    }) {
        Ok(watcher) => state.keep_spec_watcher(watcher),
        Err(err) => tracing::warn!(
            target: "grove::specgraph",
            "spec watcher unavailable for {}: {err}",
            root.display()
        ),
    }
    // The panels hold an index of the previous project. This is the same event a file change emits,
    // so the Specs panel re-indexes through the path it already has.
    let _ = app.emit(
        events::SPEC_CHANGED,
        grove_core::ipc::EventEnvelope::global(SpecChange { paths: Vec::new() }),
    );
    display_path(&root)
}
