use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use tracing::warn;

use super::types::{MergeRequestListItem, MergeRequestResponse, MergeRequestStatus};
use crate::ci::PipelineStatus;

/// GitLab API client.
pub struct GitLabClient {
    client: reqwest::Client,
    base_url: String,
    project_id: u64,
}

impl GitLabClient {
    pub fn new(base_url: &str, project_id: u64, token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "PRIVATE-TOKEN",
            HeaderValue::from_str(token).context("Invalid token")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

        // Validate and clean base_url - should be just the hostname (e.g., https://gitlab.com)
        let cleaned_url = Self::clean_base_url(base_url);

        Ok(Self {
            client,
            base_url: cleaned_url,
            project_id,
        })
    }

    /// Clean base_url by stripping any path segments.
    /// GitLab base_url should be just the hostname (e.g., https://gitlab.com),
    /// not including any path like /namespace/project.
    fn clean_base_url(url: &str) -> String {
        let trimmed = url.trim_end_matches('/');

        // Find the scheme and extract just scheme://host
        if let Some(scheme_end) = trimmed.find("://") {
            let after_scheme = &trimmed[scheme_end + 3..];
            // Find the first / which would indicate a path
            if let Some(path_start) = after_scheme.find('/') {
                let host = &after_scheme[..path_start];
                let scheme = &trimmed[..scheme_end];
                let cleaned = format!("{}://{}", scheme, host);
                let stripped_path = &after_scheme[path_start..];
                warn!(
                    "GitLab base_url contained path '{}' which was stripped to '{}'. \
                     base_url should be just the hostname (e.g., https://gitlab.com)",
                    stripped_path, cleaned
                );
                return cleaned;
            }
        }

        trimmed.to_string()
    }

    /// Get merge request status for a branch.
    /// Two-step: list MRs to find iid, then fetch individual MR for pipeline data.
    pub async fn get_mr_for_branch(&self, branch: &str) -> Result<MergeRequestStatus> {
        // Step 1: List MRs to find the iid
        let list_url = format!(
            "{}/api/v4/projects/{}/merge_requests",
            self.base_url, self.project_id
        );

        let response = self
            .client
            .get(&list_url)
            .query(&[
                ("source_branch", branch),
                ("state", "opened"),
                ("per_page", "1"),
            ])
            .send()
            .await
            .context("Failed to fetch merge requests")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("GitLab API error: {} - {}", status, body);
        }

        let mrs: Vec<MergeRequestListItem> = response
            .json()
            .await
            .context("Failed to parse merge request list")?;

        let list_item = match mrs.into_iter().next() {
            Some(item) => item,
            None => return Ok(MergeRequestStatus::None),
        };

        // Step 2: Fetch individual MR for full data including head_pipeline
        let detail_url = format!(
            "{}/api/v4/projects/{}/merge_requests/{}",
            self.base_url, self.project_id, list_item.iid
        );

        let response = self
            .client
            .get(&detail_url)
            .send()
            .await
            .context("Failed to fetch merge request details")?;

        if !response.status().is_success() {
            // Fall back to list data if detail fetch fails
            return Ok(MergeRequestStatus::Open {
                iid: list_item.iid,
                url: list_item.web_url,
                pipeline: PipelineStatus::None,
            });
        }

        let mr: MergeRequestResponse = response
            .json()
            .await
            .context("Failed to parse merge request response")?;

        Ok(mr.into_status())
    }

    /// Get multiple MR statuses efficiently.
    pub async fn get_mrs_for_branches(
        &self,
        branches: &[String],
    ) -> Result<Vec<(String, MergeRequestStatus)>> {
        let mut results = Vec::new();

        for branch in branches {
            let status = self
                .get_mr_for_branch(branch)
                .await
                .unwrap_or(MergeRequestStatus::None);
            results.push((branch.clone(), status));
        }

        Ok(results)
    }

    /// Check if the client is properly configured.
    pub async fn test_connection(&self) -> Result<()> {
        let url = format!("{}/api/v4/projects/{}", self.base_url, self.project_id);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to GitLab")?;

        if !response.status().is_success() {
            let status = response.status();
            anyhow::bail!("GitLab connection failed: {}", status);
        }

        Ok(())
    }
}

/// Optional GitLab client - may not be configured.
pub struct OptionalGitLabClient {
    client: Option<GitLabClient>,
}

impl OptionalGitLabClient {
    pub fn new(base_url: &str, project_id: Option<u64>, token: Option<&str>) -> Self {
        let client = match (project_id, token) {
            (Some(pid), Some(tok)) => GitLabClient::new(base_url, pid, tok).ok(),
            _ => None,
        };

        Self { client }
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub async fn get_mr_for_branch(&self, branch: &str) -> MergeRequestStatus {
        match &self.client {
            Some(c) => c
                .get_mr_for_branch(branch)
                .await
                .unwrap_or(MergeRequestStatus::None),
            None => MergeRequestStatus::None,
        }
    }

    pub async fn get_mrs_for_branches(
        &self,
        branches: &[String],
    ) -> Vec<(String, MergeRequestStatus)> {
        match &self.client {
            Some(c) => c.get_mrs_for_branches(branches).await.unwrap_or_default(),
            None => branches
                .iter()
                .map(|b| (b.clone(), MergeRequestStatus::None))
                .collect(),
        }
    }
}
