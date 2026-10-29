//! Typed settings over the `settings` table.
//!
//! Each top-level key is its own JSON row, so an unknown key written by a newer Grove (or by hand)
//! survives a round-trip through this build. The known keys are validated here — never in the UI and
//! never in the command layer — and a patch is applied inside one SQLite transaction: either every
//! field lands or none does. Secrets never live here; they go through [`crate::secrets::SecretStore`].

use crate::ipc::IpcError;
use crate::secrets::SecretStatus;
use crate::store::{Store, StoreError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Catalog repositories a fresh install queries. Kept in step with `SkillCatalogConfig::default()`.
pub const DEFAULT_CATALOG_SOURCES: [&str; 2] = ["anthropics/skills", "mattpocock/skills"];

/// The keys this build understands. Everything else is preserved verbatim in [`Settings::extra`].
const KNOWN_KEYS: [&str; 4] = ["catalogSources", "clearTitleModel", "theme", "titleModel"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// The one global small model used to name sessions. It is not per provider: a session running on
/// Anthropic can be named by the cheapest model of whichever provider the user configured first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleModel {
    pub provider: String,
    pub model: String,
}

/// The settings document as the UI and the core exchange it. Pinned by
/// `contract/fixtures/settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `owner/name` GitHub repositories, in display order.
    pub catalog_sources: Vec<String>,
    pub theme: Theme,
    /// The explicit title model, or `None` when the user has not chosen one (or chose Off). A `None`
    /// here means "use the suggested model", not "never name a session".
    pub title_model: Option<TitleModel>,
    /// Unknown settings keys, preserved verbatim. Never secrets.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            catalog_sources: DEFAULT_CATALOG_SOURCES
                .iter()
                .map(|source| source.to_string())
                .collect(),
            theme: Theme::System,
            title_model: None,
            extra: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Apply a partial update in memory. Validation happens before anything is written, so a rejected
    /// patch leaves both these settings and the database untouched.
    pub fn apply(&mut self, patch: SettingsPatch) -> Result<(), SettingsError> {
        if let Some(sources) = patch.catalog_sources {
            let mut next: Vec<String> = Vec::new();
            for source in sources {
                let source = source.trim();
                if source.is_empty() {
                    continue;
                }
                validate_catalog_source(source)?;
                if !next.iter().any(|seen| seen == source) {
                    next.push(source.to_string());
                }
            }
            self.catalog_sources = next;
        }

        if let Some(theme) = patch.theme {
            self.theme = theme;
        }

        if patch.clear_title_model && patch.title_model.is_some() {
            return Err(SettingsError::Invalid(
                "a title model cannot be set and cleared in the same update".into(),
            ));
        }
        if patch.clear_title_model {
            self.title_model = None;
        } else if let Some(title_model) = patch.title_model {
            let title_model = TitleModel {
                provider: title_model.provider.trim().to_string(),
                model: title_model.model.trim().to_string(),
            };
            validate_key("title model provider", &title_model.provider)?;
            validate_key("title model", &title_model.model)?;
            self.title_model = Some(title_model);
        }

        for (key, value) in patch.extra {
            validate_key("setting key", &key)?;
            if KNOWN_KEYS.contains(&key.as_str()) {
                return Err(SettingsError::Invalid(format!(
                    "'{key}' is a typed setting; use its typed field"
                )));
            }
            self.extra.insert(key, value);
        }

        Ok(())
    }

    fn from_rows(rows: &BTreeMap<String, serde_json::Value>) -> Result<Self, SettingsError> {
        let document: serde_json::Map<String, serde_json::Value> = rows
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let settings: Settings = serde_json::from_value(serde_json::Value::Object(document))
            .map_err(|error| {
                SettingsError::Invalid(format!("stored settings are not valid: {error}"))
            })?;
        Ok(settings)
    }

    /// Replace the row map with this document. Unknown keys read earlier are in `extra`, so this
    /// never drops one.
    fn write_rows(&self, rows: &mut BTreeMap<String, serde_json::Value>) {
        rows.clear();
        rows.insert(
            "catalogSources".to_string(),
            serde_json::json!(self.catalog_sources),
        );
        rows.insert("theme".to_string(), serde_json::json!(self.theme));
        // An explicit null is what "Off" leaves behind; omitting the key would read as "never set"
        // and let a later build silently resurrect an old model.
        rows.insert(
            "titleModel".to_string(),
            serde_json::json!(self.title_model),
        );
        for (key, value) in &self.extra {
            rows.insert(key.clone(), value.clone());
        }
    }
}

