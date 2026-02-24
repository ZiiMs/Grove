use anyhow::{bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use tokio::sync::RwLock;

pub enum ForgeAuthType {
    PrivateToken,
    Bearer,
    Token,
}

pub fn create_forge_client(
    auth_type: ForgeAuthType,
    token: &str,
    extra_headers: Option<HeaderMap>,
    user_agent: Option<&str>,
) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();

    let (header_name, auth_value) = match auth_type {
        ForgeAuthType::PrivateToken => ("PRIVATE-TOKEN", token.to_string()),
        ForgeAuthType::Bearer => ("Authorization", format!("Bearer {}", token)),
        ForgeAuthType::Token => ("Authorization", format!("token {}", token)),
    };

    headers.insert(
        header_name,
        HeaderValue::from_str(&auth_value).context("Invalid token")?,
    );

    if let Some(extra) = extra_headers {
        headers.extend(extra);
    }

    let mut builder = reqwest::Client::builder().default_headers(headers);

    if let Some(agent) = user_agent {
        builder = builder.user_agent(agent);
    }

    builder.build().context("Failed to create HTTP client")
}

pub async fn check_forge_response(
    response: reqwest::Response,
    forge_name: &str,
) -> Result<reqwest::Response> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("{} API error: {} - {}", forge_name, status, body);
    }
    Ok(response)
}

pub async fn forge_get<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    forge_name: &str,
) -> Result<T> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch from {}", forge_name))?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        bail!("{} API error: {} - {}", forge_name, status, response_text);
    }

    serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to parse {} response", forge_name))
}

pub async fn forge_get_with_query<T: DeserializeOwned, Q: serde::Serialize>(
    client: &reqwest::Client,
    url: &str,
    query: &Q,
    forge_name: &str,
) -> Result<T> {
    let response = client
        .get(url)
        .query(query)
        .send()
        .await
        .with_context(|| format!("Failed to fetch from {}", forge_name))?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        bail!("{} API error: {} - {}", forge_name, status, response_text);
    }

    serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to parse {} response", forge_name))
}

pub async fn test_forge_connection(
    client: &reqwest::Client,
    url: &str,
    forge_name: &str,
) -> Result<()> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to connect to {}", forge_name))?;

    if !response.status().is_success() {
        bail!("{} connection failed: {}", forge_name, response.status());
    }

    Ok(())
}

pub struct OptionalForgeClient<T> {
    client: RwLock<Option<T>>,
}

impl<T> OptionalForgeClient<T> {
    pub fn new<F>(factory: F) -> Self
    where
        F: FnOnce() -> Option<T>,
    {
        Self {
            client: RwLock::new(factory()),
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

    pub async fn with_client<F, R, Fut>(&self, f: F, default: R) -> R
    where
        F: FnOnce(&T) -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => f(c).await,
            None => default,
        }
    }

    pub async fn with_client_result<F, R, Fut>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&T) -> Fut,
        Fut: std::future::Future<Output = Result<R>>,
    {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => f(c).await,
            None => bail!("Forge client not configured"),
        }
    }
}

pub async fn fetch_statuses_for_branches<F, Fut, S>(
    branches: &[String],
    fetch_fn: F,
) -> Vec<(String, S)>
where
    F: Fn(&str) -> Fut,
    Fut: std::future::Future<Output = S>,
    S: Default,
{
    let mut results = Vec::new();
    for branch in branches {
        let status = fetch_fn(branch).await;
        results.push((branch.clone(), status));
    }
    results
}

pub fn strip_path_from_url(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');

    if let Some(scheme_end) = trimmed.find("://") {
        let after_scheme = &trimmed[scheme_end + 3..];
        if let Some(path_start) = after_scheme.find('/') {
            let host = &after_scheme[..path_start];
            let scheme = &trimmed[..scheme_end];
            return format!("{}://{}", scheme, host);
        }
    }

    trimmed.to_string()
}
