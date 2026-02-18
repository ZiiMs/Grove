use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};

use super::types::{CheckRunsResponse, CheckStatus, PullRequestResponse, PullRequestStatus};

const GITHUB_API_URL: &str = "https://api.github.com";

pub struct GitHubClient {
    client: reqwest::Client,
    owner: String,
    repo: String,
}

impl GitHubClient {
    pub fn new(token: &str, owner: &str, repo: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).context("Invalid token")?,
        );
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent("flock-tui")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }

    pub async fn get_pr_for_branch(&self, branch: &str) -> Result<PullRequestStatus> {
        let url = format!(
            "{}/repos/{}/{}/pulls",
            GITHUB_API_URL, self.owner, self.repo
        );

        tracing::info!("Fetching PRs from: {}", url);

        let response = self
            .client
            .get(&url)
            .query(&[("state", "open")])
            .send()
            .await
            .context("Failed to fetch pull requests")?;

        tracing::info!("GitHub API response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("GitHub API error: {} - {}", status, body);
            anyhow::bail!("GitHub API error: {} - {}", status, body);
        }

        let body = response.text().await.context("Failed to read response")?;
        tracing::info!("GitHub API response body: {}", &body.chars().take(500).collect::<String>());

        let prs: Vec<PullRequestResponse> = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to parse PR response: {}", e);
                tracing::error!("Response body: {}", body);
                anyhow::bail!("Failed to parse pull request response: {}", e);
            }
        };

        tracing::info!(
            "GitHub: {} open PRs in {}/{}, looking for branch '{}'",
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
            "Found PR #{} for branch '{}', state={}, merged_at={:?}",
            pr.number,
            branch,
            pr.state,
            pr.merged_at
        );

        if pr.merged_at.is_some() {
            return Ok(PullRequestStatus::Merged { number: pr.number });
        }

        if pr.state == "closed" {
            return Ok(PullRequestStatus::Closed { number: pr.number });
        }

        let checks = self.get_checks_status(&pr.head.sha).await.unwrap_or(CheckStatus::None);

        if pr.draft {
            return Ok(PullRequestStatus::Draft {
                number: pr.number,
                url: pr.html_url,
                checks,
            });
        }

        Ok(PullRequestStatus::Open {
            number: pr.number,
            url: pr.html_url,
            checks,
        })
    }

    async fn get_checks_status(&self, sha: &str) -> Result<CheckStatus> {
        let checks_url = format!(
            "{}/repos/{}/{}/commits/{}/check-runs",
            GITHUB_API_URL, self.owner, self.repo, sha
        );

        let response = self
            .client
            .get(&checks_url)
            .send()
            .await
            .context("Failed to fetch check runs")?;

        if !response.status().is_success() {
            return Ok(CheckStatus::None);
        }

        let checks: CheckRunsResponse = response
            .json()
            .await
            .context("Failed to parse check runs response")?;

        if checks.check_runs.is_empty() {
            return Ok(CheckStatus::None);
        }

        let mut has_running = false;
        let mut has_pending = false;
        let mut has_failure = false;

        for check in &checks.check_runs {
            let status = CheckStatus::from_github_status(
                &check.status,
                check.conclusion.as_deref(),
            );

            match status {
                CheckStatus::Failure | CheckStatus::TimedOut => has_failure = true,
                CheckStatus::Running => has_running = true,
                CheckStatus::Pending => has_pending = true,
                _ => {}
            }
        }

        if has_failure {
            Ok(CheckStatus::Failure)
        } else if has_running {
            Ok(CheckStatus::Running)
        } else if has_pending {
            Ok(CheckStatus::Pending)
        } else {
            Ok(CheckStatus::Success)
        }
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
            "{}/repos/{}/{}",
            GITHUB_API_URL, self.owner, self.repo
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to GitHub")?;

        if !response.status().is_success() {
            let status = response.status();
            anyhow::bail!("GitHub connection failed: {}", status);
        }

        Ok(())
    }
}

pub struct OptionalGitHubClient {
    client: Option<GitHubClient>,
}

impl OptionalGitHubClient {
    pub fn new(owner: Option<&str>, repo: Option<&str>, token: Option<&str>) -> Self {
        let client = match (owner, repo, token) {
            (Some(o), Some(r), Some(t)) => GitHubClient::new(t, o, r).ok(),
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
}
