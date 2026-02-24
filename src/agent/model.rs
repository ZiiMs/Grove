use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::git_providers::codeberg::PullRequestStatus as CodebergPullRequestStatus;
use crate::core::git_providers::github::PullRequestStatus;
use crate::core::git_providers::gitlab::MergeRequestStatus;
use crate::core::projects::airtable::AirtableTaskStatus;
use crate::core::projects::asana::AsanaTaskStatus;
use crate::core::projects::clickup::ClickUpTaskStatus;
use crate::core::projects::linear::LinearTaskStatus;
use crate::core::projects::notion::NotionTaskStatus;
use crate::git::GitSyncStatus;

const ACTIVITY_HISTORY_SIZE: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusReason {
    pub status: AgentStatus,
    pub reason: String,
    pub pattern: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProjectMgmtTaskStatus {
    #[default]
    None,
    Asana(AsanaTaskStatus),
    Notion(NotionTaskStatus),
    ClickUp(ClickUpTaskStatus),
    Airtable(AirtableTaskStatus),
    Linear(LinearTaskStatus),
}

impl ProjectMgmtTaskStatus {
    pub fn format_short(&self) -> String {
        match self {
            ProjectMgmtTaskStatus::None => "—".to_string(),
            ProjectMgmtTaskStatus::Asana(s) => s.format_short(),
            ProjectMgmtTaskStatus::Notion(s) => s.format_short(),
            ProjectMgmtTaskStatus::ClickUp(s) => s.format_short(),
            ProjectMgmtTaskStatus::Airtable(s) => s.format_short(),
            ProjectMgmtTaskStatus::Linear(s) => s.format_short(),
        }
    }

    /// Display string for the status name column.
    pub fn format_status_name(&self) -> String {
        match self {
            ProjectMgmtTaskStatus::None => "—".to_string(),
            ProjectMgmtTaskStatus::Asana(s) => s.format_status_name(),
            ProjectMgmtTaskStatus::Notion(s) => s.format_status_name(),
            ProjectMgmtTaskStatus::ClickUp(s) => s.format_status_name(),
            ProjectMgmtTaskStatus::Airtable(s) => s.format_status_name(),
            ProjectMgmtTaskStatus::Linear(s) => s.format_status_name(),
        }
    }

    pub fn is_linked(&self) -> bool {
        !matches!(self, ProjectMgmtTaskStatus::None)
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            ProjectMgmtTaskStatus::Asana(s) => s.gid(),
            ProjectMgmtTaskStatus::Notion(s) => s.page_id(),
            ProjectMgmtTaskStatus::ClickUp(s) => s.id(),
            ProjectMgmtTaskStatus::Airtable(s) => s.id(),
            ProjectMgmtTaskStatus::Linear(s) => s.id(),
            ProjectMgmtTaskStatus::None => None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            ProjectMgmtTaskStatus::Asana(s) => s.name(),
            ProjectMgmtTaskStatus::Notion(s) => s.name(),
            ProjectMgmtTaskStatus::ClickUp(s) => s.name(),
            ProjectMgmtTaskStatus::Airtable(s) => s.name(),
            ProjectMgmtTaskStatus::Linear(s) => s.name(),
            ProjectMgmtTaskStatus::None => None,
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            ProjectMgmtTaskStatus::Asana(s) => s.url(),
            ProjectMgmtTaskStatus::Notion(s) => s.url(),
            ProjectMgmtTaskStatus::ClickUp(s) => s.url(),
            ProjectMgmtTaskStatus::Airtable(s) => s.url(),
            ProjectMgmtTaskStatus::Linear(s) => s.url(),
            ProjectMgmtTaskStatus::None => None,
        }
    }

    pub fn is_asana_not_started(&self) -> bool {
        matches!(
            self,
            ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted { .. })
        )
    }

    pub fn is_clickup_not_started(&self) -> bool {
        matches!(
            self,
            ProjectMgmtTaskStatus::ClickUp(ClickUpTaskStatus::NotStarted { .. })
        )
    }

    pub fn is_airtable_not_started(&self) -> bool {
        matches!(
            self,
            ProjectMgmtTaskStatus::Airtable(AirtableTaskStatus::NotStarted { .. })
        )
    }

    pub fn is_linear_not_started(&self) -> bool {
        matches!(
            self,
            ProjectMgmtTaskStatus::Linear(LinearTaskStatus::NotStarted { .. })
        )
    }

    pub fn as_asana(&self) -> Option<&AsanaTaskStatus> {
        match self {
            ProjectMgmtTaskStatus::Asana(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_notion(&self) -> Option<&NotionTaskStatus> {
        match self {
            ProjectMgmtTaskStatus::Notion(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_clickup(&self) -> Option<&ClickUpTaskStatus> {
        match self {
            ProjectMgmtTaskStatus::ClickUp(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_airtable(&self) -> Option<&AirtableTaskStatus> {
        match self {
            ProjectMgmtTaskStatus::Airtable(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_linear(&self) -> Option<&LinearTaskStatus> {
        match self {
            ProjectMgmtTaskStatus::Linear(s) => Some(s),
            _ => None,
        }
    }
}

/// Represents the current status of a Claude agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    /// Agent is actively running (● green)
    Running,
    /// Agent is waiting for user input (⚠ yellow, bold) - CRITICAL distinction
    AwaitingInput,
    /// Agent has completed its task (✓ cyan)
    Completed,
    /// Agent is at prompt, ready for next task (○ gray)
    Idle,
    /// Agent encountered an error (✗ red)
    Error(String),
    /// Agent is stopped/not started (○ gray)
    Stopped,
    /// Agent is paused for checkout (⏸ blue)
    Paused,
}

impl AgentStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            AgentStatus::Running => "●",
            AgentStatus::AwaitingInput => "⚠",
            AgentStatus::Completed => "✓",
            AgentStatus::Idle => "○",
            AgentStatus::Error(_) => "✗",
            AgentStatus::Stopped => "○",
            AgentStatus::Paused => "⏸",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AgentStatus::Running => "Running",
            AgentStatus::AwaitingInput => "AWAITING INPUT",
            AgentStatus::Completed => "Completed",
            AgentStatus::Idle => "Idle",
            AgentStatus::Error(_) => "Error",
            AgentStatus::Stopped => "Stopped",
            AgentStatus::Paused => "PAUSED",
        }
    }
}

/// A Claude Code agent with its associated context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub branch: String,
    pub worktree_path: String,
    pub tmux_session: String,
    pub tmux_pane: Option<String>,
    pub status: AgentStatus,
    pub custom_note: Option<String>,
    pub output_buffer: Vec<String>,
    #[serde(skip)]
    pub git_status: Option<GitSyncStatus>,
    #[serde(skip)]
    pub mr_status: MergeRequestStatus,
    #[serde(skip)]
    pub pr_status: PullRequestStatus,
    #[serde(skip)]
    pub codeberg_pr_status: CodebergPullRequestStatus,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    /// Activity history for sparkline (last 20 ticks, true = had activity)
    #[serde(skip)]
    pub activity_history: VecDeque<bool>,
    /// Checklist progress (completed, total) if a checklist is detected
    #[serde(skip)]
    pub checklist_progress: Option<(u32, u32)>,
    /// Legacy Asana task status (for backward compatibility during migration)
    #[serde(default, skip_serializing)]
    pub asana_task_status: AsanaTaskStatus,
    /// Project management task status (persisted across sessions)
    #[serde(default)]
    pub pm_task_status: ProjectMgmtTaskStatus,
    /// Whether a work summary has been requested for this agent
    #[serde(default)]
    pub summary_requested: bool,
    /// Whether to auto-continue this session on app restart
    #[serde(default)]
    pub continue_session: bool,
    #[serde(skip)]
    pub status_reason: Option<StatusReason>,
}

impl Agent {
    pub fn new(name: String, branch: String, worktree_path: String) -> Self {
        let id = Uuid::new_v4();
        let tmux_session = format!("grove-{}", id.as_simple());

        Self {
            id,
            name,
            branch,
            worktree_path,
            tmux_session,
            tmux_pane: None,
            status: AgentStatus::Stopped,
            custom_note: None,
            output_buffer: Vec::new(),
            git_status: None,
            mr_status: MergeRequestStatus::None,
            pr_status: PullRequestStatus::None,
            codeberg_pr_status: CodebergPullRequestStatus::None,
            created_at: Utc::now(),
            last_activity: Utc::now(),
            activity_history: VecDeque::with_capacity(ACTIVITY_HISTORY_SIZE),
            checklist_progress: None,
            asana_task_status: AsanaTaskStatus::None,
            pm_task_status: ProjectMgmtTaskStatus::None,
            summary_requested: false,
            continue_session: false,
            status_reason: None,
        }
    }

    pub fn migrate_legacy(&mut self) {
        if !matches!(self.asana_task_status, AsanaTaskStatus::None)
            && matches!(self.pm_task_status, ProjectMgmtTaskStatus::None)
        {
            self.pm_task_status = ProjectMgmtTaskStatus::Asana(self.asana_task_status.clone());
        }
    }

    /// Record whether there was activity in the current tick
    pub fn record_activity(&mut self, had_activity: bool) {
        if self.activity_history.len() >= ACTIVITY_HISTORY_SIZE {
            self.activity_history.pop_front();
        }
        self.activity_history.push_back(had_activity);
        if had_activity {
            self.last_activity = Utc::now();
        }
    }

    /// Get sparkline data as 0/1 values for rendering
    pub fn sparkline_data(&self) -> Vec<u64> {
        self.activity_history
            .iter()
            .map(|&active| if active { 1 } else { 0 })
            .collect()
    }

    /// Format time since last activity as human-readable string
    pub fn time_since_activity(&self) -> String {
        let now = Utc::now();
        let duration = now.signed_duration_since(self.last_activity);

        if duration.num_seconds() < 60 {
            format!("{}s ago", duration.num_seconds())
        } else if duration.num_minutes() < 60 {
            format!("{}m ago", duration.num_minutes())
        } else if duration.num_hours() < 24 {
            format!("{}h ago", duration.num_hours())
        } else {
            format!("{}d ago", duration.num_days())
        }
    }

    pub fn update_output(&mut self, output: String, max_lines: usize) {
        // Parse output into lines and add to buffer
        for line in output.lines() {
            self.output_buffer.push(line.to_string());
        }

        // Trim buffer if it exceeds max lines
        if self.output_buffer.len() > max_lines {
            let excess = self.output_buffer.len() - max_lines;
            self.output_buffer.drain(0..excess);
        }

        self.last_activity = Utc::now();
    }

    pub fn set_status(&mut self, status: AgentStatus) {
        self.status = status;
    }
}
