use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PipelineStatus {
    #[default]
    None,
    Running,
    Pending,
    Success,
    Failed,
    Canceled,
    Skipped,
    Manual,
}

impl PipelineStatus {
    pub fn from_gitlab_status(status: &str) -> Self {
        match status {
            "running" => PipelineStatus::Running,
            "pending" | "waiting_for_resource" | "preparing" | "created" => PipelineStatus::Pending,
            "success" => PipelineStatus::Success,
            "failed" => PipelineStatus::Failed,
            "canceled" => PipelineStatus::Canceled,
            "skipped" => PipelineStatus::Skipped,
            "manual" | "scheduled" => PipelineStatus::Manual,
            _ => PipelineStatus::None,
        }
    }

    pub fn from_woodpecker_status(status: &str) -> Self {
        match status {
            "running" => PipelineStatus::Running,
            "pending" | "created" | "blocked" => PipelineStatus::Pending,
            "success" => PipelineStatus::Success,
            "failure" => PipelineStatus::Failed,
            "killed" | "declined" => PipelineStatus::Canceled,
            "skipped" => PipelineStatus::Skipped,
            "error" => PipelineStatus::Failed,
            _ => PipelineStatus::None,
        }
    }

    pub fn from_forgejo_status(status: &str, conclusion: Option<&str>) -> Self {
        match status {
            "running" | "in_progress" | "waiting" => PipelineStatus::Running,
            "pending" | "queued" | "blocked" => PipelineStatus::Pending,
            "completed" => match conclusion.unwrap_or("") {
                "success" => PipelineStatus::Success,
                "failure" => PipelineStatus::Failed,
                "cancelled" | "canceled" => PipelineStatus::Canceled,
                "skipped" => PipelineStatus::Skipped,
                "timed_out" => PipelineStatus::Failed,
                _ => PipelineStatus::None,
            },
            _ => PipelineStatus::None,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            PipelineStatus::None => "─",
            PipelineStatus::Running => "●",
            PipelineStatus::Pending => "◐",
            PipelineStatus::Success => "✓",
            PipelineStatus::Failed => "✗",
            PipelineStatus::Canceled => "⊘",
            PipelineStatus::Skipped => "⊘",
            PipelineStatus::Manual => "▶",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            PipelineStatus::None => "None",
            PipelineStatus::Running => "Running",
            PipelineStatus::Pending => "Pending",
            PipelineStatus::Success => "Passed",
            PipelineStatus::Failed => "Failed",
            PipelineStatus::Canceled => "Canceled",
            PipelineStatus::Skipped => "Skipped",
            PipelineStatus::Manual => "Manual",
        }
    }
}
