use std::sync::Arc;

use anyhow::Result;

use crate::app::StatusOption;
use crate::core::projects::airtable::OptionalAirtableClient;
use crate::core::projects::asana::OptionalAsanaClient;
use crate::core::projects::clickup::OptionalClickUpClient;
use crate::core::projects::linear::OptionalLinearClient;
use crate::core::projects::notion::OptionalNotionClient;

pub struct ProjectClients {
    pub notion: Arc<OptionalNotionClient>,
    pub asana: Arc<OptionalAsanaClient>,
    pub clickup: Arc<OptionalClickUpClient>,
    pub airtable: Arc<OptionalAirtableClient>,
    pub linear: Arc<OptionalLinearClient>,
}

#[derive(Debug)]
pub enum FetchStatusError {
    NotConfigured {
        provider: &'static str,
        message: &'static str,
    },
    NotLinked {
        provider: &'static str,
        message: &'static str,
    },
    ApiError {
        provider: &'static str,
        error: anyhow::Error,
    },
}

impl FetchStatusError {
    pub fn provider(&self) -> &'static str {
        match self {
            Self::NotConfigured { provider, .. } => provider,
            Self::NotLinked { provider, .. } => provider,
            Self::ApiError { provider, .. } => provider,
        }
    }

    pub fn display_message(&self) -> String {
        match self {
            Self::NotConfigured { message, .. } => message.to_string(),
            Self::NotLinked { message, .. } => message.to_string(),
            Self::ApiError { error, .. } => format!("Failed to load status options: {}", error),
        }
    }
}

pub async fn fetch_status_options(
    pm_status: &crate::agent::ProjectMgmtTaskStatus,
    clients: &ProjectClients,
    is_subtask: bool,
) -> Result<Vec<StatusOption>, FetchStatusError> {
    use crate::agent::ProjectMgmtTaskStatus;

    match pm_status {
        ProjectMgmtTaskStatus::Notion(notion_status) => {
            fetch_notion_status_options(notion_status, &clients.notion).await
        }
        ProjectMgmtTaskStatus::Asana(asana_status) => {
            fetch_asana_status_options(asana_status, &clients.asana, is_subtask).await
        }
        ProjectMgmtTaskStatus::ClickUp(clickup_status) => {
            fetch_clickup_status_options(clickup_status, &clients.clickup).await
        }
        ProjectMgmtTaskStatus::Airtable(airtable_status) => {
            fetch_airtable_status_options(airtable_status, &clients.airtable).await
        }
        ProjectMgmtTaskStatus::Linear(linear_status) => {
            fetch_linear_status_options(linear_status, &clients.linear).await
        }
        ProjectMgmtTaskStatus::None => Err(FetchStatusError::NotLinked {
            provider: "None",
            message: "No task linked to this agent",
        }),
    }
}

async fn fetch_notion_status_options(
    notion_status: &crate::core::projects::notion::NotionTaskStatus,
    client: &Arc<OptionalNotionClient>,
) -> Result<Vec<StatusOption>, FetchStatusError> {
    if !client.is_configured().await {
        return Err(FetchStatusError::NotConfigured {
            provider: "Notion",
            message: "Notion not configured. Set NOTION_TOKEN and database_id.",
        });
    }

    let page_id = notion_status.page_id();
    if page_id.is_none() || page_id.map(|p| p.is_empty()).unwrap_or(true) {
        return Err(FetchStatusError::NotLinked {
            provider: "Notion",
            message: "No Notion page linked to this task",
        });
    }

    let opts = client
        .get_status_options()
        .await
        .map_err(|e| FetchStatusError::ApiError {
            provider: "Notion",
            error: e,
        })?;

    let options: Vec<StatusOption> = opts
        .all_options
        .into_iter()
        .map(|o| StatusOption {
            id: o.id,
            name: o.name,
        })
        .collect();

    Ok(options)
}

