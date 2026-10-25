//! Skill catalog commands. The policy lives in `grove_core::skills`; these are argument adapters
//! plus the DTO error conversion.

use crate::state::AppState;
use grove_core::ipc::IpcResult;
use grove_core::skills::{
    CatalogSnapshot, InstallRecord, InstallRequest, InstallResult, SkillPreview, SkillTarget,
};
use std::path::{Path, PathBuf};

/// Browse the catalog. `refresh` bypasses a fresh disk cache; a failed fetch still serves the cache.
#[tauri::command]
pub async fn skill_catalog(
    state: tauri::State<'_, AppState>,
    refresh: Option<bool>,
) -> IpcResult<CatalogSnapshot> {
    Ok(state.core.skills.catalog(refresh.unwrap_or(false)).await?)
}

/// Frontmatter + body of one skill's `SKILL.md`.
#[tauri::command]
pub async fn skill_preview(
    state: tauri::State<'_, AppState>,
    source: String,
    path: String,
) -> IpcResult<SkillPreview> {
    Ok(state.core.skills.preview(&source, &path).await?)
}

/// Per-provider skill directories plus project-local `.agents/skills`, each with an `exists` flag.
///
/// An absent `projectRoot` means "the project Grove is looking at": the local target follows the
/// active session instead of resolving against the app's own directory.
#[tauri::command]
pub async fn skill_targets(
    state: tauri::State<'_, AppState>,
    project_root: Option<String>,
) -> IpcResult<Vec<SkillTarget>> {
    let project_root = project_root.as_deref().map(Path::new);
    Ok(state.core.skill_targets(project_root))
}

/// Copy a skill folder into the chosen target. Refuses traversal and symlink escapes in the core.
#[tauri::command]
pub async fn skill_install(
    state: tauri::State<'_, AppState>,
    source: String,
    path: String,
    target: String,
    project_root: Option<String>,
) -> IpcResult<InstallResult> {
    // Same default as `skill_targets`: without an explicit root the project-local destination is the
    // one the dropdown just showed.
    let project_root = project_root
        .map(PathBuf::from)
        .unwrap_or_else(|| state.core.project_root());
    let request = InstallRequest {
        source,
        path,
        target,
        project_root: Some(project_root),
    };
    Ok(state.core.skills.install(request).await?)
}

/// Every recorded installation, so the catalog can mark installed skills and offer removal.
#[tauri::command]
pub async fn skill_installs(state: tauri::State<'_, AppState>) -> IpcResult<Vec<InstallRecord>> {
    Ok(state.core.skills.installs()?)
}

/// Remove one installed skill folder by its recorded path.
#[tauri::command]
pub async fn skill_remove(state: tauri::State<'_, AppState>, path: String) -> IpcResult<()> {
    state.core.skills.remove_install(&path)?;
    Ok(())
}
