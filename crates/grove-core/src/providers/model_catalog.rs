//! Per-model reasoning capabilities from the public models.dev catalogue.
//!
//! A provider's `/models` endpoint answers *which* models exist, not *what each one can do*: OpenCode
//! Go returns ids only, and the native Anthropic / OpenAI / Google lists carry no reasoning metadata.
//! models.dev is the one public catalogue that records each provider's `reasoning_options` (effort
//! values, a toggle, or token budgets). We cache the extracted provider → model → levels map so the
//! 5.9 MB source document is fetched at most once an hour, and reuse the cached map on any failure.
//!
//! This is deliberately metadata-only: a model missing from the catalogue, or a failed fetch, leaves
//! the caller with its static fallback rather than hiding the model or inventing tiers.

use super::ThinkingLevel;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The public machine-readable catalogue used by OpenCode and other clients.
pub const CATALOG_URL: &str = "https://models.dev/api.json";
/// Re-check the catalogue at most once an hour. Conditional requests make the check cheap when the
/// document has not changed; the extracted map is tiny, so the cache file stays small too.
const CACHE_TTL_SECS: u64 = 60 * 60;
const FETCH_TIMEOUT_SECS: u64 = 30;
/// The providers Grove ships with. A gateway can add another provider later; until then it falls
/// back to the static tier list.
const PROVIDERS: [&str; 5] = ["opencode-go", "anthropic", "openai", "google", "openrouter"];
/// What a reasoning-capable model with no usable option metadata gets. This preserves the old
/// behaviour instead of removing the control from a model we cannot describe.
const FALLBACK_LEVELS: [ThinkingLevel; 4] = [
    ThinkingLevel::Off,
    ThinkingLevel::Low,
    ThinkingLevel::Medium,
    ThinkingLevel::High,
];

/// Provider id → model id → the levels that model accepts, in display order.
pub type ThinkingMap = BTreeMap<String, BTreeMap<String, Vec<ThinkingLevel>>>;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    etag: Option<String>,
    fetched_at: u64,
    providers: ThinkingMap,
}

enum Fetched {
    Updated {
        providers: ThinkingMap,
        etag: Option<String>,
    },
    Unchanged,
}

/// The model-specific levels, from the cache when it is warm and from models.dev when it is not.
///
/// Never fails: an unreachable catalogue degrades to the cached map, then to an empty map so the UI
/// keeps its static fallback.
pub async fn thinking_options(http: &reqwest::Client, cache_path: &Path) -> ThinkingMap {
    thinking_options_from(http, cache_path, CATALOG_URL).await
}

async fn thinking_options_from(
    http: &reqwest::Client,
    cache_path: &Path,
    url: &str,
) -> ThinkingMap {
    let cached = read_cache(cache_path);
    if let Some(cache) = &cached {
        if now_secs().saturating_sub(cache.fetched_at) < CACHE_TTL_SECS {
            return cache.providers.clone();
        }
    }

    match fetch(http, cached.as_ref(), url).await {
        Ok(Fetched::Updated { providers, etag }) => {
            write_cache(
                cache_path,
                &Cache {
                    etag,
                    fetched_at: now_secs(),
                    providers: providers.clone(),
                },
            );
            providers
        }
        Ok(Fetched::Unchanged) => match cached {
            Some(cache) => {
                let refreshed = Cache {
                    fetched_at: now_secs(),
                    ..cache
                };
                write_cache(cache_path, &refreshed);
                refreshed.providers
            }
            None => BTreeMap::new(),
        },
        Err(error) => {
            tracing::warn!(%error, "model capability catalogue unavailable; using cache");
            cached.map(|cache| cache.providers).unwrap_or_default()
        }
    }
}

async fn fetch(
    http: &reqwest::Client,
    cached: Option<&Cache>,
    url: &str,
) -> Result<Fetched, String> {
    let mut request = http.get(url);
    if let Some(etag) = cached.and_then(|cache| cache.etag.as_deref()) {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    let response = tokio::time::timeout(Duration::from_secs(FETCH_TIMEOUT_SECS), request.send())
        .await
        .map_err(|_| "models.dev request timed out".to_string())?
        .map_err(|error| format!("cannot reach models.dev: {error}"))?;

    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Fetched::Unchanged);
    }
    if !response.status().is_success() {
        return Err(format!("models.dev returned {}", response.status()));
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = tokio::time::timeout(Duration::from_secs(FETCH_TIMEOUT_SECS), response.text())
        .await
        .map_err(|_| "models.dev response timed out".to_string())?
        .map_err(|error| format!("cannot read models.dev response: {error}"))?;
    let value: Value =
        serde_json::from_str(&body).map_err(|error| format!("models.dev JSON: {error}"))?;
    let providers = parse_catalog(&value);
    if providers.is_empty() {
        return Err("models.dev returned no announced provider models".to_string());
    }
    Ok(Fetched::Updated { providers, etag })
}

