use anyhow::{bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use tokio::sync::Mutex;

use super::types::{
    NotionBlock, NotionDatabaseResponse, NotionPageData, NotionPageResponse, NotionPropertySchema,
    NotionQueryResponse,
};

pub struct NotionClient {
    client: reqwest::Client,
    database_id: String,
    status_property_name: String,
    cached_status_options: Mutex<Option<StatusOptions>>,
}

#[derive(Debug, Clone)]
pub struct StatusOptions {
    pub status_property_id: String,
    pub status_property_name: String,
    pub not_started_id: Option<String>,
    pub in_progress_id: Option<String>,
    pub done_id: Option<String>,
    pub all_options: Vec<super::types::NotionStatusOption>,
}

impl NotionClient {
    const BASE_URL: &'static str = "https://api.notion.com/v1";
    const NOTION_VERSION: &'static str = "2022-06-28";

    pub fn new(
        token: &str,
        database_id: String,
        status_property_name: Option<String>,
    ) -> Result<Self> {
        tracing::debug!(
            "NotionClient::new: database_id={}, status_property_name={:?}",
            database_id,
            status_property_name
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).context("Invalid Notion token")?,
        );
        headers.insert(
            "Notion-Version",
            HeaderValue::from_static(Self::NOTION_VERSION),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            database_id,
            status_property_name: status_property_name.unwrap_or_else(|| "Status".to_string()),
            cached_status_options: Mutex::new(None),
        })
    }

    pub async fn get_page(&self, page_id: &str) -> Result<NotionPageData> {
        let clean_id = clean_page_id(page_id);
        let url = format!("{}/pages/{}", Self::BASE_URL, clean_id);

        tracing::debug!("Notion get_page: url={}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Notion page")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Notion get_page response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Notion API error: {} - {}", status, response_text);
            bail!("Notion API error: {} - {}", status, response_text);
        }

        let page: NotionPageResponse =
            serde_json::from_str(&response_text).context("Failed to parse Notion page response")?;

        Ok(NotionPageData::from(page))
    }

    pub async fn query_database(&self, exclude_done: bool) -> Result<Vec<NotionPageData>> {
        let url = format!("{}/databases/{}/query", Self::BASE_URL, self.database_id);

        let body = if exclude_done {
            serde_json::json!({
                "filter": {
                    "property": self.status_property_name,
                    "status": {
                        "does_not_equal": "Done"
                    }
                },
                "sorts": [{
                    "property": self.status_property_name,
                    "direction": "ascending"
                }]
            })
        } else {
            serde_json::json!({})
        };

        tracing::debug!(
            "Notion query_database: url={}, body={}",
            url,
            serde_json::to_string(&body).unwrap_or_default()
        );

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to query Notion database")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Notion query_database response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Notion API error: {} - {}", status, response_text);
            bail!("Notion API error: {} - {}", status, response_text);
        }

        let query_response: NotionQueryResponse = serde_json::from_str(&response_text)
            .context("Failed to parse Notion query response")?;

        Ok(query_response
            .results
            .into_iter()
            .map(NotionPageData::from)
            .collect())
    }

    pub async fn get_status_options(&self) -> Result<StatusOptions> {
        {
            let cache = self.cached_status_options.lock().await;
            if let Some(ref opts) = *cache {
                return Ok(opts.clone());
            }
        }

        let url = format!("{}/databases/{}", Self::BASE_URL, self.database_id);

        tracing::debug!("Notion get_status_options: url={}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Notion database")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Notion get_status_options response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Notion API error: {} - {}", status, response_text);
            bail!("Notion API error: {} - {}", status, response_text);
        }

        let db: NotionDatabaseResponse = serde_json::from_str(&response_text)
            .context("Failed to parse Notion database response")?;

        let status_prop = db
            .properties
            .iter()
            .find(|(name, prop)| {
                prop.prop_type == "status"
                    && name.to_lowercase() == self.status_property_name.to_lowercase()
            })
            .or_else(|| {
                db.properties
                    .iter()
                    .find(|(_, prop)| prop.prop_type == "status")
            })
            .context("No status property found in database")?;

        let options = Self::categorize_options(status_prop.0, status_prop.1)?;

        {
            let mut cache = self.cached_status_options.lock().await;
            *cache = Some(options.clone());
        }

        Ok(options)
    }

    fn categorize_options(prop_name: &str, prop: &NotionPropertySchema) -> Result<StatusOptions> {
        let status = prop
            .status
            .as_ref()
            .context("Status property has no options")?;

        let mut not_started_id = None;
        let mut in_progress_id = None;
        let mut done_id = None;

        for opt in &status.options {
            let lower = opt.name.to_lowercase();
            if lower.contains("not started") || lower == "to do" || lower == "todo" {
                not_started_id = Some(opt.id.clone());
            } else if lower.contains("in progress") || lower.contains("doing") {
                in_progress_id = Some(opt.id.clone());
            } else if lower.contains("done") || lower.contains("complete") {
                done_id = Some(opt.id.clone());
            }
        }

        Ok(StatusOptions {
            status_property_id: prop.id.clone(),
            status_property_name: prop_name.to_string(),
            not_started_id,
            in_progress_id,
            done_id,
            all_options: status.options.clone(),
        })
    }

    pub async fn update_page_status(
        &self,
        page_id: &str,
        status_property_name: &str,
        option_id: &str,
    ) -> Result<()> {
        let clean_id = clean_page_id(page_id);
        let url = format!("{}/pages/{}", Self::BASE_URL, clean_id);

        let body = serde_json::json!({
            "properties": {
                status_property_name: {
                    "status": { "id": option_id }
                }
            }
        });

        tracing::debug!(
            "Notion update_page_status: url={}, body={}",
            url,
            serde_json::to_string(&body).unwrap_or_default()
        );

        let response = self
            .client
            .patch(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to update Notion page status")?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        tracing::debug!(
            "Notion update_page_status response: status={}, body={}",
            status,
            response_text
        );

        if !status.is_success() {
            tracing::error!("Notion API error: {} - {}", status, response_text);
            bail!("Notion API error: {} - {}", status, response_text);
        }

        Ok(())
    }

    pub async fn append_blocks(&self, page_id: &str, blocks: Vec<NotionBlock>) -> Result<()> {
        let clean_id = clean_page_id(page_id);
        let url = format!("{}/blocks/{}/children/append", Self::BASE_URL, clean_id);

        let body = serde_json::json!({
            "children": blocks
        });

        let response = self
            .client
            .patch(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to append blocks to Notion page")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Notion API error: {} - {}", status, body);
        }

        Ok(())
    }
}

