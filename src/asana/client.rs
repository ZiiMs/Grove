use anyhow::{Context, Result};
use tokio::sync::{Mutex, RwLock};

use super::types::{
    AsanaProjectsResponse, AsanaSectionsResponse, AsanaTaskData, AsanaTaskListResponse,
    AsanaTaskResponse, AsanaTaskSummary, AsanaWorkspacesResponse, SectionOption,
};
use crate::cache::Cache;
use crate::util::pm::{create_authenticated_client, AuthType};

/// Asana API client.
#[allow(clippy::type_complexity)]
pub struct AsanaClient {
    client: reqwest::Client,
    project_gid: Option<String>,
    /// Cached section GIDs: (not_started_gid, in_progress_gid, done_gid)
    cached_sections: Mutex<Option<(Option<String>, Option<String>, Option<String>)>>,
}

impl AsanaClient {
    pub fn new(token: &str, project_gid: Option<String>) -> Result<Self> {
        let client = create_authenticated_client(AuthType::Bearer, token, None)?;

        Ok(Self {
            client,
            project_gid,
            cached_sections: Mutex::new(None),
        })
    }

    /// Fetch task details by GID.
    pub async fn get_task(&self, gid: &str) -> Result<AsanaTaskData> {
        let url = format!("https://app.asana.com/api/1.0/tasks/{}", gid);

        tracing::debug!("Asana get_task: url={}", url);

        let response = self
            .client
            .get(&url)
            .query(&[(
                "opt_fields",
                "gid,name,completed,permalink_url,parent,num_subtasks",
            )])
            .send()
            .await
            .context("Failed to fetch Asana task")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Asana get_task response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Asana API error: {} - {}", status, response_text);
            anyhow::bail!("Asana API error: {} - {}", status, response_text);
        }

        let task_resp: AsanaTaskResponse =
            serde_json::from_str(&response_text).context("Failed to parse Asana task response")?;

