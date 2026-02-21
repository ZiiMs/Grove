# Notion Integration Plan

Integrate Notion as a project management backend using a **unified provider model** (similar to GitProvider) that allows users to switch between Asana and Notion.

## Overview

The integration will:
- Use a unified `ProjectMgmtProvider` enum (Asana | Notion) following the GitProvider pattern
- Link existing Notion pages (tasks) to agents via URL/ID
- Auto-detect status property with config overrides
- Update status as agents progress
- Optionally append completion notes to pages (configurable per-task)

---

## Architecture Decision: Unified Project Management Provider

Following the `GitProvider` pattern, project management is now provider-based:

```rust
pub enum ProjectMgmtProvider {
    Asana,  // default for backward compatibility
    Notion,
}
```

This means:
- Single "Tasks" column in agent list (not separate Asana/Notion columns)
- 'a' key opens task assignment for the configured provider
- Settings "Project Mgmt" tab shows provider dropdown + provider-specific fields
- Auto-migrate existing Asana configs to new structure

---

## File Structure

```
src/notion/
├── mod.rs           # Re-exports
├── types.rs         # NotionTaskStatus, API response structs
└── client.rs        # NotionClient, OptionalNotionClient
```

---

## Phase 1: Core Types & Config

### 1.1 Project Management Provider (`src/app/config.rs`)

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProjectMgmtProvider {
    #[default]
    Asana,
    Notion,
}

impl ProjectMgmtProvider {
    pub fn display_name(&self) -> &'static str {
        match self {
            ProjectMgmtProvider::Asana => "Asana",
            ProjectMgmtProvider::Notion => "Notion",
        }
    }

    pub fn all() -> &'static [Self] {
        &[ProjectMgmtProvider::Asana, ProjectMgmtProvider::Notion]
    }
}
```

### 1.2 Global Config (`~/.flock/config.toml`)

```toml
[asana]
refresh_secs = 120

[notion]
refresh_secs = 120
```

Add to `Config` struct:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub global: GlobalConfig,
    pub gitlab: GitLabConfig,
    pub asana: AsanaConfig,
    pub notion: NotionConfig,  // NEW
    pub ui: UiConfig,
    pub performance: PerformanceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionConfig {
    #[serde(default = "default_notion_refresh")]
    pub refresh_secs: u64,
}

fn default_notion_refresh() -> u64 { 120 }

impl Default for NotionConfig {
    fn default() -> Self {
        Self { refresh_secs: default_notion_refresh() }
    }
}

impl Config {
    pub fn notion_token() -> Option<String> {
        std::env::var("NOTION_TOKEN").ok()
    }
}
```

### 1.3 Repo Config (`.flock/project.toml`)

Restructure to use unified provider:

```toml
[project_mgmt]
provider = "notion"  # "asana" or "notion"

[project_mgmt.asana]
project_gid = "1201234567890"
in_progress_section_gid = "1201234567891"
done_section_gid = "1201234567892"

[project_mgmt.notion]
database_id = "abc123def456..."        # Required for Notion
status_property_name = "Status"         # Optional override
in_progress_option = "In Progress"      # Optional override
done_option = "Done"                    # Optional override
```

