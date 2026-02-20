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
    pub url: String,
}
