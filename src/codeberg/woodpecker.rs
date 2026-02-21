use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;

use crate::ci::PipelineStatus;

const DEFAULT_WOODPECKER_URL: &str = "https://ci.codeberg.org/api";

#[derive(Debug, Deserialize)]
pub struct WoodpeckerRepo {
    pub id: u64,
    #[allow(dead_code)]
    pub full_name: String,
}

#[derive(Debug, Deserialize)]
pub struct WoodpeckerPipeline {
    #[allow(dead_code)]
    pub id: u64,
    pub number: u64,
    pub branch: String,
    pub status: String,
    #[allow(dead_code)]
    pub event: Option<String>,
}

pub struct WoodpeckerClient {
    client: reqwest::Client,
    base_url: String,
}

impl WoodpeckerClient {
    pub fn new(token: &str) -> Result<Self> {
        Self::with_url(token, DEFAULT_WOODPECKER_URL)
    }

    pub fn with_url(token: &str, base_url: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).context("Invalid token")?,
        );
        headers.insert("Accept", HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent("grove")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url: base_url.to_string(),
        })
    }

    pub async fn lookup_repo_id(&self, owner: &str, repo: &str) -> Result<Option<u64>> {
        let full_name = format!("{}/{}", owner, repo);
        let url = format!("{}/repos/lookup/{}", self.base_url, full_name);

        tracing::info!("Looking up Woodpecker repo: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to lookup repo")?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            tracing::info!("Woodpecker repo not found: {}", full_name);
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!("Woodpecker lookup error: {} - {}", status, body);
            return Ok(None);
        }

        let repo: WoodpeckerRepo = response
            .json()
            .await
            .context("Failed to parse repo response")?;

        tracing::info!("Found Woodpecker repo ID: {} for {}", repo.id, full_name);
        Ok(Some(repo.id))
    }

    pub async fn get_pipeline_for_branch(
        &self,
        repo_id: u64,
        branch: &str,
    ) -> Result<PipelineStatus> {
        let url = format!("{}/repos/{}/pipelines", self.base_url, repo_id);

        tracing::info!("Fetching Woodpecker pipelines: {}", url);

        let response = self
            .client
            .get(&url)
            .query(&[("perPage", "50")])
            .send()
            .await
            .context("Failed to fetch pipelines")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!("Woodpecker pipelines error: {} - {}", status, body);
            return Ok(PipelineStatus::None);
        }

        let pipelines: Vec<WoodpeckerPipeline> = response
            .json()
            .await
            .context("Failed to parse pipelines response")?;

        let pipeline = pipelines
            .into_iter()
            .find(|p| p.branch == branch && p.event.as_deref() != Some("cron"));

        match pipeline {
            Some(p) => {
                tracing::info!(
                    "Found Woodpecker pipeline #{} for branch '{}', status={}",
                    p.number,
                    branch,
                    p.status
                );
                Ok(PipelineStatus::from_woodpecker_status(&p.status))
            }
            None => {
                tracing::info!("No Woodpecker pipeline found for branch '{}'", branch);
                Ok(PipelineStatus::None)
            }
        }
    }

    #[allow(dead_code)]
    pub async fn test_connection(&self) -> Result<()> {
        let url = format!("{}/user", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to Woodpecker")?;

        if !response.status().is_success() {
            let status = response.status();
            anyhow::bail!("Woodpecker connection failed: {}", status);
        }

        Ok(())
    }
}
