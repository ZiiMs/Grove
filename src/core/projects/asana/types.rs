use crate::core::projects::truncate_with_ellipsis;
use serde::{Deserialize, Serialize};

/// Represents the Asana task tracking status for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AsanaTaskStatus {
    #[default]
    None,
    NotStarted {
        gid: String,
        name: String,
        url: String,
        is_subtask: bool,
        status_name: String,
    },
    InProgress {
        gid: String,
        name: String,
        url: String,
        is_subtask: bool,
        status_name: String,
    },
    Completed {
        gid: String,
        name: String,
        is_subtask: bool,
        status_name: String,
    },
    Error {
        gid: String,
        message: String,
    },
}

impl AsanaTaskStatus {
    /// Short display string for the agent list column (task name).
    pub fn format_short(&self) -> String {
        match self {
            AsanaTaskStatus::None => "—".to_string(),
            AsanaTaskStatus::NotStarted { name, .. } => truncate_with_ellipsis(name, 14),
            AsanaTaskStatus::InProgress { name, .. } => truncate_with_ellipsis(name, 14),
            AsanaTaskStatus::Completed { name, .. } => truncate_with_ellipsis(name, 14),
            AsanaTaskStatus::Error { message, .. } => {
                format!("err: {}", truncate_with_ellipsis(message, 10))
            }
        }
    }

    pub fn format_status_name(&self) -> String {
        match self {
            AsanaTaskStatus::None => "—".to_string(),
            AsanaTaskStatus::NotStarted { status_name, .. }
            | AsanaTaskStatus::InProgress { status_name, .. }
            | AsanaTaskStatus::Completed { status_name, .. } => {
                truncate_with_ellipsis(status_name, 10)
            }
            AsanaTaskStatus::Error { .. } => "Error".to_string(),
        }
    }

    /// Get the task GID if linked.
    pub fn gid(&self) -> Option<&str> {
        match self {
            AsanaTaskStatus::None => Option::None,
            AsanaTaskStatus::NotStarted { gid, .. }
            | AsanaTaskStatus::InProgress { gid, .. }
            | AsanaTaskStatus::Completed { gid, .. }
            | AsanaTaskStatus::Error { gid, .. } => Some(gid),
        }
    }

    /// Get the task URL if available.
    pub fn url(&self) -> Option<&str> {
        match self {
            AsanaTaskStatus::NotStarted { url, .. } | AsanaTaskStatus::InProgress { url, .. } => {
                Some(url)
            }
            _ => Option::None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            AsanaTaskStatus::None => None,
            AsanaTaskStatus::NotStarted { name, .. }
            | AsanaTaskStatus::InProgress { name, .. }
            | AsanaTaskStatus::Completed { name, .. } => Some(name),
            AsanaTaskStatus::Error { message, .. } => Some(message),
        }
    }

    pub fn is_linked(&self) -> bool {
        !matches!(self, AsanaTaskStatus::None)
    }

    pub fn is_subtask(&self) -> bool {
        match self {
            AsanaTaskStatus::None => false,
            AsanaTaskStatus::NotStarted { is_subtask, .. }
            | AsanaTaskStatus::InProgress { is_subtask, .. }
            | AsanaTaskStatus::Completed { is_subtask, .. } => *is_subtask,
            AsanaTaskStatus::Error { .. } => false,
        }
    }
}

// --- API response structs ---

#[derive(Debug, Deserialize)]
pub struct AsanaTaskResponse {
    pub data: AsanaTaskData,
}

#[derive(Debug, Deserialize)]
pub struct AsanaTaskData {
    pub gid: String,
    pub name: String,
    pub completed: bool,
    pub permalink_url: Option<String>,
    pub parent: Option<AsanaParent>,
    pub num_subtasks: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct AsanaParent {
    pub gid: String,
}

#[derive(Debug, Deserialize)]
pub struct AsanaSectionsResponse {
    pub data: Vec<AsanaSectionData>,
}

#[derive(Debug, Deserialize)]
pub struct AsanaSectionData {
    pub gid: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SectionOption {
    pub gid: String,
    pub name: String,
}

impl From<AsanaSectionData> for SectionOption {
    fn from(data: AsanaSectionData) -> Self {
        Self {
            gid: data.gid,
            name: data.name,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AsanaTaskListResponse {
    pub data: Vec<AsanaTaskData>,
}

#[derive(Debug, Deserialize)]
pub struct AsanaWorkspacesResponse {
    pub data: Vec<AsanaWorkspaceData>,
}

#[derive(Debug, Deserialize)]
pub struct AsanaWorkspaceData {
    pub gid: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct AsanaProjectsResponse {
    pub data: Vec<AsanaProjectData>,
}

#[derive(Debug, Deserialize)]
pub struct AsanaProjectData {
    pub gid: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct AsanaTaskSummary {
    pub gid: String,
    pub name: String,
    pub completed: bool,
    pub permalink_url: Option<String>,
    pub parent_gid: Option<String>,
    pub num_subtasks: u32,
}

impl From<AsanaTaskData> for AsanaTaskSummary {
    fn from(data: AsanaTaskData) -> Self {
        Self {
            gid: data.gid,
            name: data.name,
            completed: data.completed,
            permalink_url: data.permalink_url,
            parent_gid: data.parent.map(|p| p.gid),
            num_subtasks: data.num_subtasks.unwrap_or(0),
        }
    }
}
