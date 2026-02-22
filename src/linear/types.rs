use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LinearTaskStatus {
    #[default]
    None,
    NotStarted {
        id: String,
        identifier: String,
        name: String,
        status_name: String,
        url: String,
        is_subtask: bool,
    },
    InProgress {
        id: String,
        identifier: String,
        name: String,
        status_name: String,
        url: String,
        is_subtask: bool,
    },
    Completed {
        id: String,
        identifier: String,
        name: String,
        status_name: String,
        is_subtask: bool,
    },
    Error {
        id: String,
        message: String,
    },
}

impl LinearTaskStatus {
    pub fn format_short(&self) -> String {
        match self {
            LinearTaskStatus::None => "—".to_string(),
            LinearTaskStatus::NotStarted { name, .. } => truncate(name, 24),
            LinearTaskStatus::InProgress { name, .. } => truncate(name, 24),
            LinearTaskStatus::Completed { name, .. } => truncate(name, 24),
            LinearTaskStatus::Error { message, .. } => format!("err: {}", truncate(message, 10)),
        }
    }

    pub fn format_status_name(&self) -> String {
        match self {
            LinearTaskStatus::None => "—".to_string(),
            LinearTaskStatus::NotStarted { status_name, .. }
            | LinearTaskStatus::InProgress { status_name, .. }
            | LinearTaskStatus::Completed { status_name, .. } => truncate(status_name, 10),
            LinearTaskStatus::Error { .. } => "Error".to_string(),
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            LinearTaskStatus::None => None,
            LinearTaskStatus::NotStarted { id, .. }
            | LinearTaskStatus::InProgress { id, .. }
            | LinearTaskStatus::Completed { id, .. }
            | LinearTaskStatus::Error { id, .. } => Some(id),
        }
    }

    pub fn identifier(&self) -> Option<&str> {
        match self {
            LinearTaskStatus::None => None,
            LinearTaskStatus::NotStarted { identifier, .. }
            | LinearTaskStatus::InProgress { identifier, .. }
            | LinearTaskStatus::Completed { identifier, .. } => Some(identifier),
            LinearTaskStatus::Error { .. } => None,
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            LinearTaskStatus::NotStarted { url, .. } | LinearTaskStatus::InProgress { url, .. } => {
                Some(url)
            }
            _ => None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            LinearTaskStatus::None => None,
            LinearTaskStatus::NotStarted { name, .. }
            | LinearTaskStatus::InProgress { name, .. }
            | LinearTaskStatus::Completed { name, .. } => Some(name),
            LinearTaskStatus::Error { message, .. } => Some(message),
        }
    }

    pub fn is_linked(&self) -> bool {
        !matches!(self, LinearTaskStatus::None)
    }

    pub fn is_subtask(&self) -> bool {
        match self {
            LinearTaskStatus::None => false,
            LinearTaskStatus::NotStarted { is_subtask, .. }
            | LinearTaskStatus::InProgress { is_subtask, .. }
            | LinearTaskStatus::Completed { is_subtask, .. } => *is_subtask,
            LinearTaskStatus::Error { .. } => false,
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
pub struct WorkflowState {
    pub id: String,
    pub name: String,
    pub state_type: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LinearIssueSummary {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub state_id: String,
    pub state_name: String,
    pub state_type: String,
    pub url: String,
    pub parent_id: Option<String>,
    pub has_children: bool,
}

#[derive(Debug, Deserialize)]
pub struct GraphQLResponse<T> {
    pub data: T,
}

#[derive(Debug, Deserialize)]
pub struct IssueQueryData {
    pub issue: Option<LinearIssueData>,
}

#[derive(Debug, Deserialize)]
pub struct TeamIssuesQueryData {
    pub team: Option<TeamIssuesData>,
}

#[derive(Debug, Deserialize)]
pub struct TeamIssuesData {
    pub issues: IssuesConnection,
}

#[derive(Debug, Deserialize)]
pub struct IssuesConnection {
    pub nodes: Vec<LinearIssueData>,
}

#[derive(Debug, Deserialize)]
pub struct LinearIssueData {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub state: LinearStateData,
    pub parent: Option<LinearIssueParent>,
    pub children: Option<ChildrenConnection>,
}

#[derive(Debug, Deserialize)]
pub struct LinearStateData {
    pub id: String,
    pub name: String,
    #[serde(rename = "type", default)]
    pub state_type: String,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LinearIssueParent {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct ChildIssueData {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct ChildrenConnection {
    pub nodes: Vec<ChildIssueData>,
}

#[derive(Debug, Deserialize)]
pub struct TeamStatesQueryData {
    pub team: Option<TeamStatesData>,
}

#[derive(Debug, Deserialize)]
pub struct TeamStatesData {
    pub states: StatesConnection,
}

#[derive(Debug, Deserialize)]
pub struct StatesConnection {
    pub nodes: Vec<LinearStateData>,
}

#[derive(Debug, Deserialize)]
pub struct IssueUpdateData {
    #[serde(rename = "issueUpdate")]
    pub issue_update: IssueUpdateResult,
}

#[derive(Debug, Deserialize)]
pub struct IssueUpdateResult {
    pub success: bool,
    pub issue: Option<IssueUpdateIssueData>,
}

#[derive(Debug, Deserialize)]
pub struct IssueUpdateIssueData {
    pub id: String,
    pub state: IssueUpdateStateData,
}

#[derive(Debug, Deserialize)]
pub struct IssueUpdateStateData {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct TeamsQueryData {
    pub teams: TeamsConnection,
}

#[derive(Debug, Deserialize)]
pub struct TeamsConnection {
    pub nodes: Vec<TeamData>,
}

#[derive(Debug, Deserialize)]
pub struct TeamData {
    pub id: String,
    pub name: String,
    pub key: String,
}

pub fn parse_linear_issue_id(input: &str) -> String {
    let trimmed = input.trim();

    if trimmed.contains("linear.app") {
        if let Some(last) = trimmed.trim_end_matches('/').rsplit('/').next() {
            if last.starts_with("issue/") {
                return last.strip_prefix("issue/").unwrap_or(last).to_string();
            }
            return last.to_string();
        }
    }

    if trimmed.contains('-') && trimmed.chars().filter(|c| c.is_numeric()).count() > 0 {
        let parts: Vec<&str> = trimmed.split('-').collect();
        if parts.len() >= 2 {
            return trimmed.to_uppercase();
        }
    }

    trimmed.to_string()
}
