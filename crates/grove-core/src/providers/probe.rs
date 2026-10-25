//! Asking a provider which models the configured credential can actually reach.
//!
//! This is the "test connection" button: it costs one small request, and it proves the key works
//! better than a shape check could. The catalog's static list is the offline fallback the picker
//! starts from, so a failed probe degrades the choices instead of emptying them.

use super::wire::{api_error, authorize};
use super::{ProviderError, ProviderSpec, Wire};

/// The models the credential can see, in the vendor's own order.
pub async fn list_models(
    http: &reqwest::Client,
    spec: &ProviderSpec,
    key: &str,
) -> Result<Vec<String>, ProviderError> {
    let url = match spec.wire {
        Wire::Anthropic => format!(
            "{}/v1/models?limit=100",
            spec.base_url.trim_end_matches('/')
        ),
        Wire::OpenAi => format!("{}/models", spec.base_url.trim_end_matches('/')),
        Wire::Google => format!(
            "{}/v1beta/models?pageSize=200",
            spec.base_url.trim_end_matches('/')
        ),
    };
    let response = authorize(http.get(url), spec.wire, key)
        .send()
        .await
        .map_err(|error| ProviderError::Api(format!("cannot reach {}: {error}", spec.name)))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(api_error(status.as_u16(), &body));
    }
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| ProviderError::Protocol(format!("{} models: {error}", spec.name)))?;
    let models: Vec<String> = match spec.wire {
        Wire::Google => value
            .get("models")
            .and_then(|models| models.as_array())
            .map(|models| {
                models
                    .iter()
                    .filter_map(|model| model.get("name").and_then(|name| name.as_str()))
                    // Google names them `models/gemini-2.5-pro`; the request takes the suffix.
                    .map(|name| name.trim_start_matches("models/").to_string())
                    .collect()
            })
            .unwrap_or_default(),
        _ => value
            .get("data")
            .and_then(|data| data.as_array())
            .map(|data| {
                data.iter()
                    .filter_map(|model| model.get("id").and_then(|id| id.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    };
    if models.is_empty() {
        return Err(ProviderError::Protocol(format!(
            "{} returned no models",
            spec.name
        )));
    }
    let mut models: Vec<String> = models;
    models.sort();
    models.dedup();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anthropic() -> ProviderSpec {
        crate::providers::catalog::find("anthropic").unwrap()
    }

    #[tokio::test]
    async fn a_rejected_key_is_reported_with_the_settings_hint() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(401)
                    .set_body_string(r#"{"error":{"message":"invalid x-api-key"}}"#),
            )
            .mount(&server)
            .await;
        let mut spec = anthropic();
        spec.base_url = server.uri();

        let error = list_models(&reqwest::Client::new(), &spec, "bad")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("Settings → Providers"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn the_models_are_read_sorted_and_deduplicated() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(r#"{"data":[{"id":"b"},{"id":"a"},{"id":"a"}]}"#),
            )
            .mount(&server)
            .await;
        let mut spec = anthropic();
        spec.base_url = server.uri();

        let models = list_models(&reqwest::Client::new(), &spec, "key")
            .await
            .unwrap();
        assert_eq!(models, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn google_model_names_lose_their_prefix() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1beta/models"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(r#"{"models":[{"name":"models/gemini-2.5-pro"}]}"#),
            )
            .mount(&server)
            .await;
        let mut spec = crate::providers::catalog::find("google").unwrap();
        spec.base_url = server.uri();

        let models = list_models(&reqwest::Client::new(), &spec, "key")
            .await
            .unwrap();
        assert_eq!(models, vec!["gemini-2.5-pro"]);
    }
}
