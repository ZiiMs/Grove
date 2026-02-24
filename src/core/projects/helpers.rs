use anyhow::{bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::cache::Cache;

pub fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

pub fn extract_id_from_url(input: &str, domain: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.contains(domain) {
        return trimmed
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .map(|s| s.to_string());
    }
    None
}

pub fn parse_service_id(input: &str, domain: &str) -> String {
    extract_id_from_url(input, domain).unwrap_or_else(|| input.trim().to_string())
}

pub enum AuthType {
    Bearer,
    Token,
    PrivateToken,
}

pub fn create_authenticated_client(
    auth_type: AuthType,
    token: &str,
    extra_headers: Option<HeaderMap>,
) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();

    let auth_value = match auth_type {
        AuthType::Bearer => format!("Bearer {}", token),
        AuthType::Token => format!("{} ", token),
        AuthType::PrivateToken => token.to_string(),
    };

    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&auth_value).context("Invalid token")?,
    );

    if let Some(extra) = extra_headers {
        headers.extend(extra);
    }

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("Failed to create HTTP client")
}

pub async fn http_get<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    service_name: &str,
) -> Result<T> {
    tracing::debug!("{} GET: url={}", service_name, url);

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch from {}", service_name))?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    tracing::debug!(
        "{} response: status={}, body={}",
        service_name,
        status,
        response_text
    );

    if !status.is_success() {
        tracing::error!("{} API error: {} - {}", service_name, status, response_text);
        bail!("{} API error: {} - {}", service_name, status, response_text);
    }

    serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to parse {} response", service_name))
}

pub async fn http_get_with_query<T: DeserializeOwned, Q: Serialize>(
    client: &reqwest::Client,
    url: &str,
    query: &Q,
    service_name: &str,
) -> Result<T> {
    tracing::debug!("{} GET: url={}", service_name, url);

    let response = client
        .get(url)
        .query(query)
        .send()
        .await
        .with_context(|| format!("Failed to fetch from {}", service_name))?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    tracing::debug!(
        "{} response: status={}, body={}",
        service_name,
        status,
        response_text
    );

    if !status.is_success() {
        tracing::error!("{} API error: {} - {}", service_name, status, response_text);
        bail!("{} API error: {} - {}", service_name, status, response_text);
    }

    serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to parse {} response", service_name))
}

pub async fn http_post<B: Serialize>(
    client: &reqwest::Client,
    url: &str,
    body: &B,
    service_name: &str,
) -> Result<()> {
    tracing::debug!("{} POST: url={}", service_name, url);

    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("Failed to post to {}", service_name))?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    tracing::debug!(
        "{} response: status={}, body={}",
        service_name,
        status,
        response_text
    );

    if !status.is_success() {
        tracing::error!("{} API error: {} - {}", service_name, status, response_text);
        bail!("{} API error: {} - {}", service_name, status, response_text);
    }

    Ok(())
}

pub async fn http_post_response<T: DeserializeOwned, B: Serialize>(
    client: &reqwest::Client,
    url: &str,
    body: &B,
    service_name: &str,
) -> Result<T> {
    tracing::debug!("{} POST: url={}", service_name, url);

    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("Failed to post to {}", service_name))?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    tracing::debug!(
        "{} response: status={}, body={}",
        service_name,
        status,
        response_text
    );

    if !status.is_success() {
        tracing::error!("{} API error: {} - {}", service_name, status, response_text);
        bail!("{} API error: {} - {}", service_name, status, response_text);
    }

    serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to parse {} response", service_name))
}

pub async fn http_put<B: Serialize>(
    client: &reqwest::Client,
    url: &str,
    body: &B,
    service_name: &str,
) -> Result<()> {
    tracing::debug!("{} PUT: url={}", service_name, url);

    let response = client
        .put(url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("Failed to put to {}", service_name))?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    tracing::debug!(
        "{} response: status={}, body={}",
        service_name,
        status,
        response_text
    );

    if !status.is_success() {
        tracing::error!("{} API error: {} - {}", service_name, status, response_text);
        bail!("{} API error: {} - {}", service_name, status, response_text);
    }

    Ok(())
}

pub struct OptionalClient<T, C: Clone> {
    client: RwLock<Option<T>>,
    cache: Cache<C>,
    service_name: &'static str,
}

