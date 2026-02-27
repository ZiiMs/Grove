use std::collections::HashMap;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use uuid::Uuid;

use crate::app::{AppState, InputMode, LogLevel, PreviewTab};
use crate::devserver::DevServerStatus;

use super::components::{
    render_confirm_modal, render_input_modal, AgentListWidget, DevServerViewWidget,
    DevServerWarningModal, DiffViewWidget, EmptyDevServerWidget, EmptyDiffWidget,
    EmptyOutputWidget, GitSetupModal, GlobalSetupWizard, HelpOverlay, LoadingOverlay,
    OutputViewWidget, PmSetupModal, PmStatusDebugOverlay, ProjectSetupWizard, SettingsModal,
    StatusBarWidget, StatusDebugOverlay, StatusDropdown, SystemMetricsWidget, TaskListModal,
    TaskReassignmentWarningModal, ToastWidget, TutorialWizard,
};

#[derive(Clone)]
pub struct DevServerRenderInfo {
    pub status: DevServerStatus,
    pub logs: Vec<String>,
    pub agent_name: String,
}

const BANNER: &[&str] = &[
    "",
    " ██████╗ ██████╗  ██████╗ ██╗   ██╗███████╗",
    " ██╔════╝ ██╔══██╗██╔═══██╗██║   ██║██╔════╝",
    " ██║  ███╗██████╔╝██║   ██║██║   ██║█████╗  ",
    " ██║   ██║██╔══██╗██║   ██║╚██╗ ██╔╝██╔══╝  ",
    " ╚██████╔╝██║  ██║╚██████╔╝ ╚████╔╝ ███████╗",
    "  ╚═════╝ ╚═╝  ╚═╝ ╚═════╝   ╚═══╝  ╚══════╝",
    "",
];

pub struct AppWidget<'a> {
    state: &'a AppState,
    devserver_info: Option<DevServerRenderInfo>,
    devserver_statuses: HashMap<Uuid, DevServerStatus>,
}

