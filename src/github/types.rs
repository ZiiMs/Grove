use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CheckStatus {
    #[default]
    None,
    Pending,
    Running,
    Success,
    Failure,
    Cancelled,
    Skipped,
    TimedOut,
}

impl CheckStatus {
    pub fn from_github_status(status: &str, conclusion: Option<&str>) -> Self {
        match status {
            "queued" => CheckStatus::Pending,
            "in_progress" | "waiting" => CheckStatus::Running,
            "completed" => match conclusion.unwrap_or("") {
                "success" => CheckStatus::Success,
                "failure" => CheckStatus::Failure,
                "cancelled" => CheckStatus::Cancelled,
                "skipped" => CheckStatus::Skipped,
                "timed_out" => CheckStatus::TimedOut,
                "action_required" => CheckStatus::Pending,
                _ => CheckStatus::None,
            },
            _ => CheckStatus::None,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            CheckStatus::None => "─",
            CheckStatus::Pending => "◐",
            CheckStatus::Running => "●",
            CheckStatus::Success => "✓",
            CheckStatus::Failure => "✗",
            CheckStatus::Cancelled => "⊘",
            CheckStatus::Skipped => "⊘",
            CheckStatus::TimedOut => "⏱",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CheckStatus::None => "None",
            CheckStatus::Pending => "Pending",
            CheckStatus::Running => "Running",
            CheckStatus::Success => "Passed",
            CheckStatus::Failure => "Failed",
            CheckStatus::Cancelled => "Cancelled",
            CheckStatus::Skipped => "Skipped",
            CheckStatus::TimedOut => "Timed Out",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PullRequestStatus {
    #[default]
    None,
    Open {
        number: u64,
        url: String,
        checks: CheckStatus,
    },
    Merged {
        number: u64,
    },
    Closed {
        number: u64,
    },
    Draft {
        number: u64,
        url: String,
        checks: CheckStatus,
    },
}

impl PullRequestStatus {
    pub fn format_short(&self) -> String {
        match self {
            PullRequestStatus::None => "None".to_string(),
            PullRequestStatus::Open { number, .. } => format!("#{}", number),
            PullRequestStatus::Merged { number } => format!("#{} Merged", number),
            PullRequestStatus::Closed { number } => format!("#{} Closed", number),
            PullRequestStatus::Draft { number, .. } => format!("#{} Draft", number),
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            PullRequestStatus::Open { url, .. } | PullRequestStatus::Draft { url, .. } => Some(url),
            _ => None,
        }
    }

    pub fn checks(&self) -> &CheckStatus {
        match self {
            PullRequestStatus::Open { checks, .. } | PullRequestStatus::Draft { checks, .. } => {
                checks
            }
            _ => &CheckStatus::None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PullRequestResponse {
    pub number: u64,
    pub state: String,
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    pub head: PullRequestHead,
    #[serde(default)]
    pub merged: bool,
    #[serde(rename = "merged_at")]
    pub merged_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PullRequestHead {
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_field: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckRunsResponse {
    pub check_runs: Vec<CheckRunResponse>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRunResponse {
    pub status: String,
    pub conclusion: Option<String>,
}
