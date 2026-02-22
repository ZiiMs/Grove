use anyhow::{bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use std::collections::{HashMap, HashSet};
use tokio::sync::{Mutex, RwLock};

use super::types::{
    GraphQLResponse, IssueQueryData, IssueUpdateData, LinearIssueSummary, TeamIssuesQueryData,
    TeamStatesQueryData, TeamsQueryData, WorkflowState,
};
use crate::cache::Cache;

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

pub struct LinearClient {
    client: reqwest::Client,
    team_id: String,
    cached_states: Mutex<Option<Vec<WorkflowState>>>,
}

impl LinearClient {
    pub fn new(token: &str, team_id: String) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(token).context("Invalid Linear token")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            team_id,
            cached_states: Mutex::new(None),
        })
    }

    pub async fn get_teams(&self) -> Result<Vec<(String, String, String)>> {
        let query = r#"
            query Teams {
                teams {
                    nodes {
                        id
                        name
                        key
                    }
                }
            }
        "#;

        let body = serde_json::json!({ "query": query });

        tracing::debug!("Linear get_teams: sending request");

        let response = self
            .client
            .post(LINEAR_API_URL)
            .json(&body)
            .send()
            .await
            .context("Failed to fetch Linear teams")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Linear get_teams response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Linear API error: {} - {}", status, response_text);
            bail!("Linear API error: {} - {}", status, response_text);
        }

        let data: GraphQLResponse<TeamsQueryData> = serde_json::from_str(&response_text)
            .context("Failed to parse Linear teams response")?;

        Ok(data
            .data
            .teams
            .nodes
            .into_iter()
            .map(|t| (t.id, t.name, t.key))
            .collect())
    }

    pub async fn get_issue(&self, issue_id: &str) -> Result<LinearIssueSummary> {
        let query = r#"
            query Issue($id: String!) {
                issue(id: $id) {
                    id
                    identifier
                    title
                    url
                    state {
                        id
                        name
                        type
                        color
                    }
                    parent { id }
                    children(first: 50) {
                        nodes { id }
                    }
                }
            }
        "#;

        let body = serde_json::json!({
            "query": query,
            "variables": { "id": issue_id }
        });

        tracing::debug!("Linear get_issue: id={}", issue_id);

        let response = self
            .client
            .post(LINEAR_API_URL)
            .json(&body)
            .send()
            .await
            .context("Failed to fetch Linear issue")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Linear get_issue response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Linear API error: {} - {}", status, response_text);
            bail!("Linear API error: {} - {}", status, response_text);
        }

        let data: GraphQLResponse<IssueQueryData> = serde_json::from_str(&response_text)
            .context("Failed to parse Linear issue response")?;

        let issue = data.data.issue.context("Issue not found")?;

        let has_children = issue
            .children
            .as_ref()
            .map(|c| !c.nodes.is_empty())
            .unwrap_or(false);

        Ok(LinearIssueSummary {
            id: issue.id,
            identifier: issue.identifier,
            title: issue.title,
            state_id: issue.state.id,
            state_name: issue.state.name,
            state_type: issue.state.state_type,
            url: issue.url,
            parent_id: issue.parent.map(|p| p.id),
            has_children,
        })
    }

    pub async fn get_team_issues(&self) -> Result<Vec<LinearIssueSummary>> {
        let query = r#"
            query TeamIssues($teamId: String!, $first: Int) {
                team(id: $teamId) {
                    issues(first: $first) {
                        nodes {
                            id
                            identifier
                            title
                            url
                            state {
                                id
                                name
                                type
                                color
                            }
                            parent { id }
                            children(first: 50) {
                                nodes {
                                    id
                                    identifier
                                    title
                                    url
                                    state {
                                        id
                                        name
                                        type
                                        color
                                    }
                                    parent { id }
                                    children(first: 1) { nodes { id } }
                                }
                            }
                        }
                    }
                }
            }
        "#;

        let body = serde_json::json!({
            "query": query,
            "variables": {
                "teamId": self.team_id,
                "first": 100
            }
        });

        tracing::debug!("Linear get_team_issues: team_id={}", self.team_id);

        let response = self
            .client
            .post(LINEAR_API_URL)
            .json(&body)
            .send()
            .await
            .context("Failed to fetch Linear team issues")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Linear get_team_issues response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Linear API error: {} - {}", status, response_text);
            bail!("Linear API error: {} - {}", status, response_text);
        }

        let data: GraphQLResponse<TeamIssuesQueryData> = serde_json::from_str(&response_text)
            .context("Failed to parse Linear team issues response")?;

        let team = data.data.team.context("Team not found")?;

        Ok(team
            .issues
            .nodes
            .into_iter()
            .map(|issue| {
                let has_children = issue
                    .children
                    .as_ref()
                    .map(|c| !c.nodes.is_empty())
                    .unwrap_or(false);

                LinearIssueSummary {
                    id: issue.id,
                    identifier: issue.identifier,
                    title: issue.title,
                    state_id: issue.state.id,
                    state_name: issue.state.name,
                    state_type: issue.state.state_type,
                    url: issue.url,
                    parent_id: issue.parent.map(|p| p.id),
                    has_children,
                }
            })
            .collect())
    }

    pub async fn get_team_issues_with_children(&self) -> Result<Vec<LinearIssueSummary>> {
        let issues = self.get_team_issues().await?;

        let parent_ids: HashSet<String> = issues
            .iter()
            .filter_map(|i| i.parent_id.as_ref())
            .cloned()
            .collect();

        let parent_states: HashMap<String, (String, String, String)> = issues
            .iter()
            .filter(|i| parent_ids.contains(&i.id))
            .map(|i| {
                (
                    i.id.clone(),
                    (i.state_id.clone(), i.state_name.clone(), i.state_type.clone()),
                )
            })
            .collect();

        tracing::debug!(
            "Linear inheritance: {} parent states collected for {} issues",
            parent_states.len(),
            issues.len()
        );

        let enriched: Vec<LinearIssueSummary> = issues
            .into_iter()
            .map(|mut i| {
                i.has_children = parent_ids.contains(&i.id) || i.has_children;
                if let Some(parent_id) = &i.parent_id {
                    if let Some((state_id, state_name, state_type)) = parent_states.get(parent_id)
                    {
                        tracing::debug!(
                            "Linear subtask {} inheriting status '{}' from parent",
                            i.identifier,
                            state_name
                        );
                        i.state_id = state_id.clone();
                        i.state_name = state_name.clone();
                        i.state_type = state_type.clone();
                    }
                }
                i
            })
            .collect();

        Ok(enriched)
    }

    pub async fn get_workflow_states(&self) -> Result<Vec<WorkflowState>> {
        {
            let cache = self.cached_states.lock().await;
            if let Some(ref states) = *cache {
                return Ok(states.clone());
            }
        }

        let query = r#"
            query WorkflowStates($teamId: String!) {
                team(id: $teamId) {
                    states {
                        nodes {
                            id
                            name
                            type
                            color
                        }
                    }
                }
            }
        "#;

        let body = serde_json::json!({
            "query": query,
            "variables": { "teamId": self.team_id }
        });

        tracing::debug!("Linear get_workflow_states: team_id={}", self.team_id);

        let response = self
            .client
            .post(LINEAR_API_URL)
            .json(&body)
            .send()
            .await
            .context("Failed to fetch Linear workflow states")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Linear get_workflow_states response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Linear API error: {} - {}", status, response_text);
            bail!("Linear API error: {} - {}", status, response_text);
        }

        let data: GraphQLResponse<TeamStatesQueryData> = serde_json::from_str(&response_text)
            .context("Failed to parse Linear workflow states response")?;

        let team = data.data.team.context("Team not found")?;

        let states: Vec<WorkflowState> = team
            .states
            .nodes
            .into_iter()
            .map(|s| WorkflowState {
                id: s.id,
                name: s.name,
                state_type: s.state_type,
                color: s.color,
            })
            .collect();

        {
            let mut cache = self.cached_states.lock().await;
            *cache = Some(states.clone());
        }

        Ok(states)
    }

    pub async fn update_issue_status(&self, issue_id: &str, state_id: &str) -> Result<()> {
        let query = r#"
            mutation IssueUpdate($id: String!, $stateId: String!) {
                issueUpdate(id: $id, input: { stateId: $stateId }) {
                    success
                    issue {
                        id
                        state { id name }
                    }
                }
            }
        "#;

        let body = serde_json::json!({
            "query": query,
            "variables": {
                "id": issue_id,
                "stateId": state_id
            }
        });

        tracing::debug!(
            "Linear update_issue_status: issue_id={}, state_id={}",
            issue_id,
            state_id
        );

        let response = self
            .client
            .post(LINEAR_API_URL)
            .json(&body)
            .send()
            .await
            .context("Failed to update Linear issue status")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Linear update_issue_status response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Linear API error: {} - {}", status, response_text);
            bail!("Linear API error: {} - {}", status, response_text);
        }

        let data: GraphQLResponse<IssueUpdateData> = serde_json::from_str(&response_text)
            .context("Failed to parse Linear issue update response")?;

        if !data.data.issue_update.success {
            bail!("Linear issue update failed");
        }

        Ok(())
    }

    async fn find_state_by_type(&self, state_types: &[&str]) -> Result<Option<String>> {
        let states = self.get_workflow_states().await?;

        for state_type in state_types {
            for state in &states {
                if state.state_type.to_lowercase() == *state_type {
                    return Ok(Some(state.id.clone()));
                }
            }
        }

        Ok(None)
    }

    pub async fn move_to_in_progress(
        &self,
        issue_id: &str,
        override_state_id: Option<&str>,
    ) -> Result<()> {
        let state_id = match override_state_id {
            Some(id) => id.to_string(),
            None => self
                .find_state_by_type(&["started", "in_progress"])
                .await?
                .context("No 'In Progress' state found in Linear team")?,
        };

        self.update_issue_status(issue_id, &state_id).await
    }

    pub async fn move_to_done(
        &self,
        issue_id: &str,
        override_state_id: Option<&str>,
    ) -> Result<()> {
        let state_id = match override_state_id {
            Some(id) => id.to_string(),
            None => self
                .find_state_by_type(&["completed", "done", "cancelled"])
                .await?
                .context("No 'Done' state found in Linear team")?,
        };

        self.update_issue_status(issue_id, &state_id).await
    }

    pub async fn move_to_not_started(
        &self,
        issue_id: &str,
        override_state_id: Option<&str>,
    ) -> Result<()> {
        let state_id = match override_state_id {
            Some(id) => id.to_string(),
            None => self
                .find_state_by_type(&["backlog", "unstarted", "todo"])
                .await?
                .context("No 'Not Started' state found in Linear team")?,
        };

        self.update_issue_status(issue_id, &state_id).await
    }
}