/// A partial update. Omitted fields keep their stored value; unknown fields are written too, so the
/// UI can round-trip a settings document it does not fully understand.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    pub catalog_sources: Option<Vec<String>>,
    pub theme: Option<Theme>,
    /// A new explicit choice. `None` means "this update does not touch it": JSON null cannot be told
    /// apart from an omitted key, so clearing has its own flag.
    pub title_model: Option<TitleModel>,
    /// Clear the explicit choice back to the suggested model. The UI's Off option sends this.
    #[serde(default)]
    pub clear_title_model: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// The status of one secret, as the UI sees it. Never includes the value — there is no read-back.
/// Pinned by `contract/fixtures/secret-status.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretStatusView {
    pub key: String,
    pub status: SecretStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("invalid settings: {0}")]
    Invalid(String),
    #[error(transparent)]
    Store(StoreError),
}

impl From<StoreError> for SettingsError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Invalid(message) => SettingsError::Invalid(message),
            other => SettingsError::Store(other),
        }
    }
}

impl From<SettingsError> for IpcError {
    fn from(error: SettingsError) -> Self {
        match error {
            SettingsError::Invalid(message) => IpcError::invalid(message),
            SettingsError::Store(error) => IpcError::io(error.to_string()),
        }
    }
}

fn store_error(error: SettingsError) -> StoreError {
    match error {
        SettingsError::Invalid(message) => StoreError::Invalid(message),
        SettingsError::Store(error) => error,
    }
}

fn validate_key(label: &str, value: &str) -> Result<(), SettingsError> {
    if value.is_empty() {
        return Err(SettingsError::Invalid(format!("{label} must not be empty")));
    }
    if value.chars().any(char::is_control) {
        return Err(SettingsError::Invalid(format!(
            "{label} contains a control character"
        )));
    }
    Ok(())
}

fn validate_catalog_source(source: &str) -> Result<(), SettingsError> {
    let mut parts = source.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    let extra = parts.next();
    let segment_ok = |segment: &str| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if extra.is_some() || !segment_ok(owner) || !segment_ok(name) {
        return Err(SettingsError::Invalid(format!(
            "catalog source '{source}' is not an owner/name GitHub repository"
        )));
    }
    Ok(())
}

