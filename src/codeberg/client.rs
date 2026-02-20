use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};

use super::forgejo_actions::ForgejoActionsClient;
use super::types::{PullRequestResponse, PullRequestStatus};
use super::woodpecker::WoodpeckerClient;
use crate::app::config::CodebergCiProvider;
use crate::ci::PipelineStatus;

const DEFAULT_CODEBERG_URL: &str = "https://codeberg.org";

pub struct CodebergClient {
    client: reqwest::Client,
    base_url: String,
    owner: String,
    repo: String,
    ci_provider: CodebergCiProvider,
    forgejo_client: Option<ForgejoActionsClient>,
    woodpecker_client: Option<Arc<RwLock<Option<WoodpeckerClient>>>>,
    woodpecker_repo_id: Arc<RwLock<Option<u64>>>,
}

impl CodebergClient {
    pub fn new(
        token: &str,
        owner: &str,
        repo: &str,
        base_url: &str,
        ci_provider: CodebergCiProvider,
        woodpecker_token: Option<&str>,
        woodpecker_repo_id: Option<u64>,
    ) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("token {}", token)).context("Invalid token")?,
        );
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/json"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent("flock-tui")
            .build()
            .context("Failed to create HTTP client")?;

        let forgejo_client = match ci_provider {
            CodebergCiProvider::ForgejoActions => {
                Some(ForgejoActionsClient::new(token, owner, repo, base_url)?)
            }
            CodebergCiProvider::Woodpecker => None,
        };

        let woodpecker_client = match (ci_provider, woodpecker_token) {
            (CodebergCiProvider::Woodpecker, Some(t)) => {
                Some(Arc::new(RwLock::new(Some(WoodpeckerClient::new(t)?))))
            }
            _ => Some(Arc::new(RwLock::new(None))),
        };

        Ok(Self {
            client,
            base_url: base_url.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            ci_provider,
            forgejo_client,
            woodpecker_client,
            woodpecker_repo_id: Arc::new(RwLock::new(woodpecker_repo_id)),
        })
    }

    pub async fn get_pr_for_branch(&self, branch: &str) -> Result<PullRequestStatus> {
        let url = format!(
            "{}/api/v1/repos/{}/{}/pulls",
            self.base_url, self.owner, self.repo
        );

        tracing::info!("Fetching Codeberg PRs from: {}", url);

        let response = self
            .client
            .get(&url)
            .query(&[("state", "open")])
            .send()
            .await
            .context("Failed to fetch pull requests")?;

        tracing::info!("Codeberg API response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("Codeberg API error: {} - {}", status, body);
            anyhow::bail!("Codeberg API error: {} - {}", status, body);
        }

        let body = response.text().await.context("Failed to read response")?;
        tracing::info!(
            "Codeberg API response body: {}",
            &body.chars().take(500).collect::<String>()
        );

        let prs: Vec<PullRequestResponse> = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to parse PR response: {}", e);
                tracing::error!("Response body: {}", body);
                anyhow::bail!("Failed to parse pull request response: {}", e);
            }
        };

        tracing::info!(
            "Codeberg: {} open PRs in {}/{}, looking for branch '{}'",
            prs.len(),
            self.owner,
            self.repo,
            branch
        );

        for pr in &prs {
            tracing::info!("  PR #{} head.ref = '{}'", pr.number, pr.head.ref_field);
        }

        let pr = match prs.into_iter().find(|pr| pr.head.ref_field == branch) {
            Some(pr) => pr,
            None => {
                tracing::info!("No PR found for branch '{}'", branch);
                return Ok(PullRequestStatus::None);
            }
        };

        tracing::info!(
            "Found PR #{} for branch '{}', state={}, merged={}, merged_at={:?}",
            pr.number,
            branch,
            pr.state,
            pr.merged,
            pr.merged_at
        );

        if pr.merged || pr.merged_at.is_some() {
            return Ok(PullRequestStatus::Merged { number: pr.number });
        }

        if pr.state == "closed" {
            return Ok(PullRequestStatus::Closed { number: pr.number });
        }

        let pipeline = self.fetch_pipeline_status(branch).await;

        if pr.draft {
            return Ok(PullRequestStatus::Draft {
                number: pr.number,
                url: pr.html_url,
                pipeline,
            });
        }

        Ok(PullRequestStatus::Open {
            number: pr.number,
            url: pr.html_url,
            pipeline,
        })
    }

    async fn fetch_pipeline_status(&self, branch: &str) -> PipelineStatus {
        match self.ci_provider {
            CodebergCiProvider::ForgejoActions => {
                if let Some(ref client) = self.forgejo_client {
                    client
                        .get_pipeline_for_branch(branch)
                        .await
                        .unwrap_or_default()
                } else {
                    PipelineStatus::None
                }
            }
            CodebergCiProvider::Woodpecker => {
                let repo_id = self.get_or_lookup_woodpecker_repo_id().await;
                match repo_id {
                    Some(id) => {
                        if let Some(ref client_lock) = self.woodpecker_client {
                            let client_guard = client_lock.read().await;
                            if let Some(ref client) = *client_guard {
                                client
                                    .get_pipeline_for_branch(id, branch)
                                    .await
                                    .unwrap_or_default()
                            } else {
                                PipelineStatus::None
                            }
                        } else {
                            PipelineStatus::None
                        }
                    }
                    None => PipelineStatus::None,
                }
            }
        }
    }

    async fn get_or_lookup_woodpecker_repo_id(&self) -> Option<u64> {
        {
            let guard = self.woodpecker_repo_id.read().await;
            if guard.is_some() {
                return *guard;
            }
        }

        if let Some(ref client_lock) = self.woodpecker_client {
            let client_guard = client_lock.read().await;
            if let Some(ref client) = *client_guard {
                if let Ok(Some(id)) = client.lookup_repo_id(&self.owner, &self.repo).await {
                    let mut guard = self.woodpecker_repo_id.write().await;
                    *guard = Some(id);
                    return Some(id);
                }
            }
        }

        None
    }

    pub fn get_cached_woodpecker_repo_id(&self) -> Option<u64> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let guard = self.woodpecker_repo_id.read().await;
                *guard
            })
        })
    }

    pub async fn get_prs_for_branches(
        &self,
        branches: &[String],
    ) -> Result<Vec<(String, PullRequestStatus)>> {
        let mut results = Vec::new();

        for branch in branches {
            let status = self
                .get_pr_for_branch(branch)
                .await
                .unwrap_or(PullRequestStatus::None);
            results.push((branch.clone(), status));
        }

        Ok(results)
    }

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
            .context("Failed to connect to Codeberg")?;

        if !response.status().is_success() {
            let status = response.status();
            anyhow::bail!("Codeberg connection failed: {}", status);
        }

        Ok(())
    }
}

