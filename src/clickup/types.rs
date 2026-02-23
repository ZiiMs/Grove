use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ClickUpTaskStatus {
    #[default]
    None,
    NotStarted {
        id: String,
        name: String,
        url: String,
        status: String,
        is_subtask: bool,
    },
    InProgress {
        id: String,
        name: String,
        url: String,
        status: String,
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

impl ClickUpTaskStatus {
    pub fn format_short(&self) -> String {
        match self {
            ClickUpTaskStatus::None => "—".to_string(),
            ClickUpTaskStatus::NotStarted { name, .. } => truncate(name, 14),
            ClickUpTaskStatus::InProgress { name, .. } => truncate(name, 14),
            ClickUpTaskStatus::Completed { name, .. } => truncate(name, 14),
            ClickUpTaskStatus::Error { message, .. } => format!("err: {}", truncate(message, 10)),
        }
    }

    /// Display string for the status name column.
    pub fn format_status_name(&self) -> String {
        match self {
            ClickUpTaskStatus::None => "—".to_string(),
            ClickUpTaskStatus::NotStarted { status, .. }
            | ClickUpTaskStatus::InProgress { status, .. } => truncate(status, 10),
            ClickUpTaskStatus::Completed { .. } => "Done".to_string(),
            ClickUpTaskStatus::Error { .. } => "Error".to_string(),
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            ClickUpTaskStatus::None => None,
            ClickUpTaskStatus::NotStarted { id, .. }
            | ClickUpTaskStatus::InProgress { id, .. }
            | ClickUpTaskStatus::Completed { id, .. }
            | ClickUpTaskStatus::Error { id, .. } => Some(id),
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            ClickUpTaskStatus::NotStarted { url, .. }
            | ClickUpTaskStatus::InProgress { url, .. } => Some(url),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            ClickUpTaskStatus::None => None,
            ClickUpTaskStatus::NotStarted { name, .. }
            | ClickUpTaskStatus::InProgress { name, .. }
            | ClickUpTaskStatus::Completed { name, .. } => Some(name),
            ClickUpTaskStatus::Error { message, .. } => Some(message),
        }
    }

    pub fn is_linked(&self) -> bool {
        !matches!(self, ClickUpTaskStatus::None)
    }

    pub fn is_subtask(&self) -> bool {
        match self {
            ClickUpTaskStatus::None => false,
            ClickUpTaskStatus::NotStarted { is_subtask, .. }
            | ClickUpTaskStatus::InProgress { is_subtask, .. }
            | ClickUpTaskStatus::Completed { is_subtask, .. } => *is_subtask,
            ClickUpTaskStatus::Error { .. } => false,
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

#[derive(Debug, Deserialize)]
pub struct ClickUpTaskListResponse {
    pub tasks: Vec<ClickUpTaskData>,
}

#[derive(Debug, Deserialize)]
pub struct ClickUpTaskResponse {
    #[serde(flatten)]
    pub data: ClickUpTaskData,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpTaskData {
    pub id: String,
    pub name: String,
    pub status: ClickUpStatusData,
    pub url: Option<String>,
    pub parent: Option<String>,
    #[serde(default)]
    pub subtasks: Vec<ClickUpTaskData>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpStatusData {
    pub status: String,
    #[serde(rename = "type")]
    pub status_type: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClickUpTaskSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub status_type: String,
    pub url: Option<String>,
    pub parent_id: Option<String>,
    pub has_children: bool,
}

impl From<ClickUpTaskData> for ClickUpTaskSummary {
    fn from(data: ClickUpTaskData) -> Self {
        Self {
            id: data.id,
            name: data.name,
            status: data.status.status,
            status_type: data.status.status_type,
            url: data.url,
            parent_id: data.parent,
            has_children: !data.subtasks.is_empty(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ClickUpListResponse {
    #[serde(flatten)]
    pub data: ClickUpListData,
}

#[derive(Debug, Deserialize)]
pub struct ClickUpListData {
    pub id: String,
    pub name: String,
    pub statuses: Vec<ClickUpStatusData>,
}

#[derive(Debug, Clone)]
pub struct StatusOption {
    pub status: String,
    pub status_type: String,
    pub color: Option<String>,
}

impl From<ClickUpStatusData> for StatusOption {
    fn from(data: ClickUpStatusData) -> Self {
        Self {
            status: data.status,
            status_type: data.status_type,
            color: data.color,
        }
    }
}

pub fn parse_clickup_task_id(input: &str) -> String {
    let trimmed = input.trim();

    if trimmed.contains("clickup.com") {
        if let Some(last) = trimmed.trim_end_matches('/').rsplit('/').next() {
            return last.to_string();
        }
    }

    trimmed.to_string()
}

#[derive(Debug, Deserialize)]
pub struct ClickUpTeamsResponse {
    pub teams: Vec<ClickUpTeam>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpTeam {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClickUpSpacesResponse {
    pub spaces: Vec<ClickUpSpace>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpSpace {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub features: Option<ClickUpSpaceFeatures>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpSpaceFeatures {
    pub due_dates: Option<ClickUpSpaceFeatureEnabled>,
    pub sprints: Option<ClickUpSpaceFeatureEnabled>,
    pub time_tracking: Option<ClickUpSpaceFeatureEnabled>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpSpaceFeatureEnabled {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct ClickUpFoldersResponse {
    pub folders: Vec<ClickUpFolder>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpFolder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub lists: Vec<ClickUpList>,
    pub hidden: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ClickUpListsResponse {
    pub lists: Vec<ClickUpList>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpList {
    pub id: String,
    pub name: String,
    pub space: Option<ClickUpSpaceRef>,
    pub folder: Option<ClickUpFolderRef>,
    pub archived: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpSpaceRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClickUpFolderRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ClickUpListWithLocation {
    pub id: String,
    pub name: String,
    pub space_name: String,
    pub folder_name: Option<String>,
}

impl ClickUpListWithLocation {
    pub fn display_path(&self) -> String {
        match &self.folder_name {
            Some(folder) => format!("{} > {} > {}", self.space_name, folder, self.name),
            None => format!("{} > {}", self.space_name, self.name),
        }
    }
}