        Ok(task_resp.data)
    }

    /// Fetch all workspaces the user has access to.
    /// Returns Vec<(gid, name, "")> - empty third element for consistency with teams format.
    pub async fn fetch_workspaces(&self) -> Result<Vec<(String, String, String)>> {
        let url = "https://app.asana.com/api/1.0/workspaces";

        tracing::debug!("Asana fetch_workspaces: url={}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to fetch Asana workspaces")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Asana fetch_workspaces response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            anyhow::bail!("Asana API error: {} - {}", status, response_text);
        }

        let workspaces: AsanaWorkspacesResponse = serde_json::from_str(&response_text)
            .context("Failed to parse Asana workspaces response")?;

        Ok(workspaces
            .data
            .into_iter()
            .map(|ws| (ws.gid, ws.name, String::new()))
            .collect())
    }

    /// Fetch all projects in a workspace.
    /// Returns Vec<(gid, name, workspace_name)>.
    pub async fn fetch_projects(
        &self,
        workspace_gid: &str,
    ) -> Result<Vec<(String, String, String)>> {
        let url = format!(
            "https://app.asana.com/api/1.0/workspaces/{}/projects",
            workspace_gid
        );

        tracing::debug!("Asana fetch_projects: url={}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Asana projects")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Asana fetch_projects response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            anyhow::bail!("Asana API error: {} - {}", status, response_text);
        }

        let projects: AsanaProjectsResponse = serde_json::from_str(&response_text)
            .context("Failed to parse Asana projects response")?;

        Ok(projects
            .data
            .into_iter()
            .map(|p| (p.gid, p.name, String::new()))
            .collect())
    }

    /// Fetch subtasks of a parent task.
    pub async fn get_subtasks(&self, parent_gid: &str) -> Result<Vec<AsanaTaskSummary>> {
        let url = format!(
            "https://app.asana.com/api/1.0/tasks/{}/subtasks",
            parent_gid
        );

        tracing::debug!("Asana get_subtasks: url={}", url);

        let response = self
            .client
            .get(&url)
            .query(&[(
                "opt_fields",
                "gid,name,completed,permalink_url,parent,num_subtasks",
            )])
            .send()
            .await
            .context("Failed to fetch Asana subtasks")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Asana get_subtasks response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Asana API error: {} - {}", status, response_text);
            anyhow::bail!("Asana API error: {} - {}", status, response_text);
        }

        let task_list: AsanaTaskListResponse = serde_json::from_str(&response_text)
            .context("Failed to parse Asana subtasks response")?;

        Ok(task_list
            .data
            .into_iter()
            .map(AsanaTaskSummary::from)
            .collect())
    }

    /// Fetch all tasks from the configured project, including subtasks.
    pub async fn get_project_tasks_with_subtasks(&self) -> Result<Vec<AsanaTaskSummary>> {
        let mut all_tasks = self.get_project_tasks().await?;

        let parent_gids: Vec<String> = all_tasks
            .iter()
            .filter(|t| t.num_subtasks > 0)
            .map(|t| t.gid.clone())
            .collect();

        for parent_gid in parent_gids {
            match self.get_subtasks(&parent_gid).await {
                Ok(subtasks) => {
                    all_tasks.extend(subtasks);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch subtasks for task {}: {}", parent_gid, e);
                }
            }
        }

        Ok(all_tasks)
    }
    pub async fn get_project_tasks(&self) -> Result<Vec<AsanaTaskSummary>> {
        let project_gid = match &self.project_gid {
            Some(gid) => gid,
            None => anyhow::bail!("No Asana project GID configured"),
        };

        let url = format!(
            "https://app.asana.com/api/1.0/projects/{}/tasks",
            project_gid
        );

        tracing::debug!("Asana get_project_tasks: url={}", url);

        let response = self
            .client
            .get(&url)
            .query(&[(
                "opt_fields",
                "gid,name,completed,permalink_url,parent,num_subtasks",
            )])
            .send()
            .await
            .context("Failed to fetch Asana project tasks")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Asana get_project_tasks response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Asana API error: {} - {}", status, response_text);
            anyhow::bail!("Asana API error: {} - {}", status, response_text);
        }

        let task_list: AsanaTaskListResponse = serde_json::from_str(&response_text)
            .context("Failed to parse Asana task list response")?;

        Ok(task_list
            .data
            .into_iter()
            .map(AsanaTaskSummary::from)
            .collect())
    }

    /// Mark a task as completed.
    pub async fn complete_task(&self, gid: &str) -> Result<()> {
        let url = format!("https://app.asana.com/api/1.0/tasks/{}", gid);

        let body = serde_json::json!({
            "data": {
                "completed": true
            }
        });

        let response = self
            .client
            .put(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to complete Asana task")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Asana API error completing task: {} - {}", status, body);
        }

        Ok(())
    }

    /// Move a task to a specific section.
    pub async fn move_task_to_section(&self, task_gid: &str, section_gid: &str) -> Result<()> {
        let url = format!(
            "https://app.asana.com/api/1.0/sections/{}/addTask",
            section_gid
        );

        let body = serde_json::json!({
            "data": {
                "task": task_gid
            }
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to move Asana task to section")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Asana API error moving task: {} - {}", status, body);
        }

        Ok(())
    }

    /// Find a section GID by name within the configured project.
    /// Results are cached after first lookup.
    async fn find_section_gid(&self, name: &str) -> Result<Option<String>> {
        let project_gid = match &self.project_gid {
            Some(gid) => gid,
            None => return Ok(None),
        };

        {
            let cache = self.cached_sections.lock().await;
            if let Some((ref not_started, ref in_progress, ref done)) = *cache {
                let lower = name.to_lowercase();
                if lower.contains("not started")
                    || lower.contains("todo")
                    || lower.contains("to do")
                    || lower.contains("backlog")
                {
                    return Ok(not_started.clone());
                }
                if lower.contains("in progress") || lower.contains("in_progress") {
                    return Ok(in_progress.clone());
                }
                if lower.contains("done") || lower.contains("complete") {
                    return Ok(done.clone());
                }
            }
        }

        let url = format!(
            "https://app.asana.com/api/1.0/projects/{}/sections",
            project_gid
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Asana sections")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Asana API error fetching sections: {} - {}", status, body);
        }

        let sections: AsanaSectionsResponse = response
            .json()
            .await
            .context("Failed to parse Asana sections response")?;

        let mut not_started_gid = None;
        let mut in_progress_gid = None;
        let mut done_gid = None;

        for section in &sections.data {
            let lower = section.name.to_lowercase();
            if lower.contains("not started")
                || lower.contains("todo")
                || lower.contains("to do")
                || lower.contains("backlog")
            {
                not_started_gid = Some(section.gid.clone());
            }
            if lower.contains("in progress") {
                in_progress_gid = Some(section.gid.clone());
            }
            if lower.contains("done") || lower.contains("complete") {
                done_gid = Some(section.gid.clone());
            }
        }

        {
            let mut cache = self.cached_sections.lock().await;
            *cache = Some((
                not_started_gid.clone(),
                in_progress_gid.clone(),
                done_gid.clone(),
            ));
        }

        let lower = name.to_lowercase();
        if lower.contains("not started")
            || lower.contains("todo")
            || lower.contains("to do")
            || lower.contains("backlog")
        {
            Ok(not_started_gid)
        } else if lower.contains("in progress") || lower.contains("in_progress") {
            Ok(in_progress_gid)
        } else if lower.contains("done") || lower.contains("complete") {
            Ok(done_gid)
        } else {
            Ok(None)
        }
    }

    /// Get all sections for the configured project.
    /// Returns a list of section options for use in status dropdown.
    pub async fn get_sections(&self) -> Result<Vec<SectionOption>> {
        let project_gid = match &self.project_gid {
            Some(gid) => gid,
            None => anyhow::bail!("No Asana project GID configured"),
        };

        let url = format!(
            "https://app.asana.com/api/1.0/projects/{}/sections",
            project_gid
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Asana sections")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Asana API error fetching sections: {} - {}", status, body);
        }

        let sections: AsanaSectionsResponse = response
            .json()
            .await
            .context("Failed to parse Asana sections response")?;

        Ok(sections.data.into_iter().map(SectionOption::from).collect())
    }

    /// Move a task to "In Progress" section.
    /// Uses `override_gid` if provided, otherwise auto-detects by section name.
    pub async fn move_to_in_progress(
        &self,
        task_gid: &str,
        override_gid: Option<&str>,
    ) -> Result<()> {
        let section_gid = match override_gid {
            Some(gid) => Some(gid.to_string()),
            None => self.find_section_gid("In Progress").await?,
        };

        match section_gid {
            Some(gid) => self.move_task_to_section(task_gid, &gid).await,
            None => {
                tracing::warn!("No 'In Progress' section found; skipping move");
                Ok(())
            }
        }
    }

    /// Move a task to "Not Started" / "To Do" section.
    /// Uses `override_gid` if provided, otherwise auto-detects by section name.
    pub async fn move_to_not_started(
        &self,
        task_gid: &str,
        override_gid: Option<&str>,
    ) -> Result<()> {
        let section_gid = match override_gid {
            Some(gid) => Some(gid.to_string()),
            None => self.find_section_gid("Not Started").await?,
        };

        match section_gid {
            Some(gid) => self.move_task_to_section(task_gid, &gid).await,
            None => {
                tracing::warn!("No 'Not Started' section found; skipping move");
                Ok(())
            }
        }
    }

    /// Mark a task as not completed (uncomplete).
    pub async fn uncomplete_task(&self, gid: &str) -> Result<()> {
        let url = format!("https://app.asana.com/api/1.0/tasks/{}", gid);

        let body = serde_json::json!({
            "data": {
                "completed": false
            }
        });

        let response = self
            .client
            .put(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to uncomplete Asana task")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Asana API error uncompleting task: {} - {}", status, body);
        }

        Ok(())
    }
    /// Move a task to the "Done" section.
    /// Uses `override_gid` if provided, otherwise auto-detects by section name.
    pub async fn move_to_done(&self, task_gid: &str, override_gid: Option<&str>) -> Result<()> {
        let section_gid = match override_gid {
            Some(gid) => Some(gid.to_string()),
            None => self.find_section_gid("Done").await?,
        };

        match section_gid {
            Some(gid) => self.move_task_to_section(task_gid, &gid).await,
            None => {
                tracing::warn!("No 'Done' section found; skipping move");
                Ok(())
            }
        }
    }
}