async fn fetch_asana_status_options(
    asana_status: &crate::core::projects::asana::AsanaTaskStatus,
    client: &Arc<OptionalAsanaClient>,
    is_subtask: bool,
) -> Result<Vec<StatusOption>, FetchStatusError> {
    if !client.is_configured().await {
        return Err(FetchStatusError::NotConfigured {
            provider: "Asana",
            message: "Asana not configured. Set ASANA_TOKEN and project_gid.",
        });
    }

    if asana_status.gid().is_none() {
        return Err(FetchStatusError::NotLinked {
            provider: "Asana",
            message: "No Asana task linked to this agent",
        });
    }

    if is_subtask {
        let provider_statuses =
            client
                .fetch_statuses()
                .await
                .map_err(|e| FetchStatusError::ApiError {
                    provider: "Asana",
                    error: e,
                })?;

        let options: Vec<StatusOption> = provider_statuses
            .children
            .unwrap_or_default()
            .into_iter()
            .map(|s| StatusOption {
                id: s.id,
                name: s.name,
            })
            .collect();

        Ok(options)
    } else {
        let sections = client
            .get_sections()
            .await
            .map_err(|e| FetchStatusError::ApiError {
                provider: "Asana",
                error: e,
            })?;

        let options: Vec<StatusOption> = sections
            .into_iter()
            .map(|s| StatusOption {
                id: s.gid,
                name: s.name,
            })
            .collect();

        Ok(options)
    }
}

async fn fetch_clickup_status_options(
    clickup_status: &crate::core::projects::clickup::ClickUpTaskStatus,
    client: &Arc<OptionalClickUpClient>,
) -> Result<Vec<StatusOption>, FetchStatusError> {
    if !client.is_configured().await {
        return Err(FetchStatusError::NotConfigured {
            provider: "ClickUp",
            message: "ClickUp not configured. Set CLICKUP_TOKEN and list_id.",
        });
    }

    if clickup_status.id().is_none() {
        return Err(FetchStatusError::NotLinked {
            provider: "ClickUp",
            message: "No ClickUp task linked to this agent",
        });
    }

    let statuses = client
        .get_statuses()
        .await
        .map_err(|e| FetchStatusError::ApiError {
            provider: "ClickUp",
            error: e,
        })?;

    let options: Vec<StatusOption> = statuses
        .into_iter()
        .map(|s| StatusOption {
            id: s.status.clone(),
            name: s.status,
        })
        .collect();

    Ok(options)
}

async fn fetch_airtable_status_options(
    airtable_status: &crate::core::projects::airtable::AirtableTaskStatus,
    client: &Arc<OptionalAirtableClient>,
) -> Result<Vec<StatusOption>, FetchStatusError> {
    if !client.is_configured().await {
        return Err(FetchStatusError::NotConfigured {
            provider: "Airtable",
            message: "Airtable not configured. Set AIRTABLE_TOKEN, base_id, and table_name.",
        });
    }

    if airtable_status.id().is_none() {
        return Err(FetchStatusError::NotLinked {
            provider: "Airtable",
            message: "No Airtable record linked to this agent",
        });
    }

    let opts = client
        .get_status_options()
        .await
        .map_err(|e| FetchStatusError::ApiError {
            provider: "Airtable",
            error: e,
        })?;

    let options: Vec<StatusOption> = opts
        .into_iter()
        .map(|o| StatusOption {
            id: o.name.clone(),
            name: o.name,
        })
        .collect();

    Ok(options)
}

async fn fetch_linear_status_options(
    linear_status: &crate::core::projects::linear::LinearTaskStatus,
    client: &Arc<OptionalLinearClient>,
) -> Result<Vec<StatusOption>, FetchStatusError> {
    if !client.is_configured().await {
        return Err(FetchStatusError::NotConfigured {
            provider: "Linear",
            message: "Linear not configured. Set LINEAR_TOKEN and team_id.",
        });
    }

    if linear_status.id().is_none() {
        return Err(FetchStatusError::NotLinked {
            provider: "Linear",
            message: "No Linear issue linked to this agent",
        });
    }

    let states = client
        .get_workflow_states()
        .await
        .map_err(|e| FetchStatusError::ApiError {
            provider: "Linear",
            error: e,
        })?;

    let options: Vec<StatusOption> = states
        .into_iter()
        .map(|s| StatusOption {
            id: s.id,
            name: s.name,
        })
        .collect();

    Ok(options)
}
