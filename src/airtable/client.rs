use anyhow::{bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use tokio::sync::Mutex;

use super::types::{
    AirtableRecord, AirtableRecordsResponse, AirtableTableSchema, AirtableTaskSummary,
    StatusOption,
};
use crate::cache::Cache;

pub struct AirtableClient {
    client: reqwest::Client,
    base_id: String,
    table_name: String,
    status_field_name: String,
    cached_status_options: Mutex<Option<Vec<StatusOption>>>,
}

impl AirtableClient {
    const BASE_URL: &'static str = "https://api.airtable.com/v0";

    pub fn new(
        token: &str,
        base_id: String,
        table_name: String,
        status_field_name: Option<String>,
    ) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .context("Invalid Airtable token")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_id,
            table_name,
            status_field_name: status_field_name.unwrap_or_else(|| "Status".to_string()),
            cached_status_options: Mutex::new(None),
        })
    }

    pub async fn get_record(&self, record_id: &str) -> Result<AirtableTaskSummary> {
        let url = format!("{}/{}/{}", Self::BASE_URL, self.base_id, self.table_name);
        let url = format!("{}?filterByFormula={{RECORD_ID()='{}'}}", url, record_id);

        tracing::debug!("Airtable get_record: url={}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Airtable record")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Airtable get_record response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Airtable API error: {} - {}", status, response_text);
            bail!("Airtable API error: {} - {}", status, response_text);
        }

        let records: AirtableRecordsResponse =
            serde_json::from_str(&response_text).context("Failed to parse Airtable response")?;

        let record = records
            .records
            .into_iter()
            .next()
            .context("Record not found")?;

        Ok(self.record_to_summary(record))
    }

    pub async fn list_records(&self) -> Result<Vec<AirtableTaskSummary>> {
        let mut all_records = Vec::new();
        let mut offset: Option<String> = None;

        loop {
            let mut url = format!("{}/{}/{}", Self::BASE_URL, self.base_id, self.table_name);

            if let Some(off) = &offset {
                url = format!("{}?offset={}", url, off);
            }

            tracing::debug!("Airtable list_records: url={}", url);

            let response = self
                .client
                .get(&url)
                .send()
                .await
                .context("Failed to fetch Airtable records")?;

            let status = response.status();
            let response_text = response.text().await.unwrap_or_default();

            tracing::debug!(
                "Airtable list_records response: status={}, body={}",
                status,
                response_text
            );

            if !status.is_success() {
                tracing::error!("Airtable API error: {} - {}", status, response_text);
                bail!("Airtable API error: {} - {}", status, response_text);
            }

            let records: AirtableRecordsResponse = serde_json::from_str(&response_text)
                .context("Failed to parse Airtable response")?;

            all_records.extend(
                records
                    .records
                    .into_iter()
                    .map(|r| self.record_to_summary(r)),
            );

            offset = records.offset;
            if offset.is_none() {
                break;
            }
        }

        Ok(all_records)
    }

    pub async fn list_records_with_children(&self) -> Result<Vec<AirtableTaskSummary>> {
        let records = self.list_records().await?;

        let child_ids: std::collections::HashSet<String> = records
            .iter()
            .filter_map(|r| r.parent_id.as_ref())
            .cloned()
            .collect();

        let mut result = Vec::new();
        for mut record in records {
            record.has_children = child_ids.contains(&record.id);
            result.push(record);
        }

        Ok(result)
    }

    fn record_to_summary(&self, record: AirtableRecord) -> AirtableTaskSummary {
        let parent_id = record
            .fields
            .parent
            .as_ref()
            .and_then(|p| p.first().map(|p| p.id.clone()));

        let url = format!("https://airtable.com/{}/{}", self.base_id, self.table_name);

        AirtableTaskSummary {
            id: record.id,
            name: record.fields.name.unwrap_or_else(|| "Untitled".to_string()),
            status: record.fields.status,
            url,
            parent_id,
            has_children: false,
        }
    }

    pub async fn get_status_options(&self) -> Result<Vec<StatusOption>> {
        {
            let cache = self.cached_status_options.lock().await;
            if let Some(ref opts) = *cache {
                return Ok(opts.clone());
            }
        }

        let url = format!(
            "https://api.airtable.com/v0/meta/bases/{}/tables",
            self.base_id
        );

        tracing::debug!("Airtable get_status_options: url={}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Airtable base schema")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Airtable get_status_options response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Airtable API error: {} - {}", status, response_text);
            bail!("Airtable API error: {} - {}", status, response_text);
        }

        let schema: AirtableTableSchema =
            serde_json::from_str(&response_text).context("Failed to parse Airtable schema")?;

        let table = schema
            .tables
            .into_iter()
            .find(|t| t.name == self.table_name)
            .context("Table not found in base")?;

        let status_field = table
            .fields
            .into_iter()
            .find(|f| {
                f.name.to_lowercase() == self.status_field_name.to_lowercase()
                    && f.field_type == "singleSelect"
            })
            .context("Status field not found or not a singleSelect field")?;

        let options: Vec<StatusOption> = status_field
            .options
            .map(|o| {
                o.choices
                    .into_iter()
                    .map(|c| StatusOption { name: c.name })
                    .collect()
            })
            .unwrap_or_default();

        {
            let mut cache = self.cached_status_options.lock().await;
            *cache = Some(options.clone());
        }

        Ok(options)
    }

    pub async fn update_record_status(&self, record_id: &str, status_value: &str) -> Result<()> {
        let url = format!("{}/{}/{}", Self::BASE_URL, self.base_id, self.table_name);

        let status_field_name = &self.status_field_name;
        let body = serde_json::json!({
            "records": [{
                "id": record_id,
                "fields": {
                    status_field_name: status_value
                }
            }]
        });

        tracing::debug!(
            "Airtable update_record_status: url={}, body={}",
            url,
            serde_json::to_string(&body).unwrap_or_default()
        );

        let response = self
            .client
            .patch(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to update Airtable record")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Airtable update_record_status response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Airtable API error: {} - {}", status, response_text);
            bail!("Airtable API error: {} - {}", status, response_text);
        }

        Ok(())
    }

    async fn find_status_option(
        &self,
        search_terms: &[&str],
        override_value: Option<&str>,
    ) -> Result<Option<String>> {
        if let Some(val) = override_value {
            return Ok(Some(val.to_string()));
        }

        let options = self.get_status_options().await?;
        for opt in &options {
            let lower = opt.name.to_lowercase();
            for term in search_terms {
                if lower.contains(term) {
                    return Ok(Some(opt.name.clone()));
                }
            }
        }

        Ok(None)
    }

    pub async fn move_to_in_progress(
        &self,
        record_id: &str,
        override_value: Option<&str>,
    ) -> Result<()> {
        let status_value = self
            .find_status_option(&["in progress", "doing", "active"], override_value)
            .await?;

        match status_value {
            Some(val) => self.update_record_status(record_id, &val).await,
            None => {
                tracing::warn!("No 'In Progress' status option found; skipping move");
                Ok(())
            }
        }
    }

    pub async fn move_to_done(&self, record_id: &str, override_value: Option<&str>) -> Result<()> {
        let status_value = self
            .find_status_option(&["done", "complete", "closed", "resolved"], override_value)
            .await?;

        match status_value {
            Some(val) => self.update_record_status(record_id, &val).await,
            None => {
                tracing::warn!("No 'Done' status option found; skipping move");
                Ok(())
            }
        }
    }

    pub async fn move_to_not_started(
        &self,
        record_id: &str,
        override_value: Option<&str>,
    ) -> Result<()> {
        let status_value = self
            .find_status_option(
                &["not started", "todo", "to do", "backlog", "new", "open"],
                override_value,
            )
            .await?;

        match status_value {
            Some(val) => self.update_record_status(record_id, &val).await,
            None => {
                tracing::warn!("No 'Not Started' status option found; skipping move");
                Ok(())
            }
        }
    }
}