/// Optional Asana client wrapper — mirrors `OptionalGitLabClient` pattern.
pub struct OptionalAsanaClient {
    client: RwLock<Option<AsanaClient>>,
    cached_tasks: Cache<Vec<AsanaTaskSummary>>,
}

impl OptionalAsanaClient {
    pub fn new(token: Option<&str>, project_gid: Option<String>, cache_ttl_secs: u64) -> Self {
        let client = token.and_then(|tok| AsanaClient::new(tok, project_gid).ok());
        Self {
            client: RwLock::new(client),
            cached_tasks: Cache::new(cache_ttl_secs),
        }
    }

    pub fn reconfigure(&self, token: Option<&str>, project_gid: Option<String>) {
        let new_client = token.and_then(|tok| AsanaClient::new(tok, project_gid).ok());
        if let Ok(mut guard) = self.client.try_write() {
            *guard = new_client;
        }
    }

    pub async fn is_configured(&self) -> bool {
        self.client.read().await.is_some()
    }

    pub async fn fetch_workspaces(&self) -> Result<Vec<(String, String, String)>> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.fetch_workspaces().await,
            None => anyhow::bail!("Asana not configured"),
        }
    }

    pub async fn fetch_projects(
        &self,
        workspace_gid: &str,
    ) -> Result<Vec<(String, String, String)>> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.fetch_projects(workspace_gid).await,
            None => anyhow::bail!("Asana not configured"),
        }
    }

    pub async fn get_task(&self, gid: &str) -> Result<AsanaTaskData> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.get_task(gid).await,
            None => anyhow::bail!("Asana not configured"),
        }
    }

    pub async fn get_project_tasks(&self) -> Result<Vec<AsanaTaskSummary>> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.get_project_tasks().await,
            None => anyhow::bail!("Asana not configured"),
        }
    }

    pub async fn get_project_tasks_with_subtasks(&self) -> Result<Vec<AsanaTaskSummary>> {
        if let Some(tasks) = self.cached_tasks.get().await {
            tracing::debug!("Asana cache hit: returning {} cached tasks", tasks.len());
            return Ok(tasks);
        }

        let guard = self.client.read().await;
        let client = match &*guard {
            Some(c) => c,
            None => anyhow::bail!("Asana not configured"),
        };
        let tasks = client.get_project_tasks_with_subtasks().await?;

        tracing::debug!("Asana cache miss: fetched {} tasks", tasks.len());
        self.cached_tasks.set(tasks.clone()).await;

        Ok(tasks)
    }

    pub async fn complete_task(&self, gid: &str) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.complete_task(gid).await,
            None => anyhow::bail!("Asana not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Asana cache invalidated after completing task");
        }

        result
    }

    pub async fn move_to_in_progress(
        &self,
        task_gid: &str,
        override_gid: Option<&str>,
    ) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.move_to_in_progress(task_gid, override_gid).await,
            None => anyhow::bail!("Asana not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Asana cache invalidated after moving task to in progress");
        }

        result
    }

    pub async fn move_to_done(&self, task_gid: &str, override_gid: Option<&str>) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.move_to_done(task_gid, override_gid).await,
            None => anyhow::bail!("Asana not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Asana cache invalidated after moving task to done");
        }

        result
    }

    pub async fn move_to_not_started(
        &self,
        task_gid: &str,
        override_gid: Option<&str>,
    ) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.move_to_not_started(task_gid, override_gid).await,
            None => anyhow::bail!("Asana not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Asana cache invalidated after moving task to not started");
        }

        result
    }

    pub async fn uncomplete_task(&self, gid: &str) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.uncomplete_task(gid).await,
            None => anyhow::bail!("Asana not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Asana cache invalidated after uncompleting task");
        }

        result
    }

    pub async fn get_sections(&self) -> Result<Vec<SectionOption>> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.get_sections().await,
            None => anyhow::bail!("Asana not configured"),
        }
    }

    pub async fn move_task_to_section(&self, task_gid: &str, section_gid: &str) -> Result<()> {
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => c.move_task_to_section(task_gid, section_gid).await,
            None => anyhow::bail!("Asana not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Asana cache invalidated after moving task to section");
        }

        result
    }

    pub async fn invalidate_cache(&self) {
        self.cached_tasks.invalidate().await;
        tracing::debug!("Asana cache manually invalidated");
    }
}
