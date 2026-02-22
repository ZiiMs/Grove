use anyhow::{bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use std::collections::HashSet;

use super::types::{
    ClickUpListResponse, ClickUpTaskData, ClickUpTaskListResponse, ClickUpTaskSummary, StatusOption,
};

const BASE_URL: &str = "https://api.clickup.com/api/v2";

pub struct ClickUpClient {
    client: reqwest::Client,
    list_id: String,
}

impl ClickUpClient {
    pub fn new(token: &str, list_id: String) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("{} ", token)).context("Invalid ClickUp token")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, list_id })
    }

    pub async fn get_task(&self, task_id: &str) -> Result<ClickUpTaskData> {
        let url = format!("{}/task/{}", BASE_URL, task_id);

        tracing::debug!("ClickUp get_task: url={}", url);

        let response = self
            .client
            .get(&url)
            .query(&[("include", "subtasks")])
            .send()
            .await
            .context("Failed to fetch ClickUp task")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "ClickUp get_task response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("ClickUp API error: {} - {}", status, response_text);
            bail!("ClickUp API error: {} - {}", status, response_text);
        }

        let task: ClickUpTaskData = serde_json::from_str(&response_text)
            .context("Failed to parse ClickUp task response")?;

        Ok(task)
    }

    pub async fn get_list_tasks(&self) -> Result<Vec<ClickUpTaskSummary>> {
        let url = format!("{}/list/{}/task", BASE_URL, self.list_id);

        tracing::debug!("ClickUp get_list_tasks: url={}", url);

        let response = self
            .client
            .get(&url)
            .query(&[("subtasks", "true"), ("include", "parent")])
            .send()
            .await
            .context("Failed to fetch ClickUp tasks")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "ClickUp get_list_tasks response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("ClickUp API error: {} - {}", status, response_text);
            bail!("ClickUp API error: {} - {}", status, response_text);
        }

        let task_list: ClickUpTaskListResponse = serde_json::from_str(&response_text)
            .context("Failed to parse ClickUp task list response")?;

        Ok(task_list
            .tasks
            .into_iter()
            .map(ClickUpTaskSummary::from)
            .collect())
    }

    pub async fn get_list_tasks_with_subtasks(&self) -> Result<Vec<ClickUpTaskSummary>> {
        let tasks = self.get_list_tasks().await?;

        let parent_ids: HashSet<String> = tasks
            .iter()
            .filter_map(|t| t.parent_id.as_ref())
            .cloned()
            .collect();

        let enriched_tasks: Vec<ClickUpTaskSummary> = tasks
            .into_iter()
            .map(|mut t| {
                t.has_children = parent_ids.contains(&t.id);
                t
            })
            .collect();

        Ok(enriched_tasks)
    }

    pub async fn update_task_status(&self, task_id: &str, new_status: &str) -> Result<()> {
        let url = format!("{}/task/{}", BASE_URL, task_id);

        let body = serde_json::json!({
            "status": new_status
        });

        tracing::debug!(
            "ClickUp update_task_status: url={}, body={}",
            url,
            serde_json::to_string(&body).unwrap_or_default()
        );

        let response = self
            .client
            .put(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to update ClickUp task status")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "ClickUp update_task_status response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!(
                "ClickUp API error updating task: {} - {}",
                status,
                response_text
            );
            bail!(
                "ClickUp API error updating task: {} - {}",
                status,
                response_text
            );
        }

        Ok(())
    }

    pub async fn get_statuses(&self) -> Result<Vec<StatusOption>> {
        let url = format!("{}/list/{}", BASE_URL, self.list_id);

        tracing::debug!("ClickUp get_statuses: url={}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch ClickUp list")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "ClickUp get_statuses response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!(
                "ClickUp API error fetching list: {} - {}",
                status,
                response_text
            );
            bail!(
                "ClickUp API error fetching list: {} - {}",
                status,
                response_text
            );
        }

        let list: ClickUpListResponse = serde_json::from_str(&response_text)
            .context("Failed to parse ClickUp list response")?;

        Ok(list
            .data
            .statuses
            .into_iter()
            .map(StatusOption::from)
            .collect())
    }

    pub async fn move_to_in_progress(
        &self,
        task_id: &str,
        override_status: Option<&str>,
    ) -> Result<()> {
        let status = match override_status {
            Some(s) => s.to_string(),
            None => self.find_in_progress_status().await?,
        };

        self.update_task_status(task_id, &status).await
    }

    pub async fn move_to_done(&self, task_id: &str, override_status: Option<&str>) -> Result<()> {
        let status = match override_status {
            Some(s) => s.to_string(),
            None => self.find_done_status().await?,
        };

        self.update_task_status(task_id, &status).await
    }

    pub async fn move_to_not_started(
        &self,
        task_id: &str,
        override_status: Option<&str>,
    ) -> Result<()> {
        let status = match override_status {
            Some(s) => s.to_string(),
            None => self.find_not_started_status().await?,
        };

        self.update_task_status(task_id, &status).await
    }

    async fn find_in_progress_status(&self) -> Result<String> {
        let statuses = self.get_statuses().await?;

        for opt in &statuses {
            let lower = opt.status.to_lowercase();
            if lower.contains("in progress") || lower.contains("doing") || lower == "in review" {
                return Ok(opt.status.clone());
            }
        }

        bail!("No 'In Progress' status found in ClickUp list");
    }

    async fn find_done_status(&self) -> Result<String> {
        let statuses = self.get_statuses().await?;

        for opt in &statuses {
            let lower = opt.status.to_lowercase();
            if lower.contains("done") || lower.contains("complete") || lower == "closed" {
                return Ok(opt.status.clone());
            }
        }

        bail!("No 'Done' status found in ClickUp list");
    }

    async fn find_not_started_status(&self) -> Result<String> {
        let statuses = self.get_statuses().await?;

        for opt in &statuses {
            let lower = opt.status.to_lowercase();
            if lower.contains("to do")
                || lower.contains("todo")
                || lower.contains("backlog")
                || lower.contains("open")
                || lower == "new"
            {
                return Ok(opt.status.clone());
            }
        }

        if let Some(first) = statuses.first() {
            return Ok(first.status.clone());
        }

        bail!("No 'Not Started' status found in ClickUp list");
    }
}

