use crate::core::projects::truncate_with_ellipsis;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotionTaskStatus {
    #[default]
    None,
    Linked {
        page_id: String,
        name: String,
        url: String,
        status_option_id: String,
        status_name: String,
    },
    Error {
        page_id: String,
        message: String,
    },
}

impl NotionTaskStatus {
    pub fn format_short(&self) -> String {
        match self {
            NotionTaskStatus::None => "—".to_string(),
            NotionTaskStatus::Linked { name, .. } => truncate_with_ellipsis(name, 14),
            NotionTaskStatus::Error { message, .. } => {
                format!("err: {}", truncate_with_ellipsis(message, 10))
            }
        }
    }

    pub fn format_status_name(&self) -> String {
        match self {
            NotionTaskStatus::None => "—".to_string(),
            NotionTaskStatus::Linked { status_name, .. } => truncate_with_ellipsis(status_name, 10),
            NotionTaskStatus::Error { .. } => "Error".to_string(),
        }
    }

    pub fn page_id(&self) -> Option<&str> {
        match self {
            NotionTaskStatus::None => None,
            NotionTaskStatus::Linked { page_id, .. } | NotionTaskStatus::Error { page_id, .. } => {
                Some(page_id)
            }
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            NotionTaskStatus::Linked { url, .. } => Some(url),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            NotionTaskStatus::None => None,
            NotionTaskStatus::Linked { name, .. } => Some(name),
            NotionTaskStatus::Error { message, .. } => Some(message),
        }
    }

    pub fn is_linked(&self) -> bool {
        !matches!(self, NotionTaskStatus::None)
    }

    pub fn is_completed(&self) -> bool {
        match self {
            NotionTaskStatus::Linked { status_name, .. } => {
                let lower = status_name.to_lowercase();
                lower.contains("done") || lower.contains("complete")
            }
            _ => false,
        }
    }

    pub fn is_in_progress(&self) -> bool {
        match self {
            NotionTaskStatus::Linked { status_name, .. } => {
                status_name.to_lowercase().contains("progress")
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NotionPageData {
    pub id: String,
    pub name: String,
    pub url: String,
    pub status_id: Option<String>,
    pub status_name: Option<String>,
    pub parent_page_id: Option<String>,
    pub related_task_ids: Vec<String>,
}

impl NotionPageData {
    pub fn is_completed(&self) -> bool {
        self.status_name
            .as_ref()
            .map(|n| {
                let lower = n.to_lowercase();
                lower.contains("done") || lower.contains("complete")
            })
            .unwrap_or(false)
    }
}

#[derive(Debug, Deserialize)]
pub struct NotionPageResponse {
    pub id: String,
    pub url: String,
    pub parent: Option<NotionParent>,
    pub properties: NotionProperties,
}

#[derive(Debug, Deserialize)]
pub struct NotionParent {
    #[serde(rename = "type")]
    pub parent_type: Option<String>,
    pub page_id: Option<String>,
    pub database_id: Option<String>,
}

impl NotionParent {
    pub fn get_page_id(&self) -> Option<&str> {
        self.page_id.as_deref()
    }
}

#[derive(Debug, Deserialize)]
pub struct NotionProperties {
    #[serde(rename = "Name")]
    pub name: Option<NotionTitleProperty>,
    #[serde(rename = "title")]
    pub title: Option<NotionTitleProperty>,
    #[serde(rename = "Status")]
    pub status: Option<NotionStatusPropertyValue>,
    #[serde(rename = "Task")]
    pub task: Option<NotionTitleProperty>,
    #[serde(rename = "Tasks")]
    pub tasks_relation: Option<NotionRelationPropertyValue>,
    #[serde(flatten)]
    pub other: std::collections::HashMap<String, serde_json::Value>,
}

impl NotionProperties {
    pub fn get_title(&self) -> Option<String> {
        let title_prop = self
            .name
            .as_ref()
            .or(self.title.as_ref())
            .or(self.task.as_ref());

        if let Some(prop) = title_prop {
            return prop.title.first().map(|rt| rt.plain_text.clone());
        }

        for (key, value) in &self.other {
            if let Some(obj) = value.as_object() {
                if obj.get("type").and_then(|v| v.as_str()) == Some("title") {
                    if let Some(title_arr) = obj.get("title").and_then(|v| v.as_array()) {
                        if let Some(first) = title_arr.first() {
                            if let Some(text) = first.get("plain_text").and_then(|v| v.as_str()) {
                                tracing::debug!("Found title in property '{}': {}", key, text);
                                return Some(text.to_string());
                            }
                        }
                    }
                }
            }
        }

        None
    }
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
pub struct NotionRelationPropertyValue {
    pub relation: Vec<NotionRelationItem>,
}

#[derive(Debug, Deserialize)]
pub struct NotionRelationItem {
    pub id: String,
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
pub struct NotionStatusSchema {
    pub options: Vec<NotionStatusOption>,
}

#[derive(Debug, Serialize)]
pub struct NotionBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading_2: Option<NotionTextContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<NotionTextContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bulleted_list_item: Option<NotionTextContent>,
}

#[derive(Debug, Serialize)]
pub struct NotionTextContent {
    pub rich_text: Vec<NotionRichTextInput>,
}

#[derive(Debug, Serialize)]
pub struct NotionRichTextInput {
    #[serde(rename = "type")]
    pub text_type: String,
    pub text: NotionTextDetail,
}

#[derive(Debug, Serialize)]
pub struct NotionTextDetail {
    pub content: String,
}

impl NotionTextContent {
    pub fn from_text(text: &str) -> Self {
        Self {
            rich_text: vec![NotionRichTextInput {
                text_type: "text".to_string(),
                text: NotionTextDetail {
                    content: text.to_string(),
                },
            }],
        }
    }
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

#[derive(Debug, Deserialize)]
pub struct NotionQueryResponse {
    pub results: Vec<NotionPageResponse>,
    pub has_more: bool,
}

impl From<NotionPageResponse> for NotionPageData {
    fn from(page: NotionPageResponse) -> Self {
        let name = page.properties.get_title().unwrap_or_else(|| {
            tracing::warn!(
                "No title found for page {}, properties: {:?}",
                page.id,
                page.properties.other.keys().collect::<Vec<_>>()
            );
            "Untitled".to_string()
        });

        let parent_page_id = page.parent.as_ref().and_then(|p| p.page_id.clone());

        let status = page.properties.status.as_ref();
        let status_id = status.and_then(|s| s.status.as_ref().map(|opt| opt.id.clone()));
        let status_name = status.and_then(|s| s.status.as_ref().map(|opt| opt.name.clone()));

        let related_task_ids: Vec<String> = page
            .properties
            .tasks_relation
            .as_ref()
            .map(|r| r.relation.iter().map(|i| i.id.clone()).collect())
            .unwrap_or_default();

        NotionPageData {
            id: page.id,
            name,
            url: page.url,
            status_id,
            status_name,
            parent_page_id,
            related_task_ids,
        }
    }
}
