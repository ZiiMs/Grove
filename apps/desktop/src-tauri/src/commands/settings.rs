//! Settings, secrets and the harness doctor.
//!
//! Policy lives in `grove-core`; these handlers only adapt arguments and errors. Secret commands
//! expose set / status / delete — there is deliberately no command that returns a secret value, and no
//! handler logs one.

use crate::state::AppState;
use grove_core::harness_doctor::DoctorReport;
use grove_core::ipc::{IpcError, IpcResult};
use grove_core::secrets::SecretError;
use grove_core::settings::{SecretStatusView, Settings, SettingsPatch};
use grove_core::skills::SkillSource;

/// The whole settings document, with defaults and unknown keys preserved.
#[tauri::command]
pub async fn settings_get(state: tauri::State<'_, AppState>) -> IpcResult<Settings> {
    Ok(state.core.settings.get()?)
}

/// Apply a partial update atomically and return the resulting document.
#[tauri::command]
pub async fn settings_set(
    state: tauri::State<'_, AppState>,
    patch: SettingsPatch,
) -> IpcResult<Settings> {
    let next = state.core.settings.set(patch)?;
    // Keep the skill catalog in step with the saved sources; a failed refresh is the catalog's
    // problem, not a reason to reject the settings write.
    state.core.skills.set_sources(
        next.catalog_sources
            .iter()
            .cloned()
            .map(SkillSource::new)
            .collect(),
    );
    Ok(next)
}

/// Whether a secret is present. The keychain's answer (`absent` vs `unavailable`) is not an error:
/// a locked keychain still renders as a status chip.
#[tauri::command]
pub async fn secret_status(
    state: tauri::State<'_, AppState>,
    key: String,
) -> IpcResult<SecretStatusView> {
    let key = secret_key(key)?;
    Ok(SecretStatusView {
        status: state.core.secrets.status(&key),
        key,
    })
}

#[tauri::command]
pub async fn secret_set(
    state: tauri::State<'_, AppState>,
    key: String,
    value: String,
) -> IpcResult<SecretStatusView> {
    let key = secret_key(key)?;
    let value = value.trim();
    if value.is_empty() {
        return Err(IpcError::invalid("secret value must not be empty"));
    }
    state
        .core
        .secrets
        .set(&key, value)
        .map_err(secret_unavailable)?;
    Ok(SecretStatusView {
        status: state.core.secrets.status(&key),
        key,
    })
}

#[tauri::command]
pub async fn secret_delete(
    state: tauri::State<'_, AppState>,
    key: String,
) -> IpcResult<SecretStatusView> {
    let key = secret_key(key)?;
    state
        .core
        .secrets
        .delete(&key)
        .map_err(secret_unavailable)?;
    Ok(SecretStatusView {
        status: state.core.secrets.status(&key),
        key,
    })
}

/// Probe every harness and the `node`/`git`/`gh` tools. Never fails: a missing binary is a row.
#[tauri::command]
pub async fn doctor_report(state: tauri::State<'_, AppState>) -> IpcResult<DoctorReport> {
    Ok(state.core.doctor.report().await)
}

fn secret_key(key: String) -> IpcResult<String> {
    let key = key.trim().to_string();
    if key.is_empty() || key.chars().any(char::is_control) {
        return Err(IpcError::invalid("secret key must be a non-empty name"));
    }
    Ok(key)
}

fn secret_unavailable(error: SecretError) -> IpcError {
    // The message describes the store (locked, missing), never the value.
    IpcError::unavailable(error.to_string())
}
