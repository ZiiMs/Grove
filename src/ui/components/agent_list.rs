use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};

use crate::agent::{Agent, AgentStatus, ProjectMgmtTaskStatus};
use crate::app::config::GitProvider;
use crate::asana::AsanaTaskStatus;
use crate::github::CheckStatus;
use crate::gitlab::PipelineStatus;
use crate::notion::NotionTaskStatus;

/// Braille spinner frames for running status
const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Unicode bar characters for sparkline rendering
const BARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub struct AgentListWidget<'a> {
    agents: &'a [&'a Agent],
    selected: usize,
    animation_frame: usize,
    count: usize,
    provider: GitProvider,
}

impl<'a> AgentListWidget<'a> {
    pub fn new(
        agents: &'a [&'a Agent],
        selected: usize,
        animation_frame: usize,
        provider: GitProvider,
    ) -> Self {
        Self {
            agents,
            selected,
            animation_frame,
            count: agents.len(),
            provider,
        }
    }

    pub fn with_count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let header_cells = [
            "", "S", "Name", "Status", "Active", "Rate", "Tasks", "MR", "Pipeline", "PM Task",
            "Note",
        ]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::DarkGray)));
        let header = Row::new(header_cells).height(1);

        // Build rows with separators between agents
        let rows: Vec<Row> = self
            .agents
            .iter()
            .enumerate()
            .flat_map(|(i, agent)| {
                let is_selected = i == self.selected;
                let is_last = i == self.agents.len() - 1;

                // Apply background highlight to selected row
                let mut agent_row = self.render_agent_row(agent, is_selected);
                if is_selected {
                    agent_row = agent_row.style(Style::default().bg(Color::Rgb(40, 44, 52)));
                }

                if is_last {
                    vec![agent_row]
                } else {
                    // Add separator row after each agent (except the last)
                    let separator = Row::new(vec![
                        Cell::from("──"),
                        Cell::from("──"),
                        Cell::from("────────────────────"),
                        Cell::from("──────────────────"),
                        Cell::from("────────"),
                        Cell::from("────────────"),
                        Cell::from("────────"),
                        Cell::from("──────────"),
                        Cell::from("──────────"),
                        Cell::from("────────────────"),
                        Cell::from("──────────"),
                    ])
                    .style(Style::default().fg(Color::DarkGray));
                    vec![agent_row, separator]
                }
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(2),  // Selector
                Constraint::Length(2),  // Summary
                Constraint::Length(28), // Name
                Constraint::Length(18), // Status
                Constraint::Length(8),  // Activity time
                Constraint::Length(12), // Sparkline
                Constraint::Length(8),  // Tasks (checklist progress)
                Constraint::Length(10), // MR
                Constraint::Length(10), // Pipeline
                Constraint::Length(16), // Asana
                Constraint::Min(10),    // Note (fills remaining)
            ],
        )
        .header(header)
        .block(
            Block::default()
                .title(format!(" AGENTS ({}) ", self.count))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        );

        // Render without stateful selection (we handle highlighting manually)
        frame.render_widget(table, area);
    }

    fn render_agent_row(&self, agent: &Agent, selected: bool) -> Row<'a> {
        // Selector column
        let selector = if selected { "▶" } else { "" };
        let selector_cell = Cell::from(selector).style(Style::default().fg(Color::Cyan));

        // Summary column
        let summary_cell = if agent.summary_requested {
            Cell::from("✓").style(Style::default().fg(Color::Green))
        } else {
            Cell::from("")
        };

        // Name column
        let name_style = if selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let name = truncate_string(&agent.name, 26);
        let name_cell = Cell::from(name).style(name_style);

        // Status column (with animated spinner for Running)
        let (status_text, status_style) = self.format_status(&agent.status);
        let status_cell = Cell::from(status_text).style(status_style);

        // Activity time column
        let activity_time = agent.time_since_activity();
        let activity_style = Style::default().fg(Color::DarkGray);
        let activity_cell = Cell::from(activity_time).style(activity_style);

        // Sparkline column (rendered as Unicode bars)
        let sparkline = self.render_sparkline(agent);
        let sparkline_cell = Cell::from(sparkline).style(Style::default().fg(Color::Green));

        // Tasks column (checklist progress)
        let (tasks_text, tasks_style) = match agent.checklist_progress {
            Some((completed, total)) => {
                let style = if completed == total {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                };
                (format!("{}/{}", completed, total), style)
            }
            None => ("—".to_string(), Style::default().fg(Color::DarkGray)),
        };
        let tasks_cell = Cell::from(tasks_text).style(tasks_style);

        // MR column
        let (mr_text, mr_style) = self.format_mr_status(agent);
        let mr_cell = Cell::from(mr_text).style(mr_style);

        // Pipeline column
        let (pipeline_text, pipeline_style) = self.format_pipeline_status(agent);
        let pipeline_cell = Cell::from(pipeline_text).style(pipeline_style);

        // PM Task column
        let (pm_text, pm_style) = self.format_pm_status(agent);
        let pm_cell = Cell::from(pm_text).style(pm_style);

        // Note column
        let note = agent.custom_note.as_deref().unwrap_or("");
        let note = truncate_string(note, 30);
        let note_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::ITALIC);
        let note_cell = Cell::from(note).style(if agent.custom_note.is_some() {
            note_style
        } else {
            Style::default().fg(Color::DarkGray)
        });

        Row::new(vec![
            selector_cell,
            summary_cell,
            name_cell,
            status_cell,
            activity_cell,
            sparkline_cell,
            tasks_cell,
            mr_cell,
            pipeline_cell,
            pm_cell,
            note_cell,
        ])
    }

    fn format_status(&self, status: &AgentStatus) -> (String, Style) {
        match status {
            AgentStatus::Running => {
                let spinner = SPINNER_FRAMES[self.animation_frame];
                (
                    format!("{} Running", spinner),
                    Style::default().fg(Color::Green),
                )
            }
            AgentStatus::AwaitingInput => (
                "⚠ AWAITING INPUT".to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            AgentStatus::Completed => ("✓ Completed".to_string(), Style::default().fg(Color::Cyan)),
            AgentStatus::Idle => ("○ Idle".to_string(), Style::default().fg(Color::DarkGray)),
            AgentStatus::Error(msg) => {
                let display = truncate_string(msg, 14);
                (format!("✗ {}", display), Style::default().fg(Color::Red))
            }
            AgentStatus::Stopped => (
                "○ Stopped".to_string(),
                Style::default().fg(Color::DarkGray),
            ),
            AgentStatus::Paused => (
                "⏸ PAUSED".to_string(),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
        }
    }

    fn format_mr_status(&self, agent: &Agent) -> (String, Style) {
        match self.provider {
            GitProvider::GitLab => {
                let mr_text = agent.mr_status.format_short();
                let mr_style = match &agent.mr_status {
                    crate::gitlab::MergeRequestStatus::None => Style::default().fg(Color::DarkGray),
                    crate::gitlab::MergeRequestStatus::Open { .. } => {
                        Style::default().fg(Color::Green)
                    }
                    crate::gitlab::MergeRequestStatus::Merged { .. } => {
                        Style::default().fg(Color::Magenta)
                    }
                    crate::gitlab::MergeRequestStatus::Conflicts { .. } => {
                        Style::default().fg(Color::Red)
                    }
                    crate::gitlab::MergeRequestStatus::NeedsRebase { .. } => {
                        Style::default().fg(Color::Red)
                    }
                    crate::gitlab::MergeRequestStatus::Approved { .. } => {
                        Style::default().fg(Color::Cyan)
                    }
                };
                (mr_text, mr_style)
            }
            GitProvider::GitHub => {
                let pr_text = agent.pr_status.format_short();
                let pr_style = match &agent.pr_status {
                    crate::github::PullRequestStatus::None => Style::default().fg(Color::DarkGray),
                    crate::github::PullRequestStatus::Open { .. } => {
                        Style::default().fg(Color::Green)
                    }
                    crate::github::PullRequestStatus::Merged { .. } => {
                        Style::default().fg(Color::Magenta)
                    }
                    crate::github::PullRequestStatus::Closed { .. } => {
                        Style::default().fg(Color::Red)
                    }
                    crate::github::PullRequestStatus::Draft { .. } => {
                        Style::default().fg(Color::Yellow)
                    }
                };
                (pr_text, pr_style)
            }
            GitProvider::Codeberg => {
                let pr_text = agent.codeberg_pr_status.format_short();
                let pr_style = match &agent.codeberg_pr_status {
                    crate::codeberg::PullRequestStatus::None => {
                        Style::default().fg(Color::DarkGray)
                    }
                    crate::codeberg::PullRequestStatus::Open { .. } => {
                        Style::default().fg(Color::Green)
                    }
                    crate::codeberg::PullRequestStatus::Merged { .. } => {
                        Style::default().fg(Color::Cyan)
                    }
                    crate::codeberg::PullRequestStatus::Closed { .. } => {
                        Style::default().fg(Color::Red)
                    }
                    crate::codeberg::PullRequestStatus::Draft { .. } => {
                        Style::default().fg(Color::Yellow)
                    }
                };
                (pr_text, pr_style)
            }
        }
    }

    fn format_pipeline_status(&self, agent: &Agent) -> (String, Style) {
        match self.provider {
            GitProvider::GitLab => {
                let pipeline = agent.mr_status.pipeline();
                let text = format!("{} {}", pipeline.symbol(), pipeline.label());
                let style = match pipeline {
                    PipelineStatus::None => Style::default().fg(Color::DarkGray),
                    PipelineStatus::Running => Style::default().fg(Color::LightBlue),
                    PipelineStatus::Pending => Style::default().fg(Color::Yellow),
                    PipelineStatus::Success => Style::default().fg(Color::Green),
                    PipelineStatus::Failed => Style::default().fg(Color::Red),
                    PipelineStatus::Canceled => Style::default().fg(Color::DarkGray),
                    PipelineStatus::Skipped => Style::default().fg(Color::DarkGray),
                    PipelineStatus::Manual => Style::default().fg(Color::Magenta),
                };
                (text, style)
            }
            GitProvider::GitHub => {
                let checks = agent.pr_status.checks();
                let text = format!("{} {}", checks.symbol(), checks.label());
                let style = match checks {
                    CheckStatus::None => Style::default().fg(Color::DarkGray),
                    CheckStatus::Running => Style::default().fg(Color::LightBlue),
                    CheckStatus::Pending => Style::default().fg(Color::Yellow),
                    CheckStatus::Success => Style::default().fg(Color::Green),
                    CheckStatus::Failure => Style::default().fg(Color::Red),
                    CheckStatus::Cancelled => Style::default().fg(Color::DarkGray),
                    CheckStatus::Skipped => Style::default().fg(Color::DarkGray),
                    CheckStatus::TimedOut => Style::default().fg(Color::Red),
                };
                (text, style)
            }
            GitProvider::Codeberg => {
                let pipeline = agent.codeberg_pr_status.pipeline();
                let text = format!("{} {}", pipeline.symbol(), pipeline.label());
                let style = match pipeline {
                    PipelineStatus::None => Style::default().fg(Color::DarkGray),
                    PipelineStatus::Running => Style::default().fg(Color::LightBlue),
                    PipelineStatus::Pending => Style::default().fg(Color::Yellow),
                    PipelineStatus::Success => Style::default().fg(Color::Green),
                    PipelineStatus::Failed => Style::default().fg(Color::Red),
                    PipelineStatus::Canceled => Style::default().fg(Color::DarkGray),
                    PipelineStatus::Skipped => Style::default().fg(Color::DarkGray),
                    PipelineStatus::Manual => Style::default().fg(Color::Magenta),
                };
                (text, style)
            }
        }
    }

    fn format_pm_status(&self, agent: &Agent) -> (String, Style) {
        let text = agent.pm_task_status.format_short();
        let style = match &agent.pm_task_status {
            ProjectMgmtTaskStatus::None => Style::default().fg(Color::DarkGray),
            ProjectMgmtTaskStatus::Asana(s) => match s {
                AsanaTaskStatus::None => Style::default().fg(Color::DarkGray),
                AsanaTaskStatus::NotStarted { .. } => Style::default().fg(Color::White),
                AsanaTaskStatus::InProgress { .. } => Style::default().fg(Color::LightBlue),
                AsanaTaskStatus::Completed { .. } => Style::default().fg(Color::Green),
                AsanaTaskStatus::Error { .. } => Style::default().fg(Color::Red),
            },
            ProjectMgmtTaskStatus::Notion(s) => match s {
                NotionTaskStatus::None => Style::default().fg(Color::DarkGray),
                NotionTaskStatus::NotStarted { .. } => Style::default().fg(Color::White),
                NotionTaskStatus::InProgress { .. } => Style::default().fg(Color::LightBlue),
                NotionTaskStatus::Completed { .. } => Style::default().fg(Color::Green),
                NotionTaskStatus::Error { .. } => Style::default().fg(Color::Red),
            },
        };
        (text, style)
    }

    fn render_sparkline(&self, agent: &Agent) -> String {
        let data = agent.sparkline_data();
        if data.is_empty() {
            return "─".repeat(8);
        }

        // Find max value for scaling (at least 1 to avoid division by zero)
        let max_val = *data.iter().max().unwrap_or(&1).max(&1);

        // Take last 8 values for display
        let display_data: Vec<u64> = if data.len() > 8 {
            data[data.len() - 8..].to_vec()
        } else {
            data
        };

        // Scale values to bar heights (0-8)
        let bars: String = display_data
            .iter()
            .map(|&val| {
                if max_val == 0 {
                    BARS[0]
                } else {
                    let scaled = (val * 8) / max_val.max(1);
                    BARS[scaled.min(8) as usize]
                }
            })
            .collect();

        // Pad to 8 characters if needed
        format!("{:─<8}", bars)
    }
}

/// Truncate a string to fit within max_len, adding "…" if truncated
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len - 1).collect();
        format!("{}…", truncated)
    }
}

/// Calculate the height needed for an agent in the table (always 1 row now).
pub fn agent_height(_agent: &Agent) -> u16 {
    1
}
