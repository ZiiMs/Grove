use std::collections::HashMap;
use uuid::Uuid;

use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};

use crate::agent::{Agent, AgentStatus};
use crate::app::config::{AppearanceConfig, GitProvider, ProjectMgmtProvider};
use crate::core::git_providers::codeberg::PullRequestStatus as CodebergPullRequestStatus;
use crate::core::git_providers::github::{
    CheckStatus, PullRequestStatus as GitHubPullRequestStatus,
};
use crate::core::git_providers::gitlab::{MergeRequestStatus, PipelineStatus};
use crate::devserver::DevServerStatus;

/// Braille spinner frames for running status
const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Unicode bar characters for sparkline rendering
const BARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub struct AgentListWidget<'a> {
    agents: &'a [&'a Agent],
    selected: usize,
    scroll_offset: usize,
    animation_frame: usize,
    count: usize,
    provider: GitProvider,
    devserver_statuses: &'a HashMap<Uuid, DevServerStatus>,
    appearance_config: &'a AppearanceConfig,
    pm_provider: ProjectMgmtProvider,
}

impl<'a> AgentListWidget<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agents: &'a [&'a Agent],
        selected: usize,
        scroll_offset: usize,
        animation_frame: usize,
        provider: GitProvider,
        devserver_statuses: &'a HashMap<Uuid, DevServerStatus>,
        appearance_config: &'a AppearanceConfig,
        pm_provider: ProjectMgmtProvider,
    ) -> Self {
        Self {
            agents,
            selected,
            scroll_offset,
            animation_frame,
            count: agents.len(),
            provider,
            devserver_statuses,
            appearance_config,
            pm_provider,
        }
    }

    pub fn with_count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let header_cells = [
            "", "S", "C", "Name", "Status", "Active", "Rate", "Tasks", "MR", "Pipeline", "Server",
            "Task", "Task St", "Note",
        ]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::DarkGray)));
        let header = Row::new(header_cells).height(1);

        let total_agents = self.agents.len();
        if total_agents == 0 {
            let table = Table::new(
                vec![Row::new(vec![Cell::from("")])],
                [
                    Constraint::Length(2),
                    Constraint::Length(2),
                    Constraint::Length(2),
                    Constraint::Length(28),
                    Constraint::Length(18),
                    Constraint::Length(8),
                    Constraint::Length(12),
                    Constraint::Length(8),
                    Constraint::Length(10),
                    Constraint::Length(10),
                    Constraint::Length(10),
                    Constraint::Length(16),
                    Constraint::Length(10),
                    Constraint::Min(10),
                ],
            )
            .header(header)
            .block(
                Block::default()
                    .title(format!(" AGENTS ({}) ", self.count))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White)),
            );
            frame.render_widget(table, area);
            return;
        }

        let available_height = area.height.saturating_sub(3) as usize;

        let mut scroll_offset = self.scroll_offset;

        let mut max_visible = 1;
        for _ in 0..3 {
            scroll_offset = scroll_offset.min(total_agents.saturating_sub(1));

            if self.selected < scroll_offset {
                scroll_offset = self.selected;
            }

            let has_above = scroll_offset > 0;

            let has_below_estimate = scroll_offset + max_visible < total_agents;
            let indicator_rows =
                (if has_above { 1 } else { 0 }) + (if has_below_estimate { 1 } else { 0 });

            let rows_for_agents = available_height.saturating_sub(indicator_rows);

            max_visible = if rows_for_agents <= 1 {
                1
            } else {
                rows_for_agents.div_ceil(2)
            };

            let max_scroll = total_agents.saturating_sub(max_visible);
            scroll_offset = scroll_offset.min(max_scroll);

            if self.selected >= scroll_offset + max_visible {
                scroll_offset = self.selected.saturating_sub(max_visible - 1);
            }
        }

        let has_above = scroll_offset > 0;
        let has_below = scroll_offset + max_visible < total_agents;

        let end_index = (scroll_offset + max_visible).min(total_agents);
        let visible_slice = &self.agents[scroll_offset..end_index];
        let last_visible_index = end_index.saturating_sub(1);

        let mut rows: Vec<Row> = Vec::new();

        if has_above {
            let hidden_above = scroll_offset;
            let indicator = Row::new(vec![
                Cell::from(""),
                Cell::from(""),
                Cell::from(format!("▲ {} more above", hidden_above))
                    .style(Style::default().fg(Color::Yellow)),
            ]);
            rows.push(indicator);
        }

        for (i, agent) in visible_slice.iter().enumerate() {
            let actual_index = scroll_offset + i;
            let is_selected = actual_index == self.selected;
            let is_last_overall = actual_index == total_agents - 1;
            let is_last_visible = actual_index == last_visible_index;

            let mut agent_row = self.render_agent_row(agent, is_selected);
            if is_selected {
                agent_row = agent_row.style(Style::default().bg(Color::Rgb(40, 44, 52)));
            }

            rows.push(agent_row);

            if !is_last_overall && !is_last_visible {
                let separator = Row::new(vec![
                    Cell::from("──"),
                    Cell::from("──"),
                    Cell::from("──"),
                    Cell::from("──────────────────"),
                    Cell::from("──────────────────"),
                    Cell::from("────────"),
                    Cell::from("────────────"),
                    Cell::from("────────"),
                    Cell::from("──────────"),
                    Cell::from("──────────"),
                    Cell::from("────────"),
                    Cell::from("────────────────"),
                    Cell::from("──────────"),
                    Cell::from("──────────"),
                ])
                .style(Style::default().fg(Color::DarkGray));
                rows.push(separator);
            }
        }

        if has_below {
            let hidden_below = total_agents - scroll_offset - max_visible;
            if hidden_below > 0 {
                let indicator = Row::new(vec![
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(format!("▼ {} more below", hidden_below))
                        .style(Style::default().fg(Color::Yellow)),
                ]);
                rows.push(indicator);
            }
        }

        let table = Table::new(
            rows,
            [
                Constraint::Length(2),  // Selector
                Constraint::Length(2),  // Summary
                Constraint::Length(2),  // Continue
                Constraint::Length(26), // Name
                Constraint::Length(18), // Status
                Constraint::Length(8),  // Activity time
                Constraint::Length(12), // Sparkline
                Constraint::Length(8),  // Tasks (checklist progress)
                Constraint::Length(10), // MR
                Constraint::Length(10), // Pipeline
                Constraint::Length(10), // Server
                Constraint::Length(16), // PM Task
                Constraint::Length(10), // PM Status
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

        // Continue session column
        let continue_cell = if agent.continue_session {
            Cell::from("↻").style(Style::default().fg(Color::Cyan))
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

        // Server column
        let (server_text, server_style) = self.format_devserver_status(agent);
        let server_cell = Cell::from(server_text).style(server_style);

        // PM Task column (task name)
        let (pm_text, pm_style) = self.format_pm_task_name(agent);
        let pm_cell = Cell::from(pm_text).style(pm_style);

        // PM Status column (status name)
        let (pm_status_text, pm_status_style) = self.format_pm_status_name(agent);
        let pm_status_cell = Cell::from(pm_status_text).style(pm_status_style);

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
            continue_cell,
            name_cell,
            status_cell,
            activity_cell,
            sparkline_cell,
            tasks_cell,
            mr_cell,
            pipeline_cell,
            server_cell,
            pm_cell,
            pm_status_cell,
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
        }
    }

    fn format_mr_status(&self, agent: &Agent) -> (String, Style) {
        match self.provider {
            GitProvider::GitLab => {
                let mr_text = agent.mr_status.format_short();
                let mr_style = match &agent.mr_status {
                    MergeRequestStatus::None => Style::default().fg(Color::DarkGray),
                    MergeRequestStatus::Open { .. } => Style::default().fg(Color::Green),
                    MergeRequestStatus::Merged { .. } => Style::default().fg(Color::Magenta),
                    MergeRequestStatus::Conflicts { .. } => Style::default().fg(Color::Red),
                    MergeRequestStatus::NeedsRebase { .. } => Style::default().fg(Color::Red),
                    MergeRequestStatus::Approved { .. } => Style::default().fg(Color::Cyan),
                };
                (mr_text, mr_style)
            }
            GitProvider::GitHub => {
                let pr_text = agent.pr_status.format_short();
                let pr_style = match &agent.pr_status {
                    GitHubPullRequestStatus::None => Style::default().fg(Color::DarkGray),
                    GitHubPullRequestStatus::Open { .. } => Style::default().fg(Color::Green),
                    GitHubPullRequestStatus::Merged { .. } => Style::default().fg(Color::Magenta),
                    GitHubPullRequestStatus::Closed { .. } => Style::default().fg(Color::Red),
                    GitHubPullRequestStatus::Draft { .. } => Style::default().fg(Color::Yellow),
                };
                (pr_text, pr_style)
            }
            GitProvider::Codeberg => {
                let pr_text = agent.codeberg_pr_status.format_short();
                let pr_style = match &agent.codeberg_pr_status {
                    CodebergPullRequestStatus::None => Style::default().fg(Color::DarkGray),
                    CodebergPullRequestStatus::Open { .. } => Style::default().fg(Color::Green),
                    CodebergPullRequestStatus::Merged { .. } => Style::default().fg(Color::Cyan),
                    CodebergPullRequestStatus::Closed { .. } => Style::default().fg(Color::Red),
                    CodebergPullRequestStatus::Draft { .. } => Style::default().fg(Color::Yellow),
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

    fn format_pm_task_name(&self, agent: &Agent) -> (String, Style) {
        use crate::app::config::StatusAppearance;

        let text = agent.pm_task_status.format_short();

        // Check custom appearance config first using FULL status name
        if let Some(full_name) = agent.pm_task_status.status_name_full() {
            let provider_config = self.appearance_config.get_for_provider(self.pm_provider);
            if let Some(appearance) = provider_config.statuses.get(full_name) {
                let color = crate::ui::parse_color(&appearance.color);
                return (text, Style::default().fg(color));
            }
        }

        // Fallback: use default_for_status() based on status name
        let default_appearance = if let Some(full_name) = agent.pm_task_status.status_name_full() {
            StatusAppearance::default_for_status(full_name)
        } else {
            StatusAppearance::default()
        };
        let color = crate::ui::parse_color(&default_appearance.color);
        (text, Style::default().fg(color))
    }

    fn format_pm_status_name(&self, agent: &Agent) -> (String, Style) {
        use crate::app::config::StatusAppearance;

        let display_name = agent.pm_task_status.format_status_name();

        // Check custom appearance config first using FULL status name
        if let Some(full_name) = agent.pm_task_status.status_name_full() {
            let provider_config = self.appearance_config.get_for_provider(self.pm_provider);
            if let Some(appearance) = provider_config.statuses.get(full_name) {
                let icon = appearance.icon.clone();
                let color = crate::ui::parse_color(&appearance.color);
                let text = if icon.is_empty() {
                    display_name.clone()
                } else {
                    format!("{} {}", icon, display_name)
                };
                return (text, Style::default().fg(color));
            }
        }

        // Fallback: use default_for_status() based on status name
        let default_appearance = if let Some(full_name) = agent.pm_task_status.status_name_full() {
            StatusAppearance::default_for_status(full_name)
        } else {
            StatusAppearance::default()
        };
        let icon = default_appearance.icon.clone();
        let color = crate::ui::parse_color(&default_appearance.color);
        let text = if icon.is_empty() {
            display_name
        } else {
            format!("{} {}", icon, display_name)
        };
        (text, Style::default().fg(color))
    }

    fn format_devserver_status(&self, agent: &Agent) -> (String, Style) {
        let status = self.devserver_statuses.get(&agent.id);

        match status {
            Some(DevServerStatus::Running { .. }) => {
                ("● Running".to_string(), Style::default().fg(Color::Green))
            }
            Some(DevServerStatus::Starting) => {
                ("◐ Starting".to_string(), Style::default().fg(Color::Yellow))
            }
            Some(DevServerStatus::Stopping) => {
                ("◑ Stopping".to_string(), Style::default().fg(Color::Yellow))
            }
            Some(DevServerStatus::Failed(_)) => {
                ("✗ Failed".to_string(), Style::default().fg(Color::Red))
            }
            Some(DevServerStatus::Stopped) | None => (
                "○ Stopped".to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        }
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
