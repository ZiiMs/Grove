//! provider credentials.
//!
//! A key is stored in the OS keychain through [`crate::secrets::SecretStore`] under
//! `provider.<id>.api_key`. Nothing else is durable: the settings document holds the provider id and
//! the model, never a secret, and there is no read-back path to the webview.
//!
//! An environment variable (`ANTHROPIC_API_KEY`, …) is honoured as a fallback so a developer who
//! already exports one does not have to store it twice. The keychain always wins, because that is the
//! one the user configured inside Grove.

use super::catalog;
use super::{AuthMethod, Detection, ProviderError};
use crate::secrets::{SecretStatus, SecretStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Secret name for a provider's API key. Only this *name* is ever durable.
pub fn api_key_name(provider: &str) -> String {
    format!("provider.{provider}.api_key")
}

/// Where a key came from, for the settings row. Never the key itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    Keychain,
    Environment,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            KeySource::Keychain => "keychain",
            KeySource::Environment => "environment",
        }
    }
}

/// What the settings pane shows for one provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub short: String,
    pub tint: String,
    pub method: AuthMethod,
    pub key_url: String,
    pub env_var: String,
    pub default_model: String,
    pub models: Vec<String>,
    /// `true` when a key is configured and readable.
    pub configured: bool,
    /// How the key was found, when one was.
    pub source: Option<KeySource>,
    /// Why a key is unusable, when it is: no key, or a locked keychain.
    pub detail: Option<String>,
}

/// Reads and writes provider keys. One instance per app, handed the same [`SecretStore`] as the rest
/// of the core.
#[derive(Clone)]
pub struct Credentials {
    secrets: Arc<dyn SecretStore>,
}