Add to `RepoConfig`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoConfig {
    pub git: RepoGitConfig,
    pub project_mgmt: RepoProjectMgmtConfig,  // Replaces `asana`
    pub prompts: PromptsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoProjectMgmtConfig {
    #[serde(default)]
    pub provider: ProjectMgmtProvider,
    #[serde(default)]
    pub asana: RepoAsanaConfig,
    #[serde(default)]
    pub notion: RepoNotionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoNotionConfig {
    pub database_id: Option<String>,
    pub status_property_name: Option<String>,
    pub in_progress_option: Option<String>,
    pub done_option: Option<String>,
}
```

### 1.4 Migration Strategy

Auto-migrate existing Asana users on config load:

```rust
impl RepoConfig {
    pub fn load(repo_path: &str) -> Result<Self> {
        let config_path = Self::config_path(repo_path)?;
        
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            
            // Try new format first
            if let Ok(config) = toml::from_str::<RepoConfig>(&content) {
                return Ok(config);
            }
            
            // Try legacy format (top-level `asana`)
            #[derive(Deserialize)]
            struct LegacyRepoConfig {
                git: RepoGitConfig,
                asana: RepoAsanaConfig,  // legacy location
                prompts: PromptsConfig,
            }
            
            if let Ok(legacy) = toml::from_str::<LegacyRepoConfig>(&content) {
                return Ok(RepoConfig {
                    git: legacy.git,
                    project_mgmt: RepoProjectMgmtConfig {
                        provider: ProjectMgmtProvider::Asana,
                        asana: legacy.asana,
                        notion: RepoNotionConfig::default(),
                    },
                    prompts: legacy.prompts,
                });
            }
        }
        
        Ok(Self::default())
    }
}
```

---

## Phase 2: Notion Types & Client

### 2.1 Types (`src/notion/types.rs`)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotionTaskStatus {
    #[default]
    None,
    NotStarted {
        page_id: String,
        name: String,
        url: String,
        status_option_id: String,
    },
    InProgress {
        page_id: String,
        name: String,
        url: String,
        status_option_id: String,
    },
    Completed {
        page_id: String,
        name: String,
    },
    Error {
        page_id: String,
        message: String,
    },
}

impl NotionTaskStatus {
    pub fn format_short(&self) -> String { /* truncate name */ }
    pub fn page_id(&self) -> Option<&str> { /* extract page_id */ }
    pub fn url(&self) -> Option<&str> { /* extract url */ }
    pub fn is_linked(&self) -> bool { !matches!(self, NotionTaskStatus::None) }
}

// API response types
#[derive(Debug, Deserialize)]
pub struct NotionPageResponse {
    pub id: String,
    pub url: String,
    pub properties: NotionProperties,
}

#[derive(Debug, Deserialize)]
pub struct NotionProperties {
    pub title: Option<NotionTitleProperty>,
    pub status: Option<NotionStatusPropertyValue>,
}

#[derive(Debug, Deserialize)]
pub struct NotionTitleProperty {
    pub title: Vec<NotionRichText>,
}

#[derive(Debug, Deserialize)]
pub struct NotionStatusPropertyValue {
    pub status: Option<NotionStatusOption>,
}

#[derive(Debug, Deserialize)]
pub struct NotionStatusOption {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct NotionRichText {
    pub plain_text: String,
}

#[derive(Debug, Deserialize)]
pub struct NotionDatabaseResponse {
    pub properties: std::collections::HashMap<String, NotionPropertySchema>,
}

#[derive(Debug, Deserialize)]
pub struct NotionPropertySchema {
    pub id: String,
    #[serde(rename = "type")]
    pub prop_type: String,
    pub status: Option<NotionStatusSchema>,
}

#[derive(Debug, Deserialize)]
pub struct NotionStatusSchema {
    pub options: Vec<NotionStatusOption>,
}
```

### 2.2 Client (`src/notion/client.rs`)

```rust
use anyhow::{Context, Result, bail};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use tokio::sync::Mutex;

pub struct NotionClient {
    client: reqwest::Client,
    database_id: String,
    cached_status_options: Mutex<Option<StatusOptions>>,
}

struct StatusOptions {
    status_property_id: String,
    not_started_id: Option<String>,
    in_progress_id: Option<String>,
    done_id: Option<String>,
}

impl NotionClient {
    const BASE_URL: &'static str = "https://api.notion.com/v1";
    const NOTION_VERSION: &'static str = "2022-06-28";

    pub fn new(token: &str, database_id: String) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .context("Invalid Notion token")?,
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
            cached_status_options: Mutex::new(None),
        })
    }

    /// Fetch page details by ID.
    pub async fn get_page(&self, page_id: &str) -> Result<NotionPageData> {
        let url = format!("{}/pages/{}", Self::BASE_URL, page_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Notion page")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Notion API error: {} - {}", status, body);
        }

        let page: NotionPageResponse = response
            .json()
            .await
            .context("Failed to parse Notion page response")?;

        Ok(NotionPageData::from(page))
    }

    /// Fetch database schema and find status property.
    pub async fn get_status_options(&self) -> Result<StatusOptions> {
        // Check cache first
        {
            let cache = self.cached_status_options.lock().await;
            if let Some(ref opts) = *cache {
                return Ok(opts.clone());
            }
        }

        let url = format!("{}/databases/{}", Self::BASE_URL, self.database_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Notion database")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Notion API error: {} - {}", status, body);
        }

        let db: NotionDatabaseResponse = response
            .json()
            .await
            .context("Failed to parse Notion database response")?;

        // Find status property (prefer one named "Status")
        let status_prop = db.properties.iter()
            .find(|(name, prop)| {
                prop.prop_type == "status" && name.to_lowercase() == "status"
            })
            .or_else(|| db.properties.iter()
                .find(|(_, prop)| prop.prop_type == "status"))
            .context("No status property found in database")?;

        let options = Self::categorize_options(status_prop.1)?;
        
        // Cache result
        {
            let mut cache = self.cached_status_options.lock().await;
            *cache = Some(options.clone());
        }

        Ok(options)
    }

    fn categorize_options(prop: &NotionPropertySchema) -> Result<StatusOptions> {
        let status = prop.status.as_ref()
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
            not_started_id,
            in_progress_id,
            done_id,
        })
    }

    /// Update page status property.
    pub async fn update_page_status(&self, page_id: &str, option_id: &str) -> Result<()> {
        let url = format!("{}/pages/{}", Self::BASE_URL, page_id);
        
        let body = serde_json::json!({
            "properties": {
                "Status": {
                    "status": { "id": option_id }
                }
            }
        });

        let response = self.client
            .patch(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to update Notion page status")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Notion API error: {} - {}", status, body);
        }

        Ok(())
    }

    /// Append blocks to a page.
    pub async fn append_blocks(&self, page_id: &str, blocks: Vec<NotionBlock>) -> Result<()> {
        let url = format!("{}/blocks/{}/children/append", Self::BASE_URL, page_id);
        
        let body = serde_json::json!({
            "children": blocks
        });

        let response = self.client
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

/// Optional Notion client wrapper.
pub struct OptionalNotionClient {
    client: Option<NotionClient>,
}

impl OptionalNotionClient {
    pub fn new(token: Option<&str>, database_id: Option<String>) -> Self {
        let client = token.and_then(|tok| {
            database_id.and_then(|db_id| NotionClient::new(tok, db_id).ok())
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

    pub async fn get_status_options(&self) -> Result<StatusOptions> {
        match &self.client {
            Some(c) => c.get_status_options().await,
            None => bail!("Notion not configured"),
        }
    }

    pub async fn update_page_status(&self, page_id: &str, option_id: &str) -> Result<()> {
        match &self.client {
            Some(c) => c.update_page_status(page_id, option_id).await,
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
```

---

## Phase 3: Unified Task Status

### 3.1 Update Agent Model (`src/agent/model.rs`)

Replace `asana_task_status` with unified field:

```rust
use crate::asana::AsanaTaskStatus;
use crate::notion::NotionTaskStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectMgmtTaskStatus {
    None,
    Asana(AsanaTaskStatus),
    Notion(NotionTaskStatus),
}

impl Default for ProjectMgmtTaskStatus {
    fn default() -> Self {
        ProjectMgmtTaskStatus::None
    }
}

impl ProjectMgmtTaskStatus {
    pub fn format_short(&self) -> String {
        match self {
            ProjectMgmtTaskStatus::None => "—".to_string(),
            ProjectMgmtTaskStatus::Asana(s) => s.format_short(),
            ProjectMgmtTaskStatus::Notion(s) => s.format_short(),
        }
    }

    pub fn is_linked(&self) -> bool {
        !matches!(self, ProjectMgmtTaskStatus::None)
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            ProjectMgmtTaskStatus::Asana(s) => s.url(),
            ProjectMgmtTaskStatus::Notion(s) => s.url(),
            ProjectMgmtTaskStatus::None => None,
        }
    }
}

pub struct Agent {
    // ... existing fields ...
    
    /// Project management task status (persisted across sessions)
    #[serde(default)]
    pub pm_task_status: ProjectMgmtTaskStatus,
}
```

### 3.2 Actions (`src/app/action.rs`)

Genericize actions for provider-agnostic handling:

```rust
pub enum Action {
    // Project management operations (provider-aware)
    AssignProjectTask {
        id: Uuid,
        url_or_id: String,
    },
    UpdateProjectTaskStatus {
        id: Uuid,
        status: crate::agent::ProjectMgmtTaskStatus,
    },
    OpenProjectTaskInBrowser {
        id: Uuid,
    },
    DeleteAgentAndCompleteTask {
        id: Uuid,
        append_notes: bool,  // From user confirmation
    },
    // ... other actions
}

pub enum InputMode {
    // ... existing ...
    AssignProjectTask,
    ConfirmDeleteWithTask,
}
```

---

## Phase 4: Main.rs Integration

### 4.1 Client Initialization

```rust
// Initialize Asana client
let asana_project_gid = state.settings.repo_config.project_mgmt.asana.project_gid.clone();
let asana_client = Arc::new(OptionalAsanaClient::new(
    Config::asana_token().as_deref(),
    asana_project_gid,
));

// Initialize Notion client
let notion_database_id = state.settings.repo_config.project_mgmt.notion.database_id.clone();
let notion_client = Arc::new(OptionalNotionClient::new(
    Config::notion_token().as_deref(),
    notion_database_id,
));
```

### 4.2 Provider-Aware Task Assignment

```rust
Action::AssignProjectTask { id, url_or_id } => {
    let provider = state.settings.repo_config.project_mgmt.provider;
    
    match provider {
        ProjectMgmtProvider::Asana => {
            let gid = parse_asana_task_gid(&url_or_id);
            // ... existing Asana logic
        }
        ProjectMgmtProvider::Notion => {
            let page_id = parse_notion_page_id(&url_or_id);
            let client = Arc::clone(&notion_client);
            let tx = action_tx.clone();
            
            tokio::spawn(async move {
                match client.get_page(&page_id).await {
                    Ok(page) => {
                        let status = if page.is_completed {
                            NotionTaskStatus::Completed {
                                page_id: page.id,
                                name: page.name,
                            }
                        } else {
                            NotionTaskStatus::NotStarted {
                                page_id: page.id,
                                name: page.name,
                                url: page.url,
                                status_option_id: page.status_id,
                            }
                        };
                        let _ = tx.send(Action::UpdateProjectTaskStatus {
                            id,
                            status: ProjectMgmtTaskStatus::Notion(status),
                        });
                    }
                    Err(e) => {
                        let status = NotionTaskStatus::Error {
                            page_id: page_id,
                            message: e.to_string(),
                        };
                        let _ = tx.send(Action::UpdateProjectTaskStatus {
                            id,
                            status: ProjectMgmtTaskStatus::Notion(status),
                        });
                    }
                }
            });
        }
    }
}
```

### 4.3 URL/ID Parser for Notion

```rust
/// Parse a Notion page ID from URL or bare ID.
fn parse_notion_page_id(input: &str) -> String {
    let trimmed = input.trim();
    
    // Handle URLs: https://www.notion.so/Page-Title-UUID or https://www.notion.so/UUID
    if trimmed.contains("notion.so") {
        // Remove trailing slash and fragment
        let url = trimmed.trim_end_matches('/').split('#').next().unwrap_or(trimmed);
        
        // Extract the last segment
        if let Some(last) = url.rsplit('/').next() {
            // Handle Page-Title-UUID format
            if let Some(uuid_part) = last.rsplit('-').next() {
                return clean_uuid(uuid_part);
            }
            return clean_uuid(last);
        }
    }
    
    clean_uuid(trimmed)
}

fn clean_uuid(s: &str) -> String {
    // Remove dashes and ensure lowercase
    s.replace('-', "").to_lowercase()
}
```

### 4.4 Background Polling

```rust
/// Background task to poll Notion for task status updates.
async fn poll_notion_tasks(
    notion_rx: watch::Receiver<Vec<(Uuid, String)>>,
    notion_client: Arc<OptionalNotionClient>,
    tx: mpsc::UnboundedSender<Action>,
    refresh_secs: u64,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(refresh_secs)).await;

        let tasks = notion_rx.borrow().clone();
        for (id, page_id) in tasks {
            match notion_client.get_page(&page_id).await {
                Ok(page) => {
                    let status = if page.is_completed {
                        NotionTaskStatus::Completed {
                            page_id: page.id,
                            name: page.name,
                        }
                    } else if page.status_name.as_deref() == Some("In Progress") {
                        NotionTaskStatus::InProgress {
                            page_id: page.id,
                            name: page.name,
                            url: page.url,
                            status_option_id: page.status_id.unwrap_or_default(),
                        }
                    } else {
                        NotionTaskStatus::NotStarted {
                            page_id: page.id,
                            name: page.name,
                            url: page.url,
                            status_option_id: page.status_id.unwrap_or_default(),
                        }
                    };
                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                        id,
                        status: ProjectMgmtTaskStatus::Notion(status),
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to poll Notion page {}: {}", page_id, e);
                }
            }
        }
    }
}
```

### 4.5 Auto-Transition on Agent Status Change

```rust
// When agent transitions to Running
if matches!(new_status, AgentStatus::Running) {
    if let Some(agent) = state.agents.get_mut(&id) {
        match &agent.pm_task_status {
            ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted { gid, .. }) => {
                // Move Asana task to In Progress
            }
            ProjectMgmtTaskStatus::Notion(NotionTaskStatus::NotStarted { page_id, status_option_id, .. }) => {
                // Move Notion task to In Progress
                let client = Arc::clone(&notion_client);
                let tx = action_tx.clone();
                let page_id = page_id.clone();
                tokio::spawn(async move {
                    if let Ok(opts) = client.get_status_options().await {
                        if let Some(in_progress_id) = opts.in_progress_id {
                            let _ = client.update_page_status(&page_id, &in_progress_id).await;
                        }
                    }
                });
            }
            _ => {}
        }
    }
}
```

---

## Phase 5: Completion Notes

### 5.1 Notes Format

When agent completes and user opts to append notes:

```markdown
## Agent Completed - 2024-01-15 14:32

**Branch:** feature/add-notion-integration
**Task:** Add Notion integration

### Summary
Implemented Notion API client with status tracking and page updates.

### Key Changes
- Added src/notion/ module
- Updated agent model with ProjectMgmtTaskStatus
- Added provider dropdown in settings
```

### 5.2 Block Types

```rust
pub struct NotionBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub heading_2: Option<NotionTextContent>,
    pub paragraph: Option<NotionTextContent>,
    pub bulleted_list_item: Option<NotionTextContent>,
}

pub struct NotionTextContent {
    pub rich_text: Vec<NotionRichText>,
}

impl NotionBlock {
    pub fn heading_2(text: &str) -> Self {
        Self {
            block_type: "heading_2".to_string(),
            heading_2: Some(NotionTextContent::from_text(text)),
            paragraph: None,
            bulleted_list_item: None,
        }
    }

    pub fn paragraph(text: &str) -> Self {
        Self {
            block_type: "paragraph".to_string(),
            heading_2: None,
            paragraph: Some(NotionTextContent::from_text(text)),
            bulleted_list_item: None,
        }
    }

    pub fn bullet(text: &str) -> Self {
        Self {
            block_type: "bulleted_list_item".to_string(),
            heading_2: None,
            paragraph: None,
            bulleted_list_item: Some(NotionTextContent::from_text(text)),
        }
    }
}
```

---

## Phase 6: UI Updates

### 6.1 Agent List Column

Replace "Asana" column with "Tasks" column:

```rust
// Header
["", "S", "Name", "Status", "Active", "Rate", "Tasks", "MR", "Pipeline", "Tasks", "Note"]

// Render function
fn format_pm_status(&self, agent: &Agent) -> (String, Style) {
    let text = agent.pm_task_status.format_short();
    let style = match &agent.pm_task_status {
        ProjectMgmtTaskStatus::None => Style::default().fg(Color::DarkGray),
        ProjectMgmtTaskStatus::Asana(s) => match s {
            AsanaTaskStatus::NotStarted { .. } => Style::default().fg(Color::White),
            AsanaTaskStatus::InProgress { .. } => Style::default().fg(Color::LightBlue),
            AsanaTaskStatus::Completed { .. } => Style::default().fg(Color::Green),
            AsanaTaskStatus::Error { .. } => Style::default().fg(Color::Red),
            _ => Style::default().fg(Color::DarkGray),
        },
        ProjectMgmtTaskStatus::Notion(s) => match s {
            NotionTaskStatus::NotStarted { .. } => Style::default().fg(Color::White),
            NotionTaskStatus::InProgress { .. } => Style::default().fg(Color::LightBlue),
            NotionTaskStatus::Completed { .. } => Style::default().fg(Color::Green),
            NotionTaskStatus::Error { .. } => Style::default().fg(Color::Red),
            _ => Style::default().fg(Color::DarkGray),
        },
    };
    (text, style)
}
```

### 6.2 Status Bar

Keep `[a] task` shortcut (provider-aware based on config):

```rust
let shortcuts = vec![
    ("n", "new"),
    ("d", "del"),
    ("Enter", "attach"),
    ("s", "summary"),
    ("m", "merge"),
    ("p", "push"),
    ("a", "task"),  // Opens task assignment for configured provider
    ("N", "note"),
    ("R", "refresh"),
    ("?", "help"),
    ("q", "quit"),
];
```

### 6.3 Settings Modal

Update "Project Mgmt" tab to follow Git pattern:

```rust
pub enum SettingsField {
    // ... existing ...
    
    // Project Management
    ProjectMgmtProvider,
    
    // Asana fields (shown when provider = Asana)
    AsanaProjectGid,
    AsanaInProgressGid,
    AsanaDoneGid,
    
    // Notion fields (shown when provider = Notion)
    NotionDatabaseId,
    NotionStatusProperty,
    NotionInProgressOption,
    NotionDoneOption,
}

impl SettingsItem {
    pub fn all_for_tab(tab: SettingsTab, provider: GitProvider, pm_provider: ProjectMgmtProvider) -> Vec<SettingsItem> {
        match tab {
            SettingsTab::ProjectMgmt => {
                let mut items = vec![
                    SettingsItem::Category(SettingsCategory::ProjectMgmt),
                    SettingsItem::Field(SettingsField::ProjectMgmtProvider),
                ];
                
                match pm_provider {
                    ProjectMgmtProvider::Asana => {
                        items.push(SettingsItem::Field(SettingsField::AsanaProjectGid));
                        items.push(SettingsItem::Field(SettingsField::AsanaInProgressGid));
                        items.push(SettingsItem::Field(SettingsField::AsanaDoneGid));
                        items.push(Self::render_token_status_line(
                            "ASANA_TOKEN",
                            Config::asana_token().is_some(),
                        ));
                    }
                    ProjectMgmtProvider::Notion => {
                        items.push(SettingsItem::Field(SettingsField::NotionDatabaseId));
                        items.push(SettingsItem::Field(SettingsField::NotionStatusProperty));
                        items.push(SettingsItem::Field(SettingsField::NotionInProgressOption));
                        items.push(SettingsItem::Field(SettingsField::NotionDoneOption));
                        items.push(Self::render_token_status_line(
                            "NOTION_TOKEN",
                            Config::notion_token().is_some(),
                        ));
                    }
                }
                items
            }
            // ... other tabs
        }
    }
}
```

---

## Phase 7: Environment Variables

| Variable | Description | Required For |
|----------|-------------|--------------|
| `NOTION_TOKEN` | Notion integration secret | Notion provider |
| `ASANA_TOKEN` | Asana personal access token | Asana provider |

Create tokens:
- **Notion:** https://www.notion.so/my-integrations
- **Asana:** https://app.asana.com/0/developer-console

---

## Implementation Order

| Step | Files | Description |
|------|-------|-------------|
| 1 | `src/app/config.rs` | Add `ProjectMgmtProvider`, `NotionConfig`, restructure `RepoConfig` |
| 2 | `src/notion/mod.rs`, `types.rs`, `client.rs` | Core Notion types and API client |
| 3 | `src/agent/model.rs` | Replace `asana_task_status` with `pm_task_status` |
| 4 | `src/app/action.rs` | Genericize actions for provider-aware handling |
| 5 | `src/app/state.rs` | Add `NotionDatabaseId`, `NotionStatusProperty` settings fields |
| 6 | `src/main.rs` | Client init, polling, provider-aware action handling |
| 7 | `src/ui/components/agent_list.rs` | Replace "Asana" with "Tasks" column |
| 8 | `src/ui/components/settings_modal.rs` | Provider dropdown + conditional fields |
| 9 | `src/ui/components/project_setup.rs` | Add project management setup fields |

---

## API Reference

### Notion API

- **Base URL:** `https://api.notion.com/v1/`
- **Headers:**
  - `Authorization: Bearer {token}`
  - `Notion-Version: 2022-06-28`
  - `Content-Type: application/json`
- **Key Endpoints:**
  - `GET /databases/{database_id}` - Retrieve database schema
  - `GET /pages/{page_id}` - Retrieve page
  - `PATCH /pages/{page_id}` - Update page properties
  - `PATCH /blocks/{block_id}/children/append` - Append content blocks

---

## Key Differences: Asana vs Notion

| Aspect | Asana | Notion |
|--------|-------|--------|
| Status model | Sections (fixed GIDs) | Status property (flexible options) |
| Status discovery | Explicit section GIDs | Auto-detect from schema + overrides |
| Page content | No write access | Can append completion notes |
| URL format | Multiple patterns | UUID-based (single pattern) |
| Database required | No | Yes (for status option discovery) |

---

## Decisions Summary

| Question | Decision |
|----------|----------|
| UI Layout | Single "Tasks" column, provider dropdown |
| Keyboard shortcut | 'a' for task (provider-aware) |
| Status detection | Hybrid (auto-detect + config overrides) |
| Database ID | Required for Notion |
| Completion notes | Configurable per-task when linking |
| Migration | Auto-migrate existing Asana users |
| Settings UI | Same pattern as Git (dropdown + conditional fields) |
