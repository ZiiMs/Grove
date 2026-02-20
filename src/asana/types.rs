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
    },
    InProgress {
        gid: String,
        name: String,
        url: String,
    },
    Completed {
        gid: String,
        name: String,
    },
    Error {
        gid: String,
        message: String,
    },
}

impl AsanaTaskStatus {
    /// Short display string for the agent list column.
    pub fn format_short(&self) -> String {
        match self {
            AsanaTaskStatus::None => "—".to_string(),
            AsanaTaskStatus::NotStarted { name, .. } => truncate(name, 14),
            AsanaTaskStatus::InProgress { name, .. } => truncate(name, 14),
            AsanaTaskStatus::Completed { name, .. } => truncate(name, 14),
            AsanaTaskStatus::Error { message, .. } => format!("err: {}", truncate(message, 10)),
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

    /// Whether an Asana task is linked.
    pub fn is_linked(&self) -> bool {
        !matches!(self, AsanaTaskStatus::None)
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

#[derive(Debug, Deserialize)]
pub struct AsanaTaskListResponse {
    pub data: Vec<AsanaTaskData>,
}

#[derive(Debug, Clone)]
pub struct AsanaTaskSummary {
    pub gid: String,
    pub name: String,
    pub completed: bool,
    pub permalink_url: Option<String>,
}

impl From<AsanaTaskData> for AsanaTaskSummary {
    fn from(data: AsanaTaskData) -> Self {
        Self {
            gid: data.gid,
            name: data.name,
            completed: data.completed,
            permalink_url: data.permalink_url,
        }
    }
}