impl<'a> AppWidget<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self {
            state,
            devserver_info: None,
            devserver_statuses: HashMap::new(),
        }
    }

    pub fn with_devserver(mut self, info: Option<DevServerRenderInfo>) -> Self {
        self.devserver_info = info;
        self
    }

    pub fn with_devserver_statuses(mut self, statuses: HashMap<Uuid, DevServerStatus>) -> Self {
        self.devserver_statuses = statuses;
        self
    }

    pub fn render(self, frame: &mut Frame) {
        let size = frame.area();

        let show_banner = self.state.config.ui.show_banner;
        let show_preview = self.state.config.ui.show_preview;
        let show_metrics = self.state.config.ui.show_metrics;
        let show_logs = self.state.config.ui.show_logs;

        let agent_count = self.state.agents.len().max(1);
        let agent_list_height = ((agent_count * 2) + 3).min(size.height as usize / 3) as u16;

        let mut constraints: Vec<Constraint> = Vec::new();

        if show_banner {
            constraints.push(Constraint::Length(8));
        }
        constraints.push(Constraint::Length(agent_list_height));
        if show_preview {
            constraints.push(Constraint::Min(8));
        }
        if show_metrics {
            constraints.push(Constraint::Length(6));
        }
        if show_logs {
            constraints.push(Constraint::Length(6));
        }
        constraints.push(Constraint::Length(1));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(size);

        let mut chunk_idx = 0;

        if show_banner {
            self.render_banner(frame, chunks[chunk_idx]);
            chunk_idx += 1;
        }

        self.render_agent_list(frame, chunks[chunk_idx]);
        chunk_idx += 1;

        if show_preview {
            self.render_preview(frame, chunks[chunk_idx]);
            chunk_idx += 1;
        }

        if show_metrics {
            self.render_system_metrics(frame, chunks[chunk_idx]);
            chunk_idx += 1;
        }

        if show_logs {
            self.render_logs(frame, chunks[chunk_idx]);
            chunk_idx += 1;
        }

        self.render_footer(frame, chunks[chunk_idx]);

        if self.state.show_help {
            HelpOverlay::new(&self.state.config.keybinds).render(frame, size);
        }

        if self.state.settings.active {
            SettingsModal::new(
                &self.state.settings,
                &self.state.config.global.ai_agent,
                &self.state.config.global.log_level,
                &self.state.config.global.worktree_location,
                &self.state.config.ui,
                &self.state.settings.pending_automation,
            )
            .render(frame);
        }

        if self.state.show_global_setup {
            if let Some(wizard_state) = &self.state.global_setup {
                let wizard = GlobalSetupWizard::new(wizard_state);
                wizard.render(frame);
            }
        }

        if self.state.show_project_setup {
            if let Some(wizard_state) = &self.state.project_setup {
                let repo_name = std::path::Path::new(&self.state.repo_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Project");
                let wizard = ProjectSetupWizard::new(wizard_state, repo_name);
                wizard.render(frame);
            }
        }

        if self.state.show_tutorial {
            if let Some(tutorial_state) = &self.state.tutorial {
                let wizard = TutorialWizard::new(tutorial_state);
                wizard.render(frame);
            }
        }

        if self.state.pm_setup.active {
            let provider = self.state.settings.repo_config.project_mgmt.provider;
            PmSetupModal::new(&self.state.pm_setup, provider).render(frame);
        }

        if self.state.git_setup.active {
            let provider = self.state.settings.repo_config.git.provider;
            GitSetupModal::new(&self.state.git_setup, provider).render(frame);
        }

        if let Some(message) = &self.state.loading_message {
            LoadingOverlay::render(frame, message, self.state.animation_frame);
        }

        if let Some(warning) = &self.state.task_reassignment_warning {
            TaskReassignmentWarningModal::new(warning, &self.state.agents).render(frame);
        } else if let Some(warning) = &self.state.devserver_warning {
            DevServerWarningModal::new(warning).render(frame);
        } else if let Some(mode) = &self.state.input_mode {
            self.render_modal(frame, mode, size);
        }

        if self.state.show_status_debug {
            if let Some(agent) = self.state.selected_agent() {
                StatusDebugOverlay::new(agent).render(frame, size);
            }
        }

        if self.state.pm_status_debug.active {
            let configured_providers = self.get_configured_pm_providers();
            PmStatusDebugOverlay::new(&self.state.pm_status_debug, &configured_providers)
                .render(frame, size);
        }

        if let Some(toast) = &self.state.toast {
            ToastWidget::new(toast).render(frame);
        }
    }

    fn get_configured_pm_providers(&self) -> Vec<crate::app::config::ProjectMgmtProvider> {
        use crate::app::config::ProjectMgmtProvider;

        let mut configured = Vec::new();

        if self
            .state
            .settings
            .repo_config
            .project_mgmt
            .asana
            .project_gid
            .is_some()
        {
            configured.push(ProjectMgmtProvider::Asana);
        }
        if self
            .state
            .settings
            .repo_config
            .project_mgmt
            .notion
            .database_id
            .is_some()
        {
            configured.push(ProjectMgmtProvider::Notion);
        }
        if self
            .state
            .settings
            .repo_config
            .project_mgmt
            .clickup
            .list_id
            .is_some()
        {
            configured.push(ProjectMgmtProvider::Clickup);
        }
        if self
            .state
            .settings
            .repo_config
            .project_mgmt
            .airtable
            .base_id
            .is_some()
        {
            configured.push(ProjectMgmtProvider::Airtable);
        }
        if self
            .state
            .settings
            .repo_config
            .project_mgmt
            .linear
            .team_id
            .is_some()
        {
            configured.push(ProjectMgmtProvider::Linear);
        }

        configured
    }

    fn render_modal(&self, frame: &mut Frame, mode: &InputMode, _area: Rect) {
        match mode {
            InputMode::NewAgent => {
                render_input_modal(frame, "New Agent", "Enter name:", &self.state.input_buffer);
            }
            InputMode::SetNote => {
                render_input_modal(
                    frame,
                    "Set Note",
                    "Enter note for agent:",
                    &self.state.input_buffer,
                );
            }
            InputMode::ConfirmDelete => {
                let agent_name = self
                    .state
                    .selected_agent()
                    .map(|a| a.name.as_str())
                    .unwrap_or("agent");
                render_confirm_modal(
                    frame,
                    "Delete Agent",
                    &format!("Delete agent '{}'?", agent_name),
                    "y",
                    "Esc",
                );
            }
            InputMode::ConfirmMerge => {
                let agent_name = self
                    .state
                    .selected_agent()
                    .map(|a| a.name.as_str())
                    .unwrap_or("agent");
                render_confirm_modal(
                    frame,
                    "Merge Main",
                    &format!("Send merge main request to '{}'?", agent_name),
                    "y",
                    "Esc",
                );
            }
            InputMode::ConfirmPush => {
                let agent_name = self
                    .state
                    .selected_agent()
                    .map(|a| a.name.as_str())
                    .unwrap_or("agent");
                render_confirm_modal(
                    frame,
                    "Push",
                    &format!("Push changes from '{}'?", agent_name),
                    "y",
                    "Esc",
                );
            }
            InputMode::ConfirmDeleteAsana => {
                let agent_name = self
                    .state
                    .selected_agent()
                    .map(|a| a.name.as_str())
                    .unwrap_or("agent");
                render_confirm_modal(
                    frame,
                    "Delete Agent",
                    &format!(
                        "Delete '{}'? Complete Asana task? [y]es [n]o [Esc]cancel",
                        agent_name
                    ),
                    "y",
                    "n/Esc",
                );
            }
            InputMode::ConfirmDeleteTask => {
                let agent_name = self
                    .state
                    .selected_agent()
                    .map(|a| a.name.as_str())
                    .unwrap_or("agent");
                render_confirm_modal(
                    frame,
                    "Delete Agent",
                    &format!(
                        "Delete '{}'? Complete task? [y]es [n]o [Esc]cancel",
                        agent_name
                    ),
                    "y",
                    "n/Esc",
                );
            }
            InputMode::AssignAsana => {
                render_input_modal(
                    frame,
                    "Assign Asana Task",
                    "Enter Asana task URL or GID:",
                    &self.state.input_buffer,
                );
            }
            InputMode::AssignProjectTask => {
                let provider_name = self
                    .state
                    .settings
                    .repo_config
                    .project_mgmt
                    .provider
                    .display_name();
                render_input_modal(
                    frame,
                    &format!("Assign {} Task", provider_name),
                    &format!("Enter {} task URL or ID:", provider_name),
                    &self.state.input_buffer,
                );
            }
            InputMode::BrowseTasks => {
                let provider_name = self
                    .state
                    .settings
                    .repo_config
                    .project_mgmt
                    .provider
                    .display_name();

                fn normalize_id(id: &str) -> String {
                    id.replace('-', "").to_lowercase()
                }

                let assigned_tasks: HashMap<String, String> = self
                    .state
                    .agents
                    .values()
                    .filter_map(|a| {
                        a.pm_task_status
                            .id()
                            .map(|id| (normalize_id(id), a.name.clone()))
                    })
                    .collect();
                TaskListModal::new(
                    &self.state.task_list,
                    self.state.task_list_selected,
                    self.state.task_list_scroll,
                    self.state.task_list_loading,
                    provider_name,
                    self.state.settings.repo_config.project_mgmt.provider,
                    &self.state.settings.pending_appearance,
                    &assigned_tasks,
                    &self.state.task_list_expanded_ids,
                    &self.state.config.task_list.hidden_status_names,
                    &self.state.task_list_status_options,
                    self.state.task_list_filter_open,
                    self.state.task_list_filter_selected,
                )
                .render(frame);
            }
            InputMode::SelectTaskStatus => {
                if let Some(dropdown) = &self.state.task_status_dropdown {
                    StatusDropdown::new(dropdown).render(frame);
                }
            }
        }
    }

    fn render_banner(&self, frame: &mut Frame, area: Rect) {
        let lines: Vec<Line> = BANNER
            .iter()
            .map(|&line| Line::from(Span::styled(line, Style::default().fg(Color::White))))
            .collect();

        let banner = Paragraph::new(lines).alignment(Alignment::Left);
        frame.render_widget(banner, area);
    }

    fn render_agent_list(&self, frame: &mut Frame, area: Rect) {
        let agents: Vec<&_> = self
            .state
            .agent_order
            .iter()
            .filter_map(|id| self.state.agents.get(id))
            .collect();

        if agents.is_empty() {
            let empty = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No agents yet",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Press 'n' to create one",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .block(
                Block::default()
                    .title(" AGENTS (0) ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White)),
            );
            frame.render_widget(empty, area);
        } else {
            AgentListWidget::new(
                &agents,
                self.state.selected_index,
                self.state.agent_list_scroll,
                self.state.animation_frame,
                self.state.settings.repo_config.git.provider,
                &self.devserver_statuses,
                &self.state.settings.pending_appearance,
                self.state.settings.repo_config.project_mgmt.provider,
            )
            .with_count(self.state.agents.len())
            .render(frame, area);
        }
    }

    fn render_preview(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(8)])
            .split(area);

        self.render_preview_tabs(frame, chunks[0]);

        match self.state.preview_tab {
            PreviewTab::Preview => self.render_preview_content(frame, chunks[1]),
            PreviewTab::GitDiff => self.render_gitdiff_content(frame, chunks[1]),
            PreviewTab::DevServer => self.render_devserver_content(frame, chunks[1]),
        }
    }

    fn render_preview_tabs(&self, frame: &mut Frame, area: Rect) {
        let preview_style = if self.state.preview_tab == PreviewTab::Preview {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let gitdiff_style = if self.state.preview_tab == PreviewTab::GitDiff {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let has_running = self
            .devserver_info
            .as_ref()
            .map(|info| info.status.is_running())
            .unwrap_or(false);
        let devserver_indicator = if has_running { " *" } else { "" };

        let devserver_style = if self.state.preview_tab == PreviewTab::DevServer {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let tabs = Line::from(vec![
            Span::styled(" Preview ", preview_style),
            Span::raw(" "),
            Span::styled(" Git Diff ", gitdiff_style),
            Span::raw(" "),
            Span::styled(
                format!(" Dev Server{} ", devserver_indicator),
                devserver_style,
            ),
        ]);

        let paragraph = Paragraph::new(tabs);
        frame.render_widget(paragraph, area);
    }

    fn render_preview_content(&self, frame: &mut Frame, area: Rect) {
        if let Some(content) = &self.state.preview_content {
            let agent_name = self
                .state
                .selected_agent()
                .map(|a| a.name.as_str())
                .unwrap_or("Preview");
            let title = format!("PREVIEW: {}", agent_name);
            OutputViewWidget::new(&title, content)
                .with_scroll(self.state.output_scroll)
                .render(frame, area);
        } else {
            EmptyOutputWidget::render(frame, area);
        }
    }

    fn render_devserver_content(&self, frame: &mut Frame, area: Rect) {
        if let Some(info) = &self.devserver_info {
            DevServerViewWidget::new(
                info.status.clone(),
                info.logs.clone(),
                info.agent_name.clone(),
            )
            .render(frame, area);
        } else {
            EmptyDevServerWidget::render(frame, area);
        }
    }

    fn render_gitdiff_content(&self, frame: &mut Frame, area: Rect) {
        let agent_name = self
            .state
            .selected_agent()
            .map(|a| a.name.as_str())
            .unwrap_or("Agent");
        if let Some(content) = &self.state.gitdiff_content {
            DiffViewWidget::new(agent_name, content, self.state.gitdiff_scroll).render(frame, area);
        } else {
            EmptyDiffWidget::render(frame, area);
        }
    }

    fn render_system_metrics(&self, frame: &mut Frame, area: Rect) {
        SystemMetricsWidget::new(
            &self.state.cpu_history,
            &self.state.memory_history,
            self.state.memory_used,
            self.state.memory_total,
        )
        .render(frame, area);
    }

    fn render_logs(&self, frame: &mut Frame, area: Rect) {
        let visible_lines = (area.height.saturating_sub(2)) as usize;

        let lines: Vec<Line> = self
            .state
            .logs
            .iter()
            .rev()
            .take(visible_lines)
            .map(|entry| {
                let time = entry.timestamp.format("%H:%M:%S");
                let (level_str, level_color) = match entry.level {
                    LogLevel::Info => ("INFO", Color::Green),
                    LogLevel::Warn => ("WARN", Color::Yellow),
                    LogLevel::Error => ("ERR ", Color::Red),
                    LogLevel::Debug => ("DBG ", Color::DarkGray),
                };

                Line::from(vec![
                    Span::styled(format!("{} ", time), Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("[{}] ", level_str),
                        Style::default().fg(level_color),
                    ),
                    Span::raw(entry.message.clone()),
                ])
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .title(" LOGS ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        StatusBarWidget::new(None, false, &self.state.config.keybinds).render(frame, area);
    }
}
