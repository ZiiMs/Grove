use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskItemStatus {
    NotStarted,
    InProgress,
}

impl TaskItemStatus {
    pub fn display_name(&self) -> &'static str {
        match self {
            TaskItemStatus::NotStarted => "Not Started",
            TaskItemStatus::InProgress => "In Progress",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListItem {
    pub id: String,
    pub name: String,
    pub status: TaskItemStatus,
    pub status_name: String,
    pub url: String,
    pub parent_id: Option<String>,
    pub has_children: bool,
}

impl TaskListItem {
    pub fn is_top_level(&self) -> bool {
        self.parent_id.is_none()
    }
}
