use crate::state::AppState;
use grove_core::ipc::IpcResult;
use grove_core::specgraph::{read_spec, write_spec, SpecDoc, SpecGraph, SpecIndex};

/// Index the project root the core resolved at startup (`GROVE_PROJECT_ROOT`, else the process cwd).
///
/// The graph carries its root so the panel can show which project is indexed; reads and writes take
/// paths relative to it.
#[tauri::command]
pub async fn spec_index(state: tauri::State<'_, AppState>) -> IpcResult<SpecGraph> {
    Ok(SpecIndex::load(&state.core.project_root)?.graph())
}

/// Read one spec: frontmatter-derived node, body, and the hash a save must present.
#[tauri::command]
pub async fn spec_read(state: tauri::State<'_, AppState>, path: String) -> IpcResult<SpecDoc> {
    read_spec(&state.core.project_root, &path)
}

/// Save a spec body. `expectedHash` is the hash from the read the edit was based on; a mismatch is a
/// `conflict`, never a clobber. On success the returned doc carries the new hash.
#[tauri::command]
pub async fn spec_write(
    state: tauri::State<'_, AppState>,
    path: String,
    body: String,
    expected_hash: String,
) -> IpcResult<SpecDoc> {
    write_spec(&state.core.project_root, &path, &body, &expected_hash)
}