use crate::cache::Cache;

pub struct OptionalClickUpClient {
    client: Option<ClickUpClient>,
    cached_tasks: Cache<Vec<ClickUpTaskSummary>>,
}

impl OptionalClickUpClient {
    pub fn new(token: Option<&str>, list_id: Option<String>, cache_ttl_secs: u64) -> Self {
        let client = token.and_then(|tok| list_id.and_then(|id| ClickUpClient::new(tok, id).ok()));
        Self {
            client,
            cached_tasks: Cache::new(cache_ttl_secs),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub async fn get_task(&self, task_id: &str) -> Result<ClickUpTaskData> {
        match &self.client {
            Some(c) => c.get_task(task_id).await,
            None => bail!("ClickUp not configured"),
        }
    }

    pub async fn get_list_tasks(&self) -> Result<Vec<ClickUpTaskSummary>> {
        match &self.client {
            Some(c) => c.get_list_tasks().await,
            None => bail!("ClickUp not configured"),
        }
    }

    pub async fn get_list_tasks_with_subtasks(&self) -> Result<Vec<ClickUpTaskSummary>> {
        if let Some(tasks) = self.cached_tasks.get().await {
            tracing::debug!("ClickUp cache hit: returning {} cached tasks", tasks.len());
            return Ok(tasks);
        }

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ClickUp not configured"))?;
        let tasks = client.get_list_tasks_with_subtasks().await?;

        tracing::debug!("ClickUp cache miss: fetched {} tasks", tasks.len());
        self.cached_tasks.set(tasks.clone()).await;

        Ok(tasks)
    }

    pub async fn update_task_status(&self, task_id: &str, new_status: &str) -> Result<()> {
        let result = match &self.client {
            Some(c) => c.update_task_status(task_id, new_status).await,
            None => bail!("ClickUp not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("ClickUp cache invalidated after status update");
        }

        result
    }

    pub async fn move_to_in_progress(
        &self,
        task_id: &str,
        override_status: Option<&str>,
    ) -> Result<()> {
        let result = match &self.client {
            Some(c) => c.move_to_in_progress(task_id, override_status).await,
            None => bail!("ClickUp not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("ClickUp cache invalidated after moving to in progress");
        }

        result
    }

    pub async fn move_to_done(&self, task_id: &str, override_status: Option<&str>) -> Result<()> {
        let result = match &self.client {
            Some(c) => c.move_to_done(task_id, override_status).await,
            None => bail!("ClickUp not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("ClickUp cache invalidated after moving to done");
        }

        result
    }

    pub async fn move_to_not_started(
        &self,
        task_id: &str,
        override_status: Option<&str>,
    ) -> Result<()> {
        let result = match &self.client {
            Some(c) => c.move_to_not_started(task_id, override_status).await,
            None => bail!("ClickUp not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("ClickUp cache invalidated after moving to not started");
        }

        result
    }

    pub async fn get_statuses(&self) -> Result<Vec<StatusOption>> {
        match &self.client {
            Some(c) => c.get_statuses().await,
            None => bail!("ClickUp not configured"),
        }
    }

    pub async fn invalidate_cache(&self) {
        self.cached_tasks.invalidate().await;
        tracing::debug!("ClickUp cache manually invalidated");
    }
}