pub struct OptionalAirtableClient {
    client: Option<AirtableClient>,
    cached_tasks: Cache<Vec<AirtableTaskSummary>>,
}

impl OptionalAirtableClient {
    pub fn new(
        token: Option<&str>,
        base_id: Option<String>,
        table_name: Option<String>,
        status_field_name: Option<String>,
        cache_ttl_secs: u64,
    ) -> Self {
        let client = token.and_then(|tok| {
            base_id.and_then(|bid| {
                table_name.and_then(|tn| AirtableClient::new(tok, bid, tn, status_field_name).ok())
            })
        });
        Self {
            client,
            cached_tasks: Cache::new(cache_ttl_secs),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub async fn get_record(&self, record_id: &str) -> Result<AirtableTaskSummary> {
        match &self.client {
            Some(c) => c.get_record(record_id).await,
            None => bail!("Airtable not configured"),
        }
    }

    pub async fn list_records_with_children(&self) -> Result<Vec<AirtableTaskSummary>> {
        if let Some(tasks) = self.cached_tasks.get().await {
            tracing::debug!("Airtable cache hit: returning {} cached tasks", tasks.len());
            return Ok(tasks);
        }

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Airtable not configured"))?;
        let tasks = client.list_records_with_children().await?;

        tracing::debug!("Airtable cache miss: fetched {} tasks", tasks.len());
        self.cached_tasks.set(tasks.clone()).await;

        Ok(tasks)
    }

    pub async fn get_status_options(&self) -> Result<Vec<StatusOption>> {
        match &self.client {
            Some(c) => c.get_status_options().await,
            None => bail!("Airtable not configured"),
        }
    }

    pub async fn update_record_status(&self, record_id: &str, status_value: &str) -> Result<()> {
        let result = match &self.client {
            Some(c) => c.update_record_status(record_id, status_value).await,
            None => bail!("Airtable not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Airtable cache invalidated after status update");
        }

        result
    }

    pub async fn move_to_in_progress(
        &self,
        record_id: &str,
        override_value: Option<&str>,
    ) -> Result<()> {
        let result = match &self.client {
            Some(c) => c.move_to_in_progress(record_id, override_value).await,
            None => bail!("Airtable not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Airtable cache invalidated after moving to in progress");
        }

        result
    }

    pub async fn move_to_done(&self, record_id: &str, override_value: Option<&str>) -> Result<()> {
        let result = match &self.client {
            Some(c) => c.move_to_done(record_id, override_value).await,
            None => bail!("Airtable not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Airtable cache invalidated after moving to done");
        }

        result
    }

    pub async fn move_to_not_started(
        &self,
        record_id: &str,
        override_value: Option<&str>,
    ) -> Result<()> {
        let result = match &self.client {
            Some(c) => c.move_to_not_started(record_id, override_value).await,
            None => bail!("Airtable not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Airtable cache invalidated after moving to not started");
        }

        result
    }

    pub async fn invalidate_cache(&self) {
        self.cached_tasks.invalidate().await;
        tracing::debug!("Airtable cache manually invalidated");
    }
}

pub fn parse_airtable_record_id(input: &str) -> String {
    let trimmed = input.trim();

    if trimmed.contains("airtable.com") {
        if let Some(last) = trimmed.trim_end_matches('/').rsplit('/').next() {
            return last.to_string();
        }
    }

    trimmed.to_string()
}