impl<T, C: Clone + Send> OptionalClient<T, C> {
    pub fn new<F>(factory: F, cache_ttl_secs: u64, service_name: &'static str) -> Self
    where
        F: FnOnce() -> Option<T>,
    {
        Self {
            client: RwLock::new(factory()),
            cache: Cache::new(cache_ttl_secs),
            service_name,
        }
    }

    pub fn reconfigure<F>(&self, factory: F)
    where
        F: FnOnce() -> Option<T>,
    {
        if let Ok(mut guard) = self.client.try_write() {
            *guard = factory();
        }
    }

    pub async fn is_configured(&self) -> bool {
        self.client.read().await.is_some()
    }

    pub async fn with_client<F, R, Fut>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&T) -> Fut,
        Fut: std::future::Future<Output = Result<R>>,
    {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => f(c).await,
            None => bail!("{} not configured", self.service_name),
        }
    }

    pub async fn with_client_mut_cache<F, R, Fut>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&T) -> Fut,
        Fut: std::future::Future<Output = Result<R>>,
    {
        let guard = self.client.read().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("{} not configured", self.service_name))?;
        let result = f(client).await;

        if result.is_ok() {
            self.cache.invalidate().await;
            tracing::debug!("{} cache invalidated after mutation", self.service_name);
        }

        result
    }

    pub async fn get_cached(&self) -> Option<C> {
        self.cache.get().await
    }

    pub async fn set_cache(&self, data: C) {
        self.cache.set(data).await;
    }

    pub async fn invalidate_cache(&self) {
        self.cache.invalidate().await;
        tracing::debug!("{} cache manually invalidated", self.service_name);
    }

    pub async fn get_or_fetch<F, Fut>(&self, fetcher: F) -> Result<C>
    where
        F: FnOnce(&T) -> Fut,
        Fut: std::future::Future<Output = Result<C>>,
    {
        if let Some(cached) = self.cache.get().await {
            tracing::debug!("{} cache hit", self.service_name);
            return Ok(cached);
        }

        let guard = self.client.read().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("{} not configured", self.service_name))?;
        let data = fetcher(client).await?;

        tracing::debug!("{} cache miss: fetched data", self.service_name);
        self.cache.set(data.clone()).await;

        Ok(data)
    }
}

pub fn find_status_by_terms(statuses: &[String], terms: &[&str]) -> Option<String> {
    for status in statuses {
        let lower = status.to_lowercase();
        for term in terms {
            if lower.contains(term) {
                return Some(status.clone());
            }
        }
    }
    None
}

pub fn find_in_progress_status(
    statuses: &[String],
    override_value: Option<&str>,
) -> Option<String> {
    override_value
        .map(|s| s.to_string())
        .or_else(|| find_status_by_terms(statuses, &["in progress", "doing", "in review"]))
}

pub fn find_done_status(statuses: &[String], override_value: Option<&str>) -> Option<String> {
    override_value
        .map(|s| s.to_string())
        .or_else(|| find_status_by_terms(statuses, &["done", "complete", "closed"]))
}

pub fn find_not_started_status(
    statuses: &[String],
    override_value: Option<&str>,
) -> Option<String> {
    override_value.map(|s| s.to_string()).or_else(|| {
        find_status_by_terms(
            statuses,
            &["to do", "todo", "backlog", "open", "new", "not started"],
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate_with_ellipsis("short", 10), "short");
    }

    #[test]
    fn test_truncate_long_string() {
        let result = truncate_with_ellipsis("this is a very long string", 10);
        assert_eq!(result, "this is a…");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate_with_ellipsis("exactly10", 9), "exactly10");
    }

    #[test]
    fn test_extract_id_from_url() {
        assert_eq!(
            extract_id_from_url("https://linear.app/team/issue/ABC-123", "linear.app"),
            Some("ABC-123".to_string())
        );
    }

    #[test]
    fn test_extract_id_no_match() {
        assert_eq!(extract_id_from_url("ABC-123", "linear.app"), None);
    }

    #[test]
    fn test_find_status_by_terms() {
        let statuses = vec![
            "To Do".to_string(),
            "In Progress".to_string(),
            "Done".to_string(),
        ];
        assert_eq!(
            find_status_by_terms(&statuses, &["in progress"]),
            Some("In Progress".to_string())
        );
    }

    #[test]
    fn test_find_in_progress_status_with_override() {
        let statuses = vec!["To Do".to_string()];
        assert_eq!(
            find_in_progress_status(&statuses, Some("Custom")),
            Some("Custom".to_string())
        );
    }
}
