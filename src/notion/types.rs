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
    pub fn format_short(&self) -> String {
        match self {
            NotionTaskStatus::None => "—".to_string(),
            NotionTaskStatus::NotStarted { name, .. } => truncate(name, 14),
            NotionTaskStatus::InProgress { name, .. } => truncate(name, 14),
            NotionTaskStatus::Completed { name, .. } => truncate(name, 14),
            NotionTaskStatus::Error { message, .. } => format!("err: {}", truncate(message, 10)),
        }
    }

    pub fn page_id(&self) -> Option<&str> {
        match self {
            NotionTaskStatus::None => None,
            NotionTaskStatus::NotStarted { page_id, .. }
            | NotionTaskStatus::InProgress { page_id, .. }
            | NotionTaskStatus::Completed { page_id, .. }
            | NotionTaskStatus::Error { page_id, .. } => Some(page_id),
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            NotionTaskStatus::NotStarted { url, .. } | NotionTaskStatus::InProgress { url, .. } => {
                Some(url)
            }
            _ => None,
        }
    }

    pub fn is_linked(&self) -> bool {
        !matches!(self, NotionTaskStatus::None)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max - 1).collect();
        format!("{}…", t)
    }
}

#[derive(Debug, Clone)]
pub struct NotionPageData {
    pub id: String,
    pub name: String,
    pub url: String,
    pub status_id: Option<String>,
    pub status_name: Option<String>,
    pub is_completed: bool,
}

#[derive(Debug, Deserialize)]
pub struct NotionPageResponse {
    pub id: String,
    pub url: String,
    pub properties: NotionProperties,
}

#[derive(Debug, Deserialize)]
pub struct NotionProperties {
    #[serde(rename = "Name")]
    pub name: Option<NotionTitleProperty>,
    #[serde(rename = "title")]
    pub title: Option<NotionTitleProperty>,
    #[serde(rename = "Status")]
    pub status: Option<NotionStatusPropertyValue>,
}

impl NotionProperties {
    pub fn get_title(&self) -> Option<String> {
        self.name
            .as_ref()
            .or(self.title.as_ref())
            .and_then(|t| t.title.first().map(|rt| rt.plain_text.clone()))
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
        let name = page
            .properties
            .get_title()
            .unwrap_or_else(|| "Untitled".to_string());
        let status = page.properties.status.as_ref();
        let status_id = status.and_then(|s| s.status.as_ref().map(|opt| opt.id.clone()));
        let status_name = status.and_then(|s| s.status.as_ref().map(|opt| opt.name.clone()));
        let is_completed = status_name
            .as_ref()
            .map(|n| {
                let lower = n.to_lowercase();
                lower.contains("done") || lower.contains("complete")
            })
            .unwrap_or(false);

        NotionPageData {
            id: page.id,
            name,
            url: page.url,
            status_id,
            status_name,
            is_completed,
        }
    }
}
