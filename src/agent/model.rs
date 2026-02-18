use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::asana::AsanaTaskStatus;
use crate::git::GitSyncStatus;
use crate::github::PullRequestStatus;
use crate::gitlab::MergeRequestStatus;

/// Maximum number of activity ticks to track for sparkline
const ACTIVITY_HISTORY_SIZE: usize = 20;

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
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    /// Activity history for sparkline (last 20 ticks, true = had activity)
    #[serde(skip)]
    pub activity_history: VecDeque<bool>,
    /// Checklist progress (completed, total) if a checklist is detected
    #[serde(skip)]
    pub checklist_progress: Option<(u32, u32)>,
    /// Asana task tracking status (persisted across sessions)
    #[serde(default)]
    pub asana_task_status: AsanaTaskStatus,
    /// Whether a work summary has been requested for this agent
    #[serde(default)]
    pub summary_requested: bool,
}

impl Agent {
    pub fn new(name: String, branch: String, worktree_path: String) -> Self {
        let id = Uuid::new_v4();
        let tmux_session = format!("flock-{}", id.as_simple());

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
            created_at: Utc::now(),
            last_activity: Utc::now(),
            activity_history: VecDeque::with_capacity(ACTIVITY_HISTORY_SIZE),
            checklist_progress: None,
            asana_task_status: AsanaTaskStatus::None,
            summary_requested: false,
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
