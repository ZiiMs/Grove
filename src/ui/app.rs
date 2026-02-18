use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{AppState, InputMode, LogLevel};

use super::components::{
    render_confirm_modal, render_input_modal, AgentListWidget, EmptyOutputWidget, HelpOverlay,
    LoadingOverlay, OutputViewWidget, SettingsModal, StatusBarWidget, SystemMetricsWidget,
};

/// ASCII art banner for FLOCK (with padding)
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

/// Main application UI renderer.
pub struct AppWidget<'a> {
    state: &'a AppState,
}

impl<'a> AppWidget<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub fn render(self, frame: &mut Frame) {
        let size = frame.area();

        // Calculate dynamic agent list height based on agent count
        // Each agent takes 2 lines, plus 3 for border and header
        let agent_count = self.state.agents.len().max(1);
        let agent_list_height = ((agent_count * 2) + 3).min(size.height as usize / 3) as u16;

        // Main layout: banner, agent list (dynamic), preview (fills space), system metrics, logs (optional), message (optional), footer
        let has_message = self.state.error_message.is_some();
        let main_chunks = if self.state.show_logs {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints(if has_message {
                    vec![
                        Constraint::Length(8),                 // Banner
                        Constraint::Length(agent_list_height), // Agent list (dynamic)
                        Constraint::Min(8),                    // Preview pane (fills remaining)
                        Constraint::Length(6),                 // System metrics (CPU/Memory graphs)
                        Constraint::Length(6),                 // Log panel
                        Constraint::Length(3),                 // Message (with border)
                        Constraint::Length(1),                 // Status bar
                    ]
                } else {
                    vec![
                        Constraint::Length(8),                 // Banner
                        Constraint::Length(agent_list_height), // Agent list (dynamic)
                        Constraint::Min(8),                    // Preview pane (fills remaining)
                        Constraint::Length(6),                 // System metrics (CPU/Memory graphs)
                        Constraint::Length(6),                 // Log panel
                        Constraint::Length(1),                 // Status bar
                    ]
                })
                .split(size)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints(if has_message {
                    vec![
                        Constraint::Length(8),                 // Banner
                        Constraint::Length(agent_list_height), // Agent list (dynamic)
                        Constraint::Min(10),                   // Preview pane (fills remaining)
                        Constraint::Length(6),                 // System metrics (CPU/Memory graphs)
                        Constraint::Length(3),                 // Message (with border)
                        Constraint::Length(1),                 // Status bar
                    ]
                } else {
                    vec![
                        Constraint::Length(8),                 // Banner
                        Constraint::Length(agent_list_height), // Agent list (dynamic)
                        Constraint::Min(12),                   // Preview pane (fills remaining)
                        Constraint::Length(6),                 // System metrics (CPU/Memory graphs)
                        Constraint::Length(1),                 // Status bar
                    ]
                })
                .split(size)
        };

        // Render banner
        self.render_banner(frame, main_chunks[0]);

        // Render agent list
        self.render_agent_list(frame, main_chunks[1]);

        // Render preview pane
        self.render_preview(frame, main_chunks[2]);

        // Render system metrics (CPU/Memory graphs)
        self.render_system_metrics(frame, main_chunks[3]);

        let has_message = self.state.error_message.is_some();

        if self.state.show_logs {
            // Render log panel
            self.render_logs(frame, main_chunks[4]);
            if has_message {
                self.render_message(frame, main_chunks[5]);
                self.render_footer(frame, main_chunks[6]);
            } else {
                self.render_footer(frame, main_chunks[5]);
            }
        } else if has_message {
            self.render_message(frame, main_chunks[4]);
            self.render_footer(frame, main_chunks[5]);
        } else {
            self.render_footer(frame, main_chunks[4]);
        }

        // Render help overlay if active
        if self.state.show_help {
            HelpOverlay::render(frame, size);
        }

        // Render settings modal if active
        if self.state.settings.active {
            SettingsModal::new(
                &self.state.settings,
                &self.state.settings.pending_ai_agent,
                &self.state.settings.pending_log_level,
            )
            .render(frame);
        }

        // Render modal if in input mode
        if let Some(mode) = &self.state.input_mode {
            self.render_modal(frame, mode, size);
        }

        // Render loading overlay if loading
        if let Some(message) = &self.state.loading_message {
            LoadingOverlay::render(frame, message, self.state.animation_frame);
        }
    }

    fn render_modal(&self, frame: &mut Frame, mode: &InputMode, _area: Rect) {
        match mode {
            InputMode::NewAgent => {
                render_input_modal(
                    frame,
                    "New Agent",
                    "Enter branch name:",
                    &self.state.input_buffer,
                );
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
            InputMode::AssignAsana => {
                render_input_modal(
                    frame,
                    "Assign Asana Task",
                    "Enter Asana task URL or GID:",
                    &self.state.input_buffer,
                );
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
        // Check if selected agent is paused
        if let Some(agent) = self.state.selected_agent() {
            if matches!(agent.status, crate::agent::AgentStatus::Paused) {
                self.render_paused_preview(frame, area, &agent.name);
                return;
            }
        }

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
        // Always show keybindings - modals handle input display
        StatusBarWidget::new(None, false).render(frame, area);
    }
}