impl Credentials {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets }
    }

    /// Store a key. Surrounding whitespace is stripped (a pasted key usually carries a newline) and
    /// an empty value is rejected rather than stored as an empty secret.
    pub fn set(&self, provider: &str, key: &str) -> Result<(), ProviderError> {
        let spec = catalog::find(provider)
            .ok_or_else(|| ProviderError::Unavailable(format!("unknown provider: {provider}")))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(ProviderError::Unavailable(format!(
                "{} key is empty",
                spec.name
            )));
        }
        if key.chars().any(char::is_control) {
            return Err(ProviderError::Unavailable(format!(
                "{} key contains a control character",
                spec.name
            )));
        }
        self.secrets
            .set(&api_key_name(provider), key)
            .map_err(|error| ProviderError::Unavailable(error.to_string()))
    }

    /// Remove a stored key. Removing one that is not there is a no-op, so the UI can offer it on a
    /// row that is already empty.
    pub fn delete(&self, provider: &str) -> Result<(), ProviderError> {
        if catalog::find(provider).is_none() {
            return Err(ProviderError::Unavailable(format!(
                "unknown provider: {provider}"
            )));
        }
        self.secrets
            .delete(&api_key_name(provider))
            .map_err(|error| ProviderError::Unavailable(error.to_string()))
    }

    /// The key to send on the next request: the keychain first, then the vendor's environment
    /// variable.
    pub fn key(&self, provider: &str) -> Result<(String, KeySource), ProviderError> {
        let spec = catalog::find(provider)
            .ok_or_else(|| ProviderError::Unavailable(format!("unknown provider: {provider}")))?;
        match self.secrets.get(&api_key_name(provider)) {
            Ok(Some(key)) if !key.trim().is_empty() => {
                return Ok((key.trim().to_string(), KeySource::Keychain));
            }
            Ok(_) => {}
            Err(error) => {
                return Err(ProviderError::Unavailable(format!(
                    "{} keychain is unavailable: {error}",
                    spec.name
                )));
            }
        }
        match std::env::var(&spec.env_var) {
            Ok(key) if !key.trim().is_empty() => {
                Ok((key.trim().to_string(), KeySource::Environment))
            }
            _ => Err(ProviderError::Unavailable(format!(
                "{} needs an API key. Add one in Settings → Providers, or set {}.",
                spec.name, spec.env_var
            ))),
        }
    }

    /// Whether a key is configured, without handing the value out.
    pub fn source(&self, provider: &str) -> Result<Option<KeySource>, ProviderError> {
        let spec = catalog::find(provider)
            .ok_or_else(|| ProviderError::Unavailable(format!("unknown provider: {provider}")))?;
        match self.secrets.status(&api_key_name(provider)) {
            SecretStatus::Present => Ok(Some(KeySource::Keychain)),
            SecretStatus::Absent => {
                if std::env::var(&spec.env_var).is_ok_and(|value| !value.trim().is_empty()) {
                    Ok(Some(KeySource::Environment))
                } else {
                    Ok(None)
                }
            }
            SecretStatus::Unavailable => Err(ProviderError::Unavailable(format!(
                "{} keychain is unavailable",
                spec.name
            ))),
        }
    }

    /// The readiness row for one provider, as the settings pane and the doctor show it.
    pub fn status(&self, provider: &str) -> ProviderStatus {
        let spec = catalog::find(provider).expect("callers pass a catalogued provider");
        match self.source(provider) {
            Ok(Some(source)) => ProviderStatus {
                id: spec.id.clone(),
                name: spec.name.clone(),
                short: spec.short.clone(),
                tint: spec.tint.clone(),
                method: spec.auth,
                key_url: spec.key_url.clone(),
                env_var: spec.env_var.clone(),
                default_model: spec.default_model.clone(),
                models: spec.models.clone(),
                configured: true,
                source: Some(source),
                detail: None,
            },
            Ok(None) => ProviderStatus {
                id: spec.id.clone(),
                name: spec.name.clone(),
                short: spec.short.clone(),
                tint: spec.tint.clone(),
                method: spec.auth,
                key_url: spec.key_url.clone(),
                env_var: spec.env_var.clone(),
                default_model: spec.default_model.clone(),
                models: spec.models.clone(),
                configured: false,
                source: None,
                detail: Some(format!(
                    "Add an API key from {}, or set {}.",
                    spec.key_url, spec.env_var
                )),
            },
            Err(error) => ProviderStatus {
                id: spec.id.clone(),
                name: spec.name.clone(),
                short: spec.short.clone(),
                tint: spec.tint.clone(),
                method: spec.auth,
                key_url: spec.key_url.clone(),
                env_var: spec.env_var.clone(),
                default_model: spec.default_model.clone(),
                models: spec.models.clone(),
                configured: false,
                source: None,
                detail: Some(error.to_string()),
            },
        }
    }

    /// Every catalogued provider's readiness, in display order.
    pub fn statuses(&self) -> Vec<ProviderStatus> {
        catalog::builtin()
            .iter()
            .map(|spec| self.status(&spec.id))
            .collect()
    }

    /// The [`Detection`] for one provider, so the session manager can report readiness without
    /// knowing about secrets.
    pub fn detect(&self, provider: &str) -> Detection {
        let spec = match catalog::find(provider) {
            Some(spec) => spec,
            None => {
                return Detection::missing(
                    AuthMethod::ApiKey,
                    "",
                    format!("unknown provider: {provider}"),
                )
            }
        };
        match self.source(provider) {
            Ok(Some(_)) => Detection::ready(spec.auth, spec.default_model.clone()),
            Ok(None) => Detection::missing(
                spec.auth,
                spec.default_model.clone(),
                format!(
                    "No API key for {}. Add one in Settings → Providers, or set {}.",
                    spec.name, spec.env_var
                ),
            ),
            Err(error) => {
                Detection::missing(spec.auth, spec.default_model.clone(), error.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecretStore;

    fn credentials() -> (Credentials, Arc<MemorySecretStore>) {
        let secrets = Arc::new(MemorySecretStore::new());
        (Credentials::new(secrets.clone()), secrets)
    }

    #[test]
    fn set_trim_then_read_round_trips() {
        let (credentials, _secrets) = credentials();
        credentials.set("anthropic", "  sk-ant-123\n").unwrap();
        let (key, source) = credentials.key("anthropic").unwrap();
        assert_eq!(key, "sk-ant-123");
        assert_eq!(source, KeySource::Keychain);
    }

    #[test]
    fn an_empty_key_is_rejected_and_nothing_is_stored() {
        let (credentials, _secrets) = credentials();
        let error = credentials.set("anthropic", "   \n ").unwrap_err();
        assert!(error.to_string().contains("empty"), "{error}");
        assert_eq!(credentials.source("anthropic").unwrap(), None);
    }

    #[test]
    fn an_unknown_provider_is_refused_rather_than_writing_a_stray_secret() {
        let (credentials, secrets) = credentials();
        assert!(credentials.set("nope", "key").is_err());
        assert_eq!(secrets.status(&api_key_name("nope")), SecretStatus::Absent);
        assert!(credentials.key("nope").is_err());
    }

    #[test]
    fn a_missing_key_explains_both_ways_to_configure_it() {
        let (credentials, _secrets) = credentials();
        let error = credentials.key("openai").unwrap_err().to_string();
        assert!(error.contains("Settings → Providers"), "{error}");
        assert!(error.contains("OPENAI_API_KEY"), "{error}");
    }

    #[test]
    fn delete_is_idempotent() {
        let (credentials, _secrets) = credentials();
        credentials.set("google", "key").unwrap();
        credentials.delete("google").unwrap();
        credentials.delete("google").unwrap();
        assert_eq!(credentials.source("google").unwrap(), None);
    }

    #[test]
    fn status_reports_readiness_and_never_the_key() {
        let (credentials, _secrets) = credentials();
        let ready = credentials.status("anthropic");
        assert!(!ready.configured);
        assert!(ready.detail.unwrap().contains("console.anthropic.com"));

        credentials.set("anthropic", "sk-ant-secret-value").unwrap();
        let ready = credentials.status("anthropic");
        assert!(ready.configured);
        assert_eq!(ready.source, Some(KeySource::Keychain));
        let debug = format!("{ready:?}");
        assert!(
            !debug.contains("sk-ant-secret-value"),
            "the status view leaks no key"
        );
    }

    #[test]
    fn statuses_lists_every_shipped_provider_in_order() {
        let (credentials, _secrets) = credentials();
        let ids: Vec<String> = credentials
            .statuses()
            .into_iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(
            ids,
            vec!["anthropic", "openai", "google", "openrouter", "opencode-go"]
        );
    }

    #[test]
    fn detection_answers_what_a_launch_would_need() {
        let (credentials, _secrets) = credentials();
        assert!(!credentials.detect("openai").installed);
        credentials.set("openai", "sk-1").unwrap();
        let detection = credentials.detect("openai");
        assert!(detection.installed);
        assert_eq!(detection.default_model, "gpt-5");
        assert!(!credentials.detect("bogus").installed);
    }
}
