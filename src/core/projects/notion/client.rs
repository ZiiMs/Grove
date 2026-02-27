use crate::cache::Cache;
use crate::core::projects::{create_authenticated_client, AuthType};
use anyhow::{bail, Context, Result};
use futures::future::join_all;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};

use super::types::{
    NotionBlock, NotionDatabaseResponse, NotionPageData, NotionPageResponse, NotionPropertySchema,
    NotionQueryResponse, NotionTitleProperty,
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

        let mut extra_headers = HeaderMap::new();
        extra_headers.insert(
            "Notion-Version",
            HeaderValue::from_static(Self::NOTION_VERSION),
        );
        extra_headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        let client = create_authenticated_client(AuthType::Bearer, token, Some(extra_headers))?;

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

    /// Query database and fetch all pages including related child tasks.
    /// Child tasks are linked via a "Tasks" relation property on parent pages.
    pub async fn query_database_with_children(
        &self,
        exclude_done: bool,
    ) -> Result<Vec<NotionPageData>> {
        let parent_pages = self.query_database(false).await?;

        let mut child_to_parent: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut child_ids_to_fetch: Vec<String> = Vec::new();

        for parent in &parent_pages {
            for child_id in &parent.related_task_ids {
                child_to_parent.insert(child_id.clone(), parent.id.clone());
                child_ids_to_fetch.push(child_id.clone());
            }
        }

        let mut children_by_parent: std::collections::HashMap<String, Vec<NotionPageData>> =
            std::collections::HashMap::new();

        let child_parent_pairs: Vec<(String, String)> = child_ids_to_fetch
            .into_iter()
            .filter_map(|child_id| {
                child_to_parent
                    .get(&child_id)
                    .map(|p| (child_id, p.clone()))
            })
            .collect();

        let child_futures: Vec<_> = child_parent_pairs
            .into_iter()
            .map(|(child_id, parent_id)| async move {
                match self.get_page(&child_id).await {
                    Ok(mut child_page) => {
                        child_page.parent_page_id = Some(parent_id.clone());
                        Some((parent_id, child_page))
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch child page {}: {}", child_id, e);
                        None
                    }
                }
            })
            .collect();

        let child_results = join_all(child_futures).await;

        for result in child_results.into_iter().flatten() {
            let (parent_id, child_page) = result;
            children_by_parent
                .entry(parent_id)
                .or_default()
                .push(child_page);
        }

        let mut sorted_pages = Vec::new();
        for parent in parent_pages {
            if !exclude_done || !parent.is_completed() {
                sorted_pages.push(parent.clone());
            }
            if let Some(children) = children_by_parent.get(&parent.id) {
                for child in children {
                    if !exclude_done || !child.is_completed() {
                        sorted_pages.push(child.clone());
                    }
                }
            }
        }

        Ok(sorted_pages)
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
    client: RwLock<Option<NotionClient>>,
    cached_tasks: Cache<(bool, Vec<NotionPageData>)>,
}

impl OptionalNotionClient {
    pub fn new(
        token: Option<&str>,
        database_id: Option<String>,
        status_property_name: Option<String>,
        cache_ttl_secs: u64,
    ) -> Self {
        let client = token.and_then(|tok| {
            database_id.and_then(|db_id| NotionClient::new(tok, db_id, status_property_name).ok())
        });
        Self {
            client: RwLock::new(client),
            cached_tasks: Cache::new(cache_ttl_secs),
        }
    }

    pub fn reconfigure(
        &self,
        token: Option<&str>,
        database_id: Option<String>,
        status_property_name: Option<String>,
    ) {
        let new_client = token.and_then(|tok| {
            database_id.and_then(|db_id| NotionClient::new(tok, db_id, status_property_name).ok())
        });
        if let Ok(mut guard) = self.client.try_write() {
            *guard = new_client;
        }
    }

    pub async fn is_configured(&self) -> bool {
        self.client.read().await.is_some()
    }

    pub async fn get_page(&self, page_id: &str) -> Result<NotionPageData> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.get_page(page_id).await,
            None => bail!("Notion not configured"),
        }
    }

    pub async fn query_database(&self, exclude_done: bool) -> Result<Vec<NotionPageData>> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.query_database(exclude_done).await,
            None => bail!("Notion not configured"),
        }
    }

    pub async fn query_database_with_children(
        &self,
        exclude_done: bool,
    ) -> Result<Vec<NotionPageData>> {
        if let Some((cached_exclude_done, pages)) = self.cached_tasks.get().await {
            if cached_exclude_done == exclude_done {
                tracing::debug!("Notion cache hit: returning {} cached pages", pages.len());
                return Ok(pages);
            }
        }

        let guard = self.client.read().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Notion not configured"))?;
        let pages = client.query_database_with_children(exclude_done).await?;

        tracing::debug!("Notion cache miss: fetched {} pages", pages.len());
        self.cached_tasks.set((exclude_done, pages.clone())).await;

        Ok(pages)
    }

    pub async fn get_status_options(&self) -> Result<StatusOptions> {
        let guard = self.client.read().await;
        match &*guard {
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
        let guard = self.client.read().await;
        let result = match &*guard {
            Some(c) => {
                c.update_page_status(page_id, status_property_name, option_id)
                    .await
            }
            None => bail!("Notion not configured"),
        };

        if result.is_ok() {
            self.cached_tasks.invalidate().await;
            tracing::debug!("Notion cache invalidated after status update");
        }

        result
    }

    pub async fn append_blocks(&self, page_id: &str, blocks: Vec<NotionBlock>) -> Result<()> {
        let guard = self.client.read().await;
        match &*guard {
            Some(c) => c.append_blocks(page_id, blocks).await,
            None => bail!("Notion not configured"),
        }
    }

    pub async fn invalidate_cache(&self) {
        self.cached_tasks.invalidate().await;
        tracing::debug!("Notion cache manually invalidated");
    }

    pub async fn fetch_statuses(&self) -> Result<crate::core::projects::ProviderStatuses> {
        use crate::core::projects::{ProviderStatuses, StatusPayload};

        let status_options = self.get_status_options().await?;
        let parent: Vec<StatusPayload> = status_options
            .all_options
            .into_iter()
            .map(|opt| StatusPayload {
                id: opt.id,
                name: opt.name,
                status_type: None,
                color: None,
            })
            .collect();

        Ok(ProviderStatuses::new(parent))
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

#[derive(Debug, Deserialize)]
struct NotionSearchResponse {
    results: Vec<NotionSearchResult>,
}

#[derive(Debug, Deserialize)]
struct NotionSearchResult {
    id: String,
    #[serde(rename = "object")]
    object: String,
    title: Option<Vec<NotionRichTextTitle>>,
    parent: Option<NotionSearchParent>,
}

#[derive(Debug, Deserialize)]
struct NotionSearchParent {
    page_id: Option<String>,
    database_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NotionRichTextTitle {
    plain_text: String,
}

#[derive(Debug, Deserialize)]
struct NotionPageTitleProperties {
    #[serde(rename = "Name")]
    name: Option<NotionTitleProperty>,
    #[serde(rename = "title")]
    title: Option<NotionTitleProperty>,
    #[serde(flatten)]
    other: std::collections::HashMap<String, serde_json::Value>,
}

impl NotionPageTitleProperties {
    fn get_title(&self) -> Option<String> {
        self.name
            .as_ref()
            .and_then(|t| t.title.first().map(|rt| rt.plain_text.clone()))
            .or_else(|| {
                self.title
                    .as_ref()
                    .and_then(|t| t.title.first().map(|rt| rt.plain_text.clone()))
            })
            .or_else(|| {
                for value in self.other.values() {
                    if let Some(obj) = value.as_object() {
                        if obj.get("type").and_then(|v| v.as_str()) == Some("title") {
                            if let Some(title_arr) = obj.get("title").and_then(|v| v.as_array()) {
                                if let Some(first) = title_arr.first() {
                                    if let Some(text) =
                                        first.get("plain_text").and_then(|v| v.as_str())
                                    {
                                        return Some(text.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
                None
            })
    }
}

pub async fn fetch_databases(token: &str) -> Result<Vec<(String, String, String)>> {
    let mut extra_headers = HeaderMap::new();
    extra_headers.insert(
        "Notion-Version",
        HeaderValue::from_static(NotionClient::NOTION_VERSION),
    );
    extra_headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );

    let client = create_authenticated_client(AuthType::Bearer, token, Some(extra_headers))?;

    let body = serde_json::json!({
        "filter": {
            "property": "object",
            "value": "database"
        },
        "page_size": 100
    });

    let url = format!("{}/search", NotionClient::BASE_URL);

    tracing::debug!("Notion fetch_databases: url={}", url);

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Failed to search Notion databases")?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    tracing::debug!(
        "Notion fetch_databases response: status={}, body={}",
        status,
        response_text
    );

    if !status.is_success() {
        tracing::error!("Notion API error: {} - {}", status, response_text);
        bail!("Notion API error: {} - {}", status, response_text);
    }

    let search_response: NotionSearchResponse =
        serde_json::from_str(&response_text).context("Failed to parse Notion search response")?;

    let databases_with_parents: Vec<(String, String, Option<String>)> = search_response
        .results
        .into_iter()
        .filter(|r| r.object == "database")
        .map(|r| {
            let title = r
                .title
                .as_ref()
                .and_then(|t| t.first())
                .map(|t| t.plain_text.clone())
                .unwrap_or_else(|| "Untitled".to_string());
            let parent_id = r.parent.and_then(|p| p.page_id.or(p.database_id));
            (r.id, title, parent_id)
        })
        .collect();

    let parent_ids: std::collections::HashSet<String> = databases_with_parents
        .iter()
        .filter_map(|(_, _, pid)| pid.clone())
        .collect();

    let mut parent_titles: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for parent_id in parent_ids {
        let page_url = format!("{}/pages/{}", NotionClient::BASE_URL, parent_id);
        match client.get(&page_url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(page_text) = resp.text().await {
                        if let Ok(page) = serde_json::from_str::<serde_json::Value>(&page_text) {
                            if let Some(props) = page.get("properties") {
                                if let Ok(props) = serde_json::from_value::<NotionPageTitleProperties>(
                                    props.clone(),
                                ) {
                                    if let Some(title) = props.get_title() {
                                        parent_titles.insert(parent_id.clone(), title);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to fetch parent page {}: {}", parent_id, e);
            }
        }
    }

    let databases: Vec<(String, String, String)> = databases_with_parents
        .into_iter()
        .map(|(id, title, parent_id)| {
            let parent_title = parent_id
                .and_then(|pid| parent_titles.get(&pid).cloned())
                .unwrap_or_default();
            (id, title, parent_title)
        })
        .collect();

    tracing::debug!(
        "Notion fetch_databases: found {} databases",
        databases.len()
    );

    Ok(databases)
}

pub fn extract_parent_pages(
    databases: &[(String, String, String)],
) -> Vec<(String, String, String)> {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    let mut parents: Vec<(String, String, String)> = Vec::new();

    for (_id, _title, parent_title) in databases {
        if !parent_title.is_empty() && !seen.contains(parent_title) {
            seen.insert(parent_title.clone());
            parents.push((parent_title.clone(), parent_title.clone(), String::new()));
        }
    }

    parents.sort_by(|a, b| a.0.cmp(&b.0));
    parents
}
