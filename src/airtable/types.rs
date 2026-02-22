use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AirtableTaskStatus {
    #[default]
    None,
    NotStarted {
        id: String,
        name: String,
        url: String,
        is_subtask: bool,
    },
    InProgress {
        id: String,
        name: String,
        url: String,
        is_subtask: bool,
    },
    Completed {
        id: String,
        name: String,
        is_subtask: bool,
    },
    Error {
        id: String,
        message: String,
    },
}

impl AirtableTaskStatus {
    pub fn format_short(&self) -> String {
        match self {
            AirtableTaskStatus::None => "—".to_string(),
            AirtableTaskStatus::NotStarted { name, .. } => truncate(name, 14),
            AirtableTaskStatus::InProgress { name, .. } => truncate(name, 14),
            AirtableTaskStatus::Completed { name, .. } => truncate(name, 14),
            AirtableTaskStatus::Error { message, .. } => format!("err: {}", truncate(message, 10)),
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            AirtableTaskStatus::None => None,
            AirtableTaskStatus::NotStarted { id, .. }
            | AirtableTaskStatus::InProgress { id, .. }
            | AirtableTaskStatus::Completed { id, .. }
            | AirtableTaskStatus::Error { id, .. } => Some(id),
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            AirtableTaskStatus::NotStarted { url, .. }
            | AirtableTaskStatus::InProgress { url, .. } => Some(url),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            AirtableTaskStatus::None => None,
            AirtableTaskStatus::NotStarted { name, .. }
            | AirtableTaskStatus::InProgress { name, .. }
            | AirtableTaskStatus::Completed { name, .. } => Some(name),
            AirtableTaskStatus::Error { message, .. } => Some(message),
        }
    }

    pub fn is_linked(&self) -> bool {
        !matches!(self, AirtableTaskStatus::None)
    }

    pub fn is_subtask(&self) -> bool {
        match self {
            AirtableTaskStatus::None => false,
            AirtableTaskStatus::NotStarted { is_subtask, .. }
            | AirtableTaskStatus::InProgress { is_subtask, .. }
            | AirtableTaskStatus::Completed { is_subtask, .. } => *is_subtask,
            AirtableTaskStatus::Error { .. } => false,
        }
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
pub struct StatusOption {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct AirtableTaskSummary {
    pub id: String,
    pub name: String,
    pub status: Option<String>,
    pub url: String,
    pub parent_id: Option<String>,
    pub has_children: bool,
}

#[derive(Debug, Deserialize)]
pub struct AirtableRecordsResponse {
    pub records: Vec<AirtableRecord>,
    #[serde(default)]
    pub offset: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AirtableRecord {
    pub id: String,
    pub fields: AirtableFields,
}

#[derive(Debug, Deserialize)]
pub struct AirtableFields {
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub parent: Option<Vec<ParentRecord>>,
}

#[derive(Debug, Deserialize)]
pub struct ParentRecord {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct AirtableTableSchema {
    pub tables: Vec<AirtableTable>,
}

#[derive(Debug, Deserialize)]
pub struct AirtableTable {
    pub name: String,
    pub fields: Vec<AirtableFieldSchema>,
}

#[derive(Debug, Deserialize)]
pub struct AirtableFieldSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub options: Option<AirtableFieldOptions>,
}

#[derive(Debug, Deserialize)]
pub struct AirtableFieldOptions {
    #[serde(default)]
    pub choices: Vec<AirtableChoice>,
}

#[derive(Debug, Deserialize)]
pub struct AirtableChoice {
    pub name: String,
}
