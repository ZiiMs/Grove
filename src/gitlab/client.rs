use crate::util::git::{
    create_forge_client, strip_path_from_url, test_forge_connection, ForgeAuthType,
};
use anyhow::{Context, Result};
use tokio::sync::RwLock;
use tracing::warn;

use super::types::{MergeRequestListItem, MergeRequestResponse, MergeRequestStatus};
use crate::ci::PipelineStatus;

/// Fetch project info from GitLab by path (owner/repo).
/// Returns (project_id, project_name) if found.
pub async fn fetch_project_by_path(
    base_url: &str,
    path: &str,
    token: &str,
) -> Result<(u64, String)> {
    let encoded_path = urlencoding::encode(path);
    let url = format!("{}/api/v4/projects/{}", base_url, encoded_path);

    let client = create_forge_client(ForgeAuthType::PrivateToken, token, None, None)?;

    #[derive(serde::Deserialize)]
    struct ProjectResponse {
        id: u64,
        name: String,
    }

    let project: ProjectResponse = client
        .get(&url)
        .send()
        .await
        .context("Failed to fetch project")?
        .json()
        .await
        .context("Failed to parse project response")?;

    Ok((project.id, project.name))
}

/// GitLab API client.
pub struct GitLabClient {
    client: reqwest::Client,
    base_url: String,
    project_id: u64,
}

impl GitLabClient {
    pub fn new(base_url: &str, project_id: u64, token: &str) -> Result<Self> {
        let client = create_forge_client(ForgeAuthType::PrivateToken, token, None, None)?;

        let cleaned_url = strip_path_from_url(base_url);
        if cleaned_url != base_url.trim_end_matches('/') {
            warn!(
                "GitLab base_url contained path which was stripped to '{}'",
                cleaned_url
            );
        }

        Ok(Self {
            client,
            base_url: cleaned_url,
            project_id,
        })
    }

    /// Get merge request status for a branch.
    /// Two-step: list MRs to find iid, then fetch individual MR for pipeline data.
    pub async fn get_mr_for_branch(&self, branch: &str) -> Result<MergeRequestStatus> {
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
        test_forge_connection(&self.client, &url, "GitLab").await
    }
}

/// Optional GitLab client - may not be configured.
pub struct OptionalGitLabClient {
    client: RwLock<Option<GitLabClient>>,
}

impl OptionalGitLabClient {
    pub fn new(base_url: &str, project_id: Option<u64>, token: Option<&str>) -> Self {
        let client = match (project_id, token) {
            (Some(pid), Some(tok)) => GitLabClient::new(base_url, pid, tok).ok(),
            _ => None,
        };

        Self {
            client: RwLock::new(client),
        }
    }

    pub fn reconfigure(&self, base_url: &str, project_id: Option<u64>, token: Option<&str>) {
        let new_client = match (project_id, token) {
            (Some(pid), Some(tok)) => GitLabClient::new(base_url, pid, tok).ok(),
            _ => None,
        };
        if let Ok(mut guard) = self.client.try_write() {
            *guard = new_client;
        }
    }

    pub async fn is_configured(&self) -> bool {
        self.client.read().await.is_some()
    }

    pub async fn get_mr_for_branch(&self, branch: &str) -> MergeRequestStatus {
        let guard = self.client.read().await;
        match &*guard {
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
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.get_mrs_for_branches(branches).await.unwrap_or_default(),
            None => branches
                .iter()
                .map(|b| (b.clone(), MergeRequestStatus::None))
                .collect(),
        }
    }
}
