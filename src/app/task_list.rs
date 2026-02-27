use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListItem {
    pub id: String,
    pub identifier: Option<String>,
    pub name: String,
    pub status_name: String,
    pub url: String,
    pub parent_id: Option<String>,
    pub has_children: bool,
}

impl TaskListItem {
    pub fn is_top_level(&self) -> bool {
        self.parent_id.is_none()
    }

    pub fn is_subtask(&self) -> bool {
        self.parent_id.is_some()
    }
}
