use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use tokio::sync::{Mutex, RwLock};

use super::types::{
    GraphQLResponse, IssueQueryData, IssueUpdateData, LinearIssueSummary, TeamIssuesQueryData,
    TeamStatesQueryData, TeamsQueryData, ViewerQueryData, WorkflowState,
};
use crate::cache::Cache;

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

pub struct LinearClient {
    client: reqwest::Client,
    team_id: Option<String>,
    cached_states: Mutex<Option<Vec<WorkflowState>>>,
}

impl LinearClient {
    pub fn new(token: &str, team_id: Option<String>) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(token).context("Invalid Linear token")?,
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

    pub async fn fetch_teams_with_token(token: &str) -> Result<Vec<(String, String, String)>> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(token).context("Invalid Linear token")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

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

        tracing::debug!("Linear fetch_teams_with_token: sending request");

        let response = client
            .post(LINEAR_API_URL)
            .json(&body)
            .send()
            .await
            .context("Failed to fetch Linear teams")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Linear fetch_teams_with_token response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Linear API error: {} - {}", status, response_text);
            bail!("Linear API error: {} - {}", status, response_text);
        }

        let data: GraphQLResponse<TeamsQueryData> = match serde_json::from_str(&response_text) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(
                    "Failed to parse Linear teams response: {} - body: {}",
                    e,
                    response_text
                );
                bail!("Failed to parse Linear teams response: {}", e);
            }
        };

        Ok(data
            .data
            .teams
            .nodes
            .into_iter()
            .map(|t| (t.id, t.name, t.key))
            .collect())
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

        let data: GraphQLResponse<TeamsQueryData> = match serde_json::from_str(&response_text) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(
                    "Failed to parse Linear teams response: {} - body: {}",
                    e,
                    response_text
                );
                bail!("Failed to parse Linear teams response: {}", e);
            }
        };

        Ok(data
            .data
            .teams
            .nodes
            .into_iter()
            .map(|t| (t.id, t.name, t.key))
            .collect())
    }

    pub async fn get_viewer(&self) -> Result<String> {
        let query = r#"
            query Query {
                viewer {
                    id
                    displayName
                }
            }
        "#;

        let body = serde_json::json!({ "query": query });

        tracing::debug!("Linear get_viewer: sending request");

        let response = self
            .client
            .post(LINEAR_API_URL)
            .json(&body)
            .send()
            .await
            .context("Failed to fetch Linear viewer")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Linear get_viewer response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Linear API error: {} - {}", status, response_text);
            bail!("Linear API error: {} - {}", status, response_text);
        }

        let data: GraphQLResponse<ViewerQueryData> = match serde_json::from_str(&response_text) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(
                    "Failed to parse Linear viewer response: {} - body: {}",
                    e,
                    response_text
                );
                bail!("Failed to parse Linear viewer response: {}", e);
            }
        };

        Ok(data.data.viewer.display_name)
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
                    team {
                        id
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

        let data: GraphQLResponse<IssueQueryData> = match serde_json::from_str(&response_text) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(
                    "Failed to parse Linear issue response: {} - body: {}",
                    e,
                    response_text
                );
                bail!("Failed to parse Linear issue response: {}", e);
            }
        };

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
            team_id: issue.team.id,
        })
    }

    pub async fn get_team_issues(&self) -> Result<Vec<LinearIssueSummary>> {
        let team_id = self
            .team_id
            .as_ref()
            .context("Team ID required to fetch team issues")?;

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
                                    team { id }
                                }
                            }
                            team { id }
                        }
                    }
                }
            }
        "#;

        let body = serde_json::json!({
            "query": query,
            "variables": {
                "teamId": team_id,
                "first": 100
            }
        });

        tracing::debug!("Linear get_team_issues: team_id={}", team_id);

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

        let data: GraphQLResponse<TeamIssuesQueryData> = match serde_json::from_str(&response_text)
        {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(
                    "Failed to parse Linear team issues response: {} - body: {}",
                    e,
                    response_text
                );
                bail!("Failed to parse Linear team issues response: {}", e);
            }
        };

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
                    team_id: team_id.clone(),
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

        let enriched: Vec<LinearIssueSummary> = issues
            .into_iter()
            .map(|mut i| {
                i.has_children = parent_ids.contains(&i.id) || i.has_children;
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

        let team_id = self
            .team_id
            .as_ref()
            .context("Team ID required to fetch workflow states")?;

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
            "variables": { "teamId": team_id }
        });

        tracing::debug!("Linear get_workflow_states: team_id={}", team_id);

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

        let data: GraphQLResponse<TeamStatesQueryData> = match serde_json::from_str(&response_text)
        {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(
                    "Failed to parse Linear workflow states response: {} - body: {}",
                    e,
                    response_text
                );
                bail!("Failed to parse Linear workflow states response: {}", e);
            }
        };

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

        let data: GraphQLResponse<IssueUpdateData> = match serde_json::from_str(&response_text) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(
                    "Failed to parse Linear issue update response: {} - body: {}",
                    e,
                    response_text
                );
                bail!("Failed to parse Linear issue update response: {}", e);
            }
        };

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
        let client = token.and_then(|tok| LinearClient::new(tok, team_id).ok());
        Self {
            client: RwLock::new(client),
            cached_issues: Cache::new(cache_ttl_secs),
        }
    }

    pub fn reconfigure(&self, token: Option<&str>, team_id: Option<String>) {
        let new_client = token.and_then(|tok| LinearClient::new(tok, team_id).ok());
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
            None => {
                if let Some(token) = crate::app::Config::linear_token() {
                    LinearClient::fetch_teams_with_token(&token).await
                } else {
                    bail!("Linear not configured")
                }
            }
        }
    }

    pub async fn get_viewer(&self) -> Result<String> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.get_viewer().await,
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

    pub async fn fetch_statuses(&self) -> Result<crate::core::projects::ProviderStatuses> {
        use crate::core::projects::{ProviderStatuses, StatusPayload};

        let states = self.get_workflow_states().await?;
        let parent: Vec<StatusPayload> = states
            .into_iter()
            .map(|s| StatusPayload {
                id: s.id,
                name: s.name,
                status_type: Some(s.state_type),
                color: s.color,
            })
            .collect();

        Ok(ProviderStatuses::new(parent))
    }
}