/// The small model a fresh install should use, or `None` when no provider has a credential.
///
/// The first configured provider in catalog order wins, so the answer does not depend on the order
/// the status rows arrived in. The `openrouter` case has to look at the live list: the preferred
/// route may not be offered by the account, and the catalog's static list is only a fallback.
pub fn suggest_title_model(
    statuses: &[crate::providers::credentials::ProviderStatus],
    live_models: &BTreeMap<String, Vec<String>>,
) -> Option<TitleModel> {
    let live = |provider: &str| -> &[String] {
        live_models
            .get(provider)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    for spec in crate::providers::catalog::builtin() {
        let configured = statuses
            .iter()
            .find(|status| status.id == spec.id)
            .is_some_and(|status| status.configured);
        if !configured {
            continue;
        }
        let model = match spec.id.as_str() {
            "anthropic" => "claude-haiku-4-5".to_string(),
            "openai" => "gpt-5-mini".to_string(),
            "google" => "gemini-2.5-flash".to_string(),
            "opencode-go" => "deepseek-v4.1-flash".to_string(),
            "openrouter" => {
                let preferred = "google/gemini-2.5-flash";
                let models = live(&spec.id);
                if models.iter().any(|model| model == preferred) {
                    preferred.to_string()
                } else {
                    models
                        .first()
                        .cloned()
                        .or_else(|| spec.models.first().cloned())
                        .unwrap_or_else(|| spec.default_model.clone())
                }
            }
            _ => spec.default_model.clone(),
        };
        return Some(TitleModel {
            provider: spec.id,
            model,
        });
    }
    None
}

/// The settings subsystem: validate a patch and persist the resulting document atomically.
pub struct SettingsService {
    store: Arc<Store>,
}

impl SettingsService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// The stored document, with defaults for missing keys and unknown keys preserved.
    pub fn get(&self) -> Result<Settings, SettingsError> {
        let rows = self.store.settings()?;
        Settings::from_rows(&rows)
    }

    /// Apply a patch inside one transaction and return the whole document afterwards.
    pub fn set(&self, patch: SettingsPatch) -> Result<Settings, SettingsError> {
        let saved = self.store.update_settings(|rows| {
            let mut next = Settings::from_rows(rows).map_err(store_error)?;
            next.apply(patch).map_err(store_error)?;
            next.write_rows(rows);
            Ok(next)
        })?;
        Ok(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::SkillCatalogConfig;
    use crate::store::Store;

    fn service() -> (SettingsService, Arc<Store>) {
        let store = Arc::new(Store::open_in_memory().unwrap());
        (SettingsService::new(Arc::clone(&store)), store)
    }

    fn patch(value: serde_json::Value) -> SettingsPatch {
        serde_json::from_value(value).expect("patch parses")
    }

    #[test]
    fn defaults_fill_in_every_known_field() {
        let (service, _store) = service();
        let current = service.get().unwrap();
        assert_eq!(current, Settings::default());
        assert_eq!(
            current.catalog_sources,
            DEFAULT_CATALOG_SOURCES.map(String::from).to_vec()
        );
        assert_eq!(current.theme, Theme::System);
    }

    #[test]
    fn defaults_match_the_skill_catalog_defaults() {
        let expected: Vec<String> = SkillCatalogConfig::default()
            .sources
            .iter()
            .map(|source| source.repo.clone())
            .collect();
        assert_eq!(Settings::default().catalog_sources, expected);
    }

    #[test]
    fn patch_round_trips_and_preserves_unknown_keys() {
        let (service, store) = service();
        let saved = service
            .set(patch(serde_json::json!({
                "catalogSources": ["acme/skills"],
                "theme": "dark",
                "futureFlag": { "enabled": true }
            })))
            .unwrap();
        assert_eq!(saved.theme, Theme::Dark);
        assert_eq!(
            saved.extra.get("futureFlag"),
            Some(&serde_json::json!({ "enabled": true }))
        );

        // A later patch that never mentions the unknown key must not drop it.
        let saved = service
            .set(SettingsPatch {
                theme: Some(Theme::Light),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            saved.extra.get("futureFlag"),
            Some(&serde_json::json!({ "enabled": true }))
        );
        assert_eq!(service.get().unwrap(), saved);

        // And it is a real row, so a future build can read it back.
        let rows = store.settings().unwrap();
        assert_eq!(
            rows.get("futureFlag"),
            Some(&serde_json::json!({ "enabled": true }))
        );
    }

    #[test]
    fn invalid_values_are_rejected_without_writing_anything() {
        let (service, _store) = service();
        let error = service
            .set(SettingsPatch {
                catalog_sources: Some(vec!["not-a-repository".to_string()]),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(error, SettingsError::Invalid(_)), "{error}");
        assert_eq!(
            service.get().unwrap(),
            Settings::default(),
            "an invalid patch is all-or-nothing"
        );
    }

    #[test]
    fn empty_sources_fall_back_to_none() {
        let (service, _store) = service();
        service
            .set(patch(serde_json::json!({
                "catalogSources": ["acme/skills"]
            })))
            .unwrap();
        let saved = service
            .set(patch(serde_json::json!({ "catalogSources": [] })))
            .unwrap();
        assert!(saved.catalog_sources.is_empty());
    }

    #[test]
    fn catalog_sources_are_validated() {
        let (service, _store) = service();
        for invalid in [
            serde_json::json!({ "catalogSources": ["owner/name/extra"] }),
            serde_json::json!({ "catalogSources": ["owner"] }),
            serde_json::json!({ "catalogSources": ["../escape"] }),
        ] {
            assert!(
                service.set(patch(invalid.clone())).is_err(),
                "expected {invalid} to be rejected"
            );
        }

        // An unknown theme never becomes a patch: it fails at the wire boundary.
        assert!(
            serde_json::from_value::<SettingsPatch>(serde_json::json!({ "theme": "neon" }))
                .is_err()
        );
        assert_eq!(service.get().unwrap(), Settings::default());
    }

    #[test]
    fn a_typed_key_cannot_be_smuggled_through_the_unknown_bag() {
        let (service, _store) = service();
        let mut extra = BTreeMap::new();
        extra.insert("theme".to_string(), serde_json::json!("neon"));
        let error = service
            .set(SettingsPatch {
                extra,
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(error, SettingsError::Invalid(_)));
    }

    #[test]
    fn stored_json_that_no_longer_matches_the_schema_is_invalid_not_a_panic() {
        let (service, store) = service();
        store
            .update_settings(|rows| {
                rows.insert("theme".to_string(), serde_json::json!("neon"));
                Ok(())
            })
            .unwrap();
        assert!(matches!(service.get(), Err(SettingsError::Invalid(_))));
    }

    #[test]
    fn a_title_model_round_trips_and_clears_explicitly() {
        let (service, store) = service();
        let model = TitleModel {
            provider: "anthropic".into(),
            model: "claude-haiku-4-5".into(),
        };
        let saved = service
            .set(patch(serde_json::json!({
                "titleModel": { "provider": "anthropic", "model": "claude-haiku-4-5" }
            })))
            .unwrap();
        assert_eq!(saved.title_model, Some(model.clone()));

        // "Off" is an explicit clear: the patch's typed `titleModel` cannot express null, so the
        // update carries `clearTitleModel` instead of quietly keeping the old model.
        let saved = service
            .set(patch(serde_json::json!({ "clearTitleModel": true })))
            .unwrap();
        assert_eq!(saved.title_model, None);
        assert_eq!(service.get().unwrap().title_model, None);
        assert_eq!(
            store.settings().unwrap().get("titleModel"),
            Some(&serde_json::Value::Null),
            "clearing writes an explicit null row, not the old model"
        );

        // A later patch that never mentions it leaves the choice cleared.
        let saved = service
            .set(SettingsPatch {
                theme: Some(Theme::Light),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(saved.title_model, None);
        assert_eq!(saved.theme, Theme::Light);
    }

    #[test]
    fn setting_and_clearing_a_title_model_at_once_is_rejected() {
        let (service, _store) = service();
        let error = service
            .set(patch(serde_json::json!({
                "titleModel": { "provider": "openai", "model": "gpt-5-mini" },
                "clearTitleModel": true
            })))
            .unwrap_err();
        assert!(matches!(error, SettingsError::Invalid(_)), "{error}");
        assert_eq!(service.get().unwrap(), Settings::default());
    }

    #[test]
    fn an_empty_title_model_is_rejected() {
        let (service, _store) = service();
        for invalid in [
            serde_json::json!({ "titleModel": { "provider": "", "model": "gpt-5-mini" } }),
            serde_json::json!({ "titleModel": { "provider": "openai", "model": "   " } }),
            serde_json::json!({ "titleModel": { "provider": "open\nai", "model": "gpt-5-mini" } }),
        ] {
            assert!(
                service.set(patch(invalid.clone())).is_err(),
                "expected {invalid} to be rejected"
            );
        }
        assert_eq!(service.get().unwrap(), Settings::default());
    }

    #[test]
    fn the_suggestion_is_the_first_configured_providers_small_model() {
        let statuses = vec![
            status("openai", true),
            status("anthropic", true),
            status("google", true),
        ];
        assert_eq!(
            suggest_title_model(&statuses, &BTreeMap::new()),
            Some(TitleModel {
                provider: "anthropic".into(),
                model: "claude-haiku-4-5".into()
            }),
            "catalog order decides, not the order the rows arrive in"
        );

        assert_eq!(
            suggest_title_model(&[status("openai", true)], &BTreeMap::new()),
            Some(TitleModel {
                provider: "openai".into(),
                model: "gpt-5-mini".into()
            })
        );
        assert_eq!(
            suggest_title_model(&[status("google", true)], &BTreeMap::new()),
            Some(TitleModel {
                provider: "google".into(),
                model: "gemini-2.5-flash".into()
            })
        );
        assert_eq!(
            suggest_title_model(&[status("opencode-go", true)], &BTreeMap::new()),
            Some(TitleModel {
                provider: "opencode-go".into(),
                model: "deepseek-v4.1-flash".into()
            })
        );
    }

    #[test]
    fn an_openrouter_suggestion_follows_the_live_model_list() {
        let mut live = BTreeMap::new();
        live.insert(
            "openrouter".to_string(),
            vec![
                "anthropic/claude-sonnet-4.5".to_string(),
                "google/gemini-2.5-flash".to_string(),
            ],
        );
        assert_eq!(
            suggest_title_model(&[status("openrouter", true)], &live),
            Some(TitleModel {
                provider: "openrouter".into(),
                model: "google/gemini-2.5-flash".into()
            })
        );

        let mut without = BTreeMap::new();
        without.insert(
            "openrouter".to_string(),
            vec!["z-ai/glm-5".to_string(), "openai/gpt-5".to_string()],
        );
        assert_eq!(
            suggest_title_model(&[status("openrouter", true)], &without),
            Some(TitleModel {
                provider: "openrouter".into(),
                model: "z-ai/glm-5".into()
            }),
            "the live list's first model when the preferred id is absent"
        );
    }

    #[test]
    fn no_configured_provider_suggests_nothing() {
        let statuses = vec![status("anthropic", false), status("openai", false)];
        assert_eq!(suggest_title_model(&statuses, &BTreeMap::new()), None);
        assert_eq!(suggest_title_model(&[], &BTreeMap::new()), None);
    }

    fn status(id: &str, configured: bool) -> crate::providers::credentials::ProviderStatus {
        let spec = crate::providers::catalog::find(id).expect("the provider ships");
        crate::providers::credentials::ProviderStatus {
            id: spec.id,
            name: spec.name,
            short: spec.short,
            tint: spec.tint,
            method: spec.auth,
            key_url: spec.key_url,
            env_var: spec.env_var,
            default_model: spec.default_model,
            models: spec.models,
            configured,
            source: None,
            detail: None,
        }
    }
}
