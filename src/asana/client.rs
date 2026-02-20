use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use tokio::sync::Mutex;

use super::types::{
    AsanaSectionsResponse, AsanaTaskData, AsanaTaskListResponse, AsanaTaskResponse,
    AsanaTaskSummary,
};

/// Asana API client.
pub struct AsanaClient {
    client: reqwest::Client,
    project_gid: Option<String>,
    /// Cached section GIDs: (in_progress_gid, done_gid)
    cached_sections: Mutex<Option<(Option<String>, Option<String>)>>,
}

impl AsanaClient {
    pub fn new(token: &str, project_gid: Option<String>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).context("Invalid Asana token")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

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
            .query(&[("opt_fields", "gid,name,completed,permalink_url")])
            .send()
            .await
            .context("Failed to fetch Asana task")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!("Asana get_task response: status={}, body={}", status, response_text);

        if !status.is_success() {
            tracing::error!("Asana API error: {} - {}", status, response_text);
            anyhow::bail!("Asana API error: {} - {}", status, response_text);
        }

        let task_resp: AsanaTaskResponse = serde_json::from_str(&response_text)
            .context("Failed to parse Asana task response")?;

        Ok(task_resp.data)
    }

    /// Fetch all tasks from the configured project.
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
            .query(&[("opt_fields", "gid,name,completed,permalink_url")])
            .send()
            .await
            .context("Failed to fetch Asana project tasks")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!("Asana get_project_tasks response: status={}, body={}", status, response_text);

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

        // Check cache first
        {
            let cache = self.cached_sections.lock().await;
            if let Some((ref in_progress, ref done)) = *cache {
                let lower = name.to_lowercase();
                if lower.contains("in progress") || lower.contains("in_progress") {
                    return Ok(in_progress.clone());
                }
                if lower.contains("done") || lower.contains("complete") {
                    return Ok(done.clone());
                }
            }
        }

        // Fetch sections from API
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

        // Find "In Progress" and "Done" sections by name
        let mut in_progress_gid = None;
        let mut done_gid = None;

        for section in &sections.data {
            let lower = section.name.to_lowercase();
            if lower.contains("in progress") {
                in_progress_gid = Some(section.gid.clone());
            }
            if lower.contains("done") || lower.contains("complete") {
                done_gid = Some(section.gid.clone());
            }
        }

        // Cache the results
        {
            let mut cache = self.cached_sections.lock().await;
            *cache = Some((in_progress_gid.clone(), done_gid.clone()));
        }

        let lower = name.to_lowercase();
        if lower.contains("in progress") || lower.contains("in_progress") {
            Ok(in_progress_gid)
        } else if lower.contains("done") || lower.contains("complete") {
            Ok(done_gid)
        } else {
            Ok(None)
        }
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
    client: Option<AsanaClient>,
}

impl OptionalAsanaClient {
    pub fn new(token: Option<&str>, project_gid: Option<String>) -> Self {
        let client = token.and_then(|tok| AsanaClient::new(tok, project_gid).ok());
        Self { client }
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub async fn get_task(&self, gid: &str) -> Result<AsanaTaskData> {
        match &self.client {
            Some(c) => c.get_task(gid).await,
            None => anyhow::bail!("Asana not configured"),
        }
    }

    pub async fn get_project_tasks(&self) -> Result<Vec<AsanaTaskSummary>> {
        match &self.client {
            Some(c) => c.get_project_tasks().await,
            None => anyhow::bail!("Asana not configured"),
        }
    }

    pub async fn complete_task(&self, gid: &str) -> Result<()> {
        match &self.client {
            Some(c) => c.complete_task(gid).await,
            None => anyhow::bail!("Asana not configured"),
        }
    }

    pub async fn move_to_in_progress(
        &self,
        task_gid: &str,
        override_gid: Option<&str>,
    ) -> Result<()> {
        match &self.client {
            Some(c) => c.move_to_in_progress(task_gid, override_gid).await,
            None => anyhow::bail!("Asana not configured"),
        }
    }

    pub async fn move_to_done(&self, task_gid: &str, override_gid: Option<&str>) -> Result<()> {
        match &self.client {
            Some(c) => c.move_to_done(task_gid, override_gid).await,
            None => anyhow::bail!("Asana not configured"),
        }
    }
}