/// Extract only the providers Grove can run. Unknown providers are ignored so a future catalogue
/// entry cannot change behaviour here before a provider client exists.
fn parse_catalog(value: &Value) -> ThinkingMap {
    let mut result = ThinkingMap::new();
    for provider in PROVIDERS {
        let Some(models) = value
            .get(provider)
            .and_then(|provider| provider.get("models"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let mut levels_by_model = BTreeMap::new();
        for (model_id, model) in models {
            levels_by_model.insert(model_id.clone(), model_levels(model));
        }
        if !levels_by_model.is_empty() {
            result.insert(provider.to_string(), levels_by_model);
        }
    }
    result
}

/// The levels a single models.dev model entry advertises.
///
/// `effort` values are the useful tiers. `toggle` adds Off and High (an on/off model has no finer
/// ladder), and `budget_tokens` adds the three budgets the wire encoders already map. Some models
/// advertise several option kinds at once, so the sets are merged and sorted by [`ThinkingLevel`]'s
/// declaration order rather than trusting the catalogue's ordering.
///
/// A budget model whose minimum is zero can be disabled explicitly, so it offers `None` instead of
/// `Off`: `Off` omits the vendor field, which leaves a model that reasons by default still reasoning,
/// while `None` is the zero budget the encoders send. A positive minimum keeps `Off`, since the
/// vendor has no off state to request.
fn model_levels(model: &Value) -> Vec<ThinkingLevel> {
    let reasoning = model
        .get("reasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut levels = BTreeSet::new();
    for option in model
        .get("reasoning_options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match option.get("type").and_then(Value::as_str) {
            Some("effort") => {
                for value in option
                    .get("values")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if let Some(level) = effort_level(value) {
                        levels.insert(level);
                    }
                }
            }
            Some("toggle") => {
                levels.insert(ThinkingLevel::Off);
                levels.insert(ThinkingLevel::High);
            }
            Some("budget_tokens") => {
                // A zero minimum is the vendor saying "budget 0 is a valid off switch". Omitting the
                // field instead (what `Off` does) leaves a default-on model reasoning.
                let can_turn_off = option.get("min").and_then(Value::as_f64) == Some(0.0);
                levels.insert(if can_turn_off {
                    ThinkingLevel::None
                } else {
                    ThinkingLevel::Off
                });
                levels.insert(ThinkingLevel::Low);
                levels.insert(ThinkingLevel::Medium);
                levels.insert(ThinkingLevel::High);
            }
            _ => {}
        }
    }
    if levels.is_empty() {
        return if reasoning {
            FALLBACK_LEVELS.to_vec()
        } else {
            vec![ThinkingLevel::Off]
        };
    }
    // A model that advertises both an explicit `none` effort and a toggle/budget would otherwise
    // show two options labelled Off. The explicit value is the more precise one; keep it alone.
    if levels.contains(&ThinkingLevel::None) {
        levels.remove(&ThinkingLevel::Off);
    }
    levels.into_iter().collect()
}

/// models.dev spells the explicit off state `none`; it is distinct from Grove's omit-the-field
/// `off` because OpenAI-compatible providers may default to reasoning when the field is absent.
fn effort_level(value: &str) -> Option<ThinkingLevel> {
    match value {
        "none" => Some(ThinkingLevel::None),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" => Some(ThinkingLevel::XHigh),
        "max" => Some(ThinkingLevel::Max),
        _ => None,
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn read_cache(path: &Path) -> Option<Cache> {
    let body = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&body).ok()
}

fn write_cache(path: &Path, cache: &Cache) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(parent) {
        tracing::warn!(%error, "cannot create model capability cache directory");
        return;
    }
    match serde_json::to_vec(cache) {
        Ok(body) => {
            if let Err(error) = std::fs::write(path, body) {
                tracing::warn!(%error, "cannot write model capability cache");
            }
        }
        Err(error) => tracing::warn!(%error, "cannot encode model capability cache"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn levels(map: &ThinkingMap, provider: &str, model: &str) -> Vec<ThinkingLevel> {
        map.get(provider)
            .and_then(|models| models.get(model))
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn effort_values_become_model_specific_levels() {
        let value = json!({
            "opencode-go": {
                "models": {
                    "kimi-k3": {
                        "reasoning": true,
                        "reasoning_options": [{ "type": "effort", "values": ["max"] }]
                    },
                    "gpt-6-luna": {
                        "reasoning": true,
                        "reasoning_options": [{
                            "type": "effort",
                            "values": ["none", "low", "medium", "high", "xhigh", "max"]
                        }]
                    }
                }
            }
        });

        let map = parse_catalog(&value);
        assert_eq!(
            levels(&map, "opencode-go", "kimi-k3"),
            vec![ThinkingLevel::Max]
        );
        assert_eq!(
            levels(&map, "opencode-go", "gpt-6-luna"),
            vec![
                ThinkingLevel::None,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh,
                ThinkingLevel::Max,
            ]
        );
    }

    #[test]
    fn toggle_and_budget_models_get_a_usable_control() {
        let value = json!({
            "opencode-go": {
                "models": {
                    "toggle-only": {
                        "reasoning": true,
                        "reasoning_options": [{ "type": "toggle" }]
                    },
                    "toggle-plus-effort": {
                        "reasoning": true,
                        "reasoning_options": [
                            { "type": "toggle" },
                            { "type": "effort", "values": ["low", "xhigh"] },
                            { "type": "budget_tokens", "min": 1024 }
                        ]
                    }
                }
            }
        });

        let map = parse_catalog(&value);
        assert_eq!(
            levels(&map, "opencode-go", "toggle-only"),
            vec![ThinkingLevel::Off, ThinkingLevel::High]
        );
        assert_eq!(
            levels(&map, "opencode-go", "toggle-plus-effort"),
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh,
            ]
        );
    }

    #[test]
    fn a_budget_model_with_a_zero_minimum_offers_an_explicit_off() {
        let value = json!({
            "google": {
                "models": {
                    "can-turn-off": {
                        "reasoning": true,
                        "reasoning_options": [{ "type": "budget_tokens", "min": 0, "max": 24576 }]
                    },
                    "has-a-floor": {
                        "reasoning": true,
                        "reasoning_options": [{ "type": "budget_tokens", "min": 128 }]
                    },
                    "unknown-floor": {
                        "reasoning": true,
                        "reasoning_options": [{ "type": "budget_tokens" }]
                    }
                }
            }
        });

        let map = parse_catalog(&value);
        // `none` is what the Google encoder turns into a zero budget; `off` would omit the config
        // and let a default-on model keep reasoning.
        assert_eq!(
            levels(&map, "google", "can-turn-off"),
            vec![
                ThinkingLevel::None,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ]
        );
        assert_eq!(
            levels(&map, "google", "has-a-floor"),
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ]
        );
        assert_eq!(
            levels(&map, "google", "unknown-floor"),
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ]
        );
    }

    #[test]
    fn a_reasoning_model_without_options_keeps_the_static_fallback() {
        let value = json!({
            "opencode-go": {
                "models": {
                    "no-options": { "reasoning": true, "reasoning_options": [] },
                    "not-reasoning": { "reasoning": false }
                }
            }
        });

        let map = parse_catalog(&value);
        assert_eq!(
            levels(&map, "opencode-go", "no-options"),
            FALLBACK_LEVELS.to_vec()
        );
        assert_eq!(
            levels(&map, "opencode-go", "not-reasoning"),
            vec![ThinkingLevel::Off]
        );
    }

    #[test]
    fn providers_outside_the_runtime_are_ignored() {
        let value = json!({
            "some-future-gateway": {
                "models": {
                    "mystery": {
                        "reasoning": true,
                        "reasoning_options": [{ "type": "effort", "values": ["max"] }]
                    }
                }
            }
        });

        assert!(parse_catalog(&value).is_empty());
    }

    fn catalogue_body() -> String {
        json!({
            "opencode-go": {
                "models": {
                    "kimi-k3": {
                        "reasoning": true,
                        "reasoning_options": [{ "type": "effort", "values": ["max"] }]
                    }
                }
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn a_fresh_catalogue_is_cached_and_not_requested_twice() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/catalog"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("etag", "\"v1\"")
                    .set_body_string(catalogue_body()),
            )
            .expect(1)
            .mount(&server)
            .await;
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("thinking.json");
        let url = format!("{}/catalog", server.uri());

        let first = thinking_options_from(&reqwest::Client::new(), &cache, &url).await;
        let second = thinking_options_from(&reqwest::Client::new(), &cache, &url).await;

        assert_eq!(first, second);
        assert_eq!(
            levels(&first, "opencode-go", "kimi-k3"),
            vec![ThinkingLevel::Max]
        );
        assert!(cache.exists());
        server.verify().await;
    }

    #[tokio::test]
    async fn an_unchanged_catalogue_reuses_the_cached_map_after_a_304() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/catalog"))
            .and(wiremock::matchers::header("if-none-match", "\"v1\""))
            .respond_with(wiremock::ResponseTemplate::new(304))
            .expect(1)
            .mount(&server)
            .await;
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("thinking.json");
        let cached = parse_catalog(&serde_json::from_str(&catalogue_body()).unwrap());
        write_cache(
            &cache,
            &Cache {
                etag: Some("\"v1\"".to_string()),
                fetched_at: 0,
                providers: cached.clone(),
            },
        );
        let url = format!("{}/catalog", server.uri());

        let result = thinking_options_from(&reqwest::Client::new(), &cache, &url).await;

        assert_eq!(result, cached);
        server.verify().await;
    }
}
