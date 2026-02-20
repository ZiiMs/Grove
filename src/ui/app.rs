use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{AppState, InputMode, LogLevel, PreviewTab};
use crate::devserver::DevServerStatus;

use super::components::{
    render_confirm_modal, render_input_modal, AgentListWidget, DevServerViewWidget,
    DevServerWarningModal, EmptyDevServerWidget, EmptyOutputWidget, GlobalSetupWizard, HelpOverlay,
    LoadingOverlay, OutputViewWidget, ProjectSetupWizard, SettingsModal, StatusBarWidget,
    SystemMetricsWidget, TaskListModal,
};

#[derive(Clone)]
pub struct DevServerRenderInfo {
    pub status: DevServerStatus,
    pub logs: Vec<String>,
    pub agent_name: String,
}

const BANNER: &[&str] = &[
    "",
    " ███████╗██╗      ██████╗  ██████╗██╗  ██╗",
    " ██╔════╝██║     ██╔═══██╗██╔════╝██║ ██╔╝",
    " █████╗  ██║     ██║   ██║██║     █████╔╝ ",
    " ██╔══╝  ██║     ██║   ██║██║     ██╔═██╗ ",
    " ██║     ███████╗╚██████╔╝╚██████╗██║  ██╗",
    " ╚═╝     ╚══════╝ ╚═════╝  ╚═════╝╚═╝  ╚═╝",
    "",
];

pub struct AppWidget<'a> {
    state: &'a AppState,
    devserver_info: Option<DevServerRenderInfo>,
}

impl<'a> AppWidget<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self {
            state,
            devserver_info: None,
        }
    }

    pub fn with_devserver(mut self, info: Option<DevServerRenderInfo>) -> Self {
        self.devserver_info = info;
        self
    }

    pub fn render(self, frame: &mut Frame) {
        let size = frame.area();

        let show_banner = self.state.config.ui.show_banner;
        let show_preview = self.state.config.ui.show_preview;
        let show_metrics = self.state.config.ui.show_metrics;
        let show_logs = self.state.config.ui.show_logs;
        let has_message = self.state.error_message.is_some();

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
        if has_message {
            constraints.push(Constraint::Length(3));
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

        if has_message {
            self.render_message(frame, chunks[chunk_idx]);
            chunk_idx += 1;
        }

        self.render_footer(frame, chunks[chunk_idx]);

        if self.state.show_help {
            HelpOverlay::render(frame, size);
        }

        if self.state.settings.active {
            SettingsModal::new(
                &self.state.settings,
                &self.state.config.global.ai_agent,
                &self.state.config.global.log_level,
                &self.state.config.global.worktree_location,
                &self.state.config.ui,
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

        if let Some(warning) = &self.state.devserver_warning {
            DevServerWarningModal::new(warning).render(frame);
        } else if let Some(mode) = &self.state.input_mode {
            self.render_modal(frame, mode, size);
        }

        if let Some(message) = &self.state.loading_message {
            LoadingOverlay::render(frame, message, self.state.animation_frame);
        }
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
                TaskListModal::new(
                    &self.state.task_list,
                    self.state.task_list_selected,
                    self.state.task_list_loading,
                    provider_name,
                )
                .render(frame);
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
                self.state.animation_frame,
                self.state.settings.repo_config.git.provider,
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

        if let Some(agent) = self.state.selected_agent() {
            if matches!(agent.status, crate::agent::AgentStatus::Paused) {
                self.render_paused_preview(frame, chunks[1], &agent.name);
                return;
            }
        }

        match self.state.preview_tab {
            PreviewTab::Preview => self.render_preview_content(frame, chunks[1]),
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

    fn render_paused_preview(&self, frame: &mut Frame, area: Rect, agent_name: &str) {
        let paused_art = vec![
            "",
            "",
            "  ██████╗  █████╗ ██╗   ██╗███████╗███████╗██████╗ ",
            "  ██╔══██╗██╔══██╗██║   ██║██╔════╝██╔════╝██╔══██╗",
            "  ██████╔╝███████║██║   ██║███████╗█████╗  ██║  ██║",
            "  ██╔═══╝ ██╔══██║██║   ██║╚════██║██╔══╝  ██║  ██║",
            "  ██║     ██║  ██║╚██████╔╝███████║███████╗██████╔╝",
            "  ╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚══════╝╚═════╝ ",
            "",
            "",
            "  Press 'r' to resume this agent",
        ];

        let lines: Vec<Line> = paused_art
            .iter()
            .map(|&line| Line::from(Span::styled(line, Style::default().fg(Color::Yellow))))
            .collect();

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(" PREVIEW: {} ", agent_name))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, area);
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

    fn render_message(&self, frame: &mut Frame, area: Rect) {
        if let Some(msg) = &self.state.error_message {
            let is_error = msg.contains("Error") || msg.contains("Failed") || msg.contains("error");
            let (border_color, text_color) = if is_error {
                (Color::Red, Color::Red)
            } else {
                (Color::Yellow, Color::Yellow)
            };

            let paragraph = Paragraph::new(Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(text_color).add_modifier(Modifier::BOLD),
            )))
            .block(
                Block::default()
                    .title(" EVENT ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color)),
            );
            frame.render_widget(paragraph, area);
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        StatusBarWidget::new(None, false).render(frame, area);
    }
}
