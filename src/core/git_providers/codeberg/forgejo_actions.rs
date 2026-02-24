use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;

use crate::ci::PipelineStatus;

#[derive(Debug, Deserialize)]
pub struct ForgejoActionRun {
    pub id: u64,
    #[allow(dead_code)]
    pub name: Option<String>,
    pub head_branch: String,
    pub status: String,
    pub conclusion: Option<String>,
    #[allow(dead_code)]
    pub event: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ForgejoActionRunsResponse {
    pub workflow_runs: Vec<ForgejoActionRun>,
}

pub struct ForgejoActionsClient {
    client: reqwest::Client,
    base_url: String,
    owner: String,
    repo: String,
}

impl ForgejoActionsClient {
    pub fn new(token: &str, owner: &str, repo: &str, base_url: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("token {}", token)).context("Invalid token")?,
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
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }

    pub async fn get_pipeline_for_branch(&self, branch: &str) -> Result<PipelineStatus> {
        let url = format!(
            "{}/api/v1/repos/{}/{}/actions/runs",
            self.base_url, self.owner, self.repo
        );

        tracing::info!("Fetching Forgejo Actions runs: {}", url);

        let response = self
            .client
            .get(&url)
            .query(&[("limit", "50")])
            .send()
            .await
            .context("Failed to fetch action runs")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!("Forgejo Actions error: {} - {}", status, body);
            return Ok(PipelineStatus::None);
        }

        let runs: ForgejoActionRunsResponse = response
            .json()
            .await
            .context("Failed to parse action runs response")?;

        let run = runs
            .workflow_runs
            .into_iter()
            .find(|r| r.head_branch == branch && r.event.as_deref() != Some("schedule"));

        match run {
            Some(r) => {
                tracing::info!(
                    "Found Forgejo Actions run #{} for branch '{}', status={:?}, conclusion={:?}",
                    r.id,
                    branch,
                    r.status,
                    r.conclusion
                );
                Ok(PipelineStatus::from_forgejo_status(
                    &r.status,
                    r.conclusion.as_deref(),
                ))
            }
            None => {
                tracing::info!("No Forgejo Actions run found for branch '{}'", branch);
                Ok(PipelineStatus::None)
            }
        }
    }

    #[allow(dead_code)]
    pub async fn test_connection(&self) -> Result<()> {
        let url = format!(
            "{}/api/v1/repos/{}/{}",
            self.base_url, self.owner, self.repo
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to Forgejo")?;

        if !response.status().is_success() {
            let status = response.status();
            anyhow::bail!("Forgejo connection failed: {}", status);
        }

        Ok(())
    }
}