pub struct OptionalCodebergClient {
    client: Option<CodebergClient>,
}

impl OptionalCodebergClient {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        owner: Option<&str>,
        repo: Option<&str>,
        base_url: Option<&str>,
        token: Option<&str>,
        ci_provider: CodebergCiProvider,
        woodpecker_token: Option<&str>,
        woodpecker_repo_id: Option<u64>,
    ) -> Self {
        let client = match (owner, repo, token) {
            (Some(o), Some(r), Some(t)) => {
                let url = base_url.unwrap_or(DEFAULT_CODEBERG_URL);
                CodebergClient::new(t, o, r, url, ci_provider, woodpecker_token, woodpecker_repo_id)
                    .ok()
            }
            _ => None,
        };

        Self { client }
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub async fn get_pr_for_branch(&self, branch: &str) -> PullRequestStatus {
        match &self.client {
            Some(c) => c
                .get_pr_for_branch(branch)
                .await
                .unwrap_or(PullRequestStatus::None),
            None => PullRequestStatus::None,
        }
    }

    pub async fn get_prs_for_branches(
        &self,
        branches: &[String],
    ) -> Vec<(String, PullRequestStatus)> {
        match &self.client {
            Some(c) => c.get_prs_for_branches(branches).await.unwrap_or_default(),
            None => branches
                .iter()
                .map(|b| (b.clone(), PullRequestStatus::None))
                .collect(),
        }
    }

    pub fn get_cached_woodpecker_repo_id(&self) -> Option<u64> {
        self.client.as_ref()?.get_cached_woodpecker_repo_id()
    }
}
