//! Settings, secrets and the provider doctor.
//!
//! Policy lives in `grove-core`; these handlers only adapt arguments and errors. Secret commands
//! expose set / status / delete — there is deliberately no command that returns a secret value, and no
//! handler logs one.

use crate::state::AppState;
use grove_core::ipc::{IpcError, IpcResult, PlanUsage, ProviderStatus};
use grove_core::providers::ProviderError;
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

/// Every provider's readiness: whether a key is configured, and which model a session would use.
#[tauri::command]
pub async fn provider_statuses(
    state: tauri::State<'_, AppState>,
) -> IpcResult<Vec<ProviderStatus>> {
    Ok(state.core.credentials.statuses())
}

/// The provider's remaining plan usage, or `null` when that provider has no plan endpoint.
#[tauri::command]
pub async fn plan_usage(
    state: tauri::State<'_, AppState>,
    provider: String,
) -> IpcResult<Option<PlanUsage>> {
    Ok(state.core.sessions.plan_usage(&provider).await?)
}

/// Store an API key for one provider. Returns the refreshed row; the value never comes back.
#[tauri::command]
pub async fn provider_key_set(
    state: tauri::State<'_, AppState>,
    provider: String,
    key: String,
) -> IpcResult<ProviderStatus> {
    state
        .core
        .credentials
        .set(&provider, &key)
        .map_err(provider_error)?;
    Ok(state.core.credentials.status(&provider))
}

/// Remove a stored key. The environment fallback, if any, then applies again.
#[tauri::command]
pub async fn provider_key_delete(
    state: tauri::State<'_, AppState>,
    provider: String,
) -> IpcResult<ProviderStatus> {
    state
        .core
        .credentials
        .delete(&provider)
        .map_err(provider_error)?;
    Ok(state.core.credentials.status(&provider))
}

/// The models the configured key can reach. This is the "test connection" button: it proves the key
/// by using it, and the picker can then offer what the account actually has.
#[tauri::command]
pub async fn provider_models(
    state: tauri::State<'_, AppState>,
    provider: String,
) -> IpcResult<Vec<String>> {
    state
        .core
        .provider_models(&provider)
        .await
        .map_err(provider_error)
}

/// The reasoning levels each provider model advertises. Cached by the core; an unavailable catalogue
/// returns an empty map so the composer keeps its static fallback.
#[tauri::command]
pub async fn provider_thinking(
    state: tauri::State<'_, AppState>,
) -> IpcResult<grove_core::providers::model_catalog::ThinkingMap> {
    Ok(state.core.provider_thinking().await)
}

/// The small model that should name sessions when the user has not chosen one.
///
/// The caller passes the live model lists it already holds (`provider_models` refreshed them); the
/// OpenRouter suggestion depends on whether the account offers the preferred route. Nothing here
/// persists the suggestion: Settings shows it and a save writes the choice.
#[tauri::command]
pub fn title_model_suggest(
    state: tauri::State<'_, AppState>,
    live_models: Option<std::collections::BTreeMap<String, Vec<String>>>,
) -> IpcResult<Option<grove_core::settings::TitleModel>> {
    Ok(grove_core::settings::suggest_title_model(
        &state.core.credentials.statuses(),
        &live_models.unwrap_or_default(),
    ))
}

fn provider_error(error: ProviderError) -> IpcError {
    match error {
        ProviderError::Unavailable(message) => IpcError::unavailable(message),
        other => IpcError::invalid(other.to_string()),
    }
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