fn clean_page_id(id: &str) -> String {
    let cleaned = id.replace('-', "").to_lowercase();
    if cleaned.len() == 32 {
        format!(
            "{}-{}-{}-{}-{}",
            &cleaned[0..8],
            &cleaned[8..12],
            &cleaned[12..16],
            &cleaned[16..20],
            &cleaned[20..32]
        )
    } else {
        id.to_string()
    }
}

pub struct OptionalNotionClient {
    client: Option<NotionClient>,
}

impl OptionalNotionClient {
    pub fn new(
        token: Option<&str>,
        database_id: Option<String>,
        status_property_name: Option<String>,
    ) -> Self {
        let client = token.and_then(|tok| {
            database_id.and_then(|db_id| NotionClient::new(tok, db_id, status_property_name).ok())
        });
        Self { client }
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub async fn get_page(&self, page_id: &str) -> Result<NotionPageData> {
        match &self.client {
            Some(c) => c.get_page(page_id).await,
            None => bail!("Notion not configured"),
        }
    }

    pub async fn query_database(&self, exclude_done: bool) -> Result<Vec<NotionPageData>> {
        match &self.client {
            Some(c) => c.query_database(exclude_done).await,
            None => bail!("Notion not configured"),
        }
    }

    pub async fn get_status_options(&self) -> Result<StatusOptions> {
        match &self.client {
            Some(c) => c.get_status_options().await,
            None => bail!("Notion not configured"),
        }
    }

    pub async fn update_page_status(
        &self,
        page_id: &str,
        status_property_name: &str,
        option_id: &str,
    ) -> Result<()> {
        match &self.client {
            Some(c) => {
                c.update_page_status(page_id, status_property_name, option_id)
                    .await
            }
            None => bail!("Notion not configured"),
        }
    }

    pub async fn append_blocks(&self, page_id: &str, blocks: Vec<NotionBlock>) -> Result<()> {
        match &self.client {
            Some(c) => c.append_blocks(page_id, blocks).await,
            None => bail!("Notion not configured"),
        }
    }
}

pub fn parse_notion_page_id(input: &str) -> String {
    let trimmed = input.trim();

    if trimmed.contains("notion.so") {
        let url = trimmed
            .trim_end_matches('/')
            .split('#')
            .next()
            .unwrap_or(trimmed);

        if let Some(last) = url.rsplit('/').next() {
            if let Some(uuid_part) = last.rsplit('-').next() {
                return clean_uuid(uuid_part);
            }
            return clean_uuid(last);
        }
    }

    clean_uuid(trimmed)
}

fn clean_uuid(s: &str) -> String {
    s.replace('-', "").to_lowercase()
}