pub struct OptionalLinearClient {
    client: RwLock<Option<LinearClient>>,
    cached_issues: Cache<Vec<LinearIssueSummary>>,
}

impl OptionalLinearClient {
    pub fn new(token: Option<&str>, team_id: Option<String>, cache_ttl_secs: u64) -> Self {
        let client = token.and_then(|tok| team_id.and_then(|tid| LinearClient::new(tok, tid).ok()));
        Self {
            client: RwLock::new(client),
            cached_issues: Cache::new(cache_ttl_secs),
        }
    }

    pub fn reconfigure(&self, token: Option<&str>, team_id: Option<String>) {
        let new_client =
            token.and_then(|tok| team_id.and_then(|tid| LinearClient::new(tok, tid).ok()));
        if let Ok(mut guard) = self.client.try_write() {
            *guard = new_client;
        }
    }

    pub async fn is_configured(&self) -> bool {
        self.client.read().await.is_some()
    }

    pub async fn get_teams(&self) -> Result<Vec<(String, String, String)>> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.get_teams().await,
            None => bail!("Linear not configured"),
        }
    }

    pub async fn get_issue(&self, issue_id: &str) -> Result<LinearIssueSummary> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.get_issue(issue_id).await,
            None => bail!("Linear not configured"),
        }
    }

    pub async fn get_team_issues_with_children(&self) -> Result<Vec<LinearIssueSummary>> {
        if let Some(issues) = self.cached_issues.get().await {
            tracing::debug!("Linear cache hit: returning {} cached issues", issues.len());
            return Ok(issues);
        }

        let guard = self.client.read().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Linear not configured"))?;
        let issues = client.get_team_issues_with_children().await?;

        tracing::debug!("Linear cache miss: fetched {} issues", issues.len());
        self.cached_issues.set(issues.clone()).await;

        Ok(issues)
    }

    pub async fn get_workflow_states(&self) -> Result<Vec<WorkflowState>> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.get_workflow_states().await,
            None => bail!("Linear not configured"),
        }
    }

    pub async fn update_issue_status(&self, issue_id: &str, state_id: &str) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.update_issue_status(issue_id, state_id).await,
            None => bail!("Linear not configured"),
        };

        if result.is_ok() {
            self.cached_issues.invalidate().await;
            tracing::debug!("Linear cache invalidated after status update");
        }

        result
    }

    pub async fn move_to_in_progress(
        &self,
        issue_id: &str,
        override_state_id: Option<&str>,
    ) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.move_to_in_progress(issue_id, override_state_id).await,
            None => bail!("Linear not configured"),
        };

        if result.is_ok() {
            self.cached_issues.invalidate().await;
            tracing::debug!("Linear cache invalidated after moving to in progress");
        }

        result
    }

    pub async fn move_to_done(
        &self,
        issue_id: &str,
        override_state_id: Option<&str>,
    ) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.move_to_done(issue_id, override_state_id).await,
            None => bail!("Linear not configured"),
        };

        if result.is_ok() {
            self.cached_issues.invalidate().await;
            tracing::debug!("Linear cache invalidated after moving to done");
        }

        result
    }

    pub async fn move_to_not_started(
        &self,
        issue_id: &str,
        override_state_id: Option<&str>,
    ) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.move_to_not_started(issue_id, override_state_id).await,
            None => bail!("Linear not configured"),
        };

        if result.is_ok() {
            self.cached_issues.invalidate().await;
            tracing::debug!("Linear cache invalidated after moving to not started");
        }

        result
    }

    pub async fn invalidate_cache(&self) {
        self.cached_issues.invalidate().await;
        tracing::debug!("Linear cache manually invalidated");
    }
}
