use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{
    AiAgent, CodebergCiProvider, Config, ConfigLogLevel, GitProvider, SettingsCategory,
    SettingsField, SettingsItem, SettingsState, SettingsTab, UiConfig, WorktreeLocation,
};

pub struct SettingsModal<'a> {
    state: &'a SettingsState,
    ai_agent: &'a AiAgent,
    log_level: &'a ConfigLogLevel,
    worktree_location: &'a WorktreeLocation,
    ui_config: &'a UiConfig,
}

impl<'a> SettingsModal<'a> {
    pub fn new(
        state: &'a SettingsState,
        ai_agent: &'a AiAgent,
        log_level: &'a ConfigLogLevel,
        worktree_location: &'a WorktreeLocation,
        ui_config: &'a UiConfig,
    ) -> Self {
        Self {
            state,
            ai_agent,
            log_level,
            worktree_location,
            ui_config,
        }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(70, 80, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" SETTINGS ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(2),
            ])
            .split(inner);

        self.render_tabs(frame, chunks[0]);
        self.render_fields(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);

        if let crate::app::DropdownState::Open { selected_index } = self.state.dropdown {
            self.render_dropdown(frame, selected_index);
        }
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let tabs = SettingsTab::all();
        let tab_width = area.width / tabs.len() as u16;

        let spans: Vec<Span> = tabs
            .iter()
            .flat_map(|tab| {
                let is_active = *tab == self.state.tab;
                let style = if is_active {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let name = tab.display_name();
                let padding =
                    " ".repeat((tab_width.saturating_sub(name.len() as u16 + 2) / 2) as usize);
                vec![
                    Span::styled(format!("{}{}{}", padding, name, padding), style),
                    Span::raw(" "),
                ]
            })
            .collect();

        let paragraph = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
        frame.render_widget(paragraph, area);

        let tab_line = Rect::new(area.x, area.y + 2, area.width, 1);
        let divider = Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(divider, tab_line);
    }

    fn render_fields(&self, frame: &mut Frame, area: Rect) {
        let items = self.state.all_items();
        let navigable = self.state.navigable_items();

        let selected_field_idx = navigable
            .get(self.state.field_index)
            .map(|(idx, _)| *idx)
            .unwrap_or(0);

        let mut lines = Vec::new();

        for (item_idx, item) in items.iter().enumerate() {
            match item {
                SettingsItem::Category(cat) => {
                    lines.push(self.render_category_line(cat));
                    if *cat == SettingsCategory::Asana {
                        lines.push(Self::render_token_status_line(
                            "ASANA_TOKEN",
                            Config::asana_token().is_some(),
                        ));
                    }
                }
                SettingsItem::Field(field) => {
                    let is_selected = item_idx == selected_field_idx;
                    lines.push(self.render_field_line(field, is_selected));
                    if *field == SettingsField::GitProvider {
                        match self.state.repo_config.git.provider {
                            GitProvider::GitLab => {
                                lines.push(Self::render_token_status_line(
                                    "GITLAB_TOKEN",
                                    Config::gitlab_token().is_some(),
                                ));
                            }
                            GitProvider::GitHub => {
                                lines.push(Self::render_token_status_line(
                                    "GITHUB_TOKEN",
                                    Config::github_token().is_some(),
                                ));
                            }
                            GitProvider::Codeberg => {
                                lines.push(Self::render_token_status_line(
                                    "CODEBERG_TOKEN",
                                    Config::codeberg_token().is_some(),
                                ));
                                if matches!(
                                    self.state.repo_config.git.codeberg.ci_provider,
                                    CodebergCiProvider::Woodpecker
                                ) {
                                    lines.push(Self::render_token_status_line(
                                        "WOODPECKER_TOKEN",
                                        Config::woodpecker_token().is_some(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_category_line(&self, cat: &SettingsCategory) -> Line<'static> {
        Line::from(vec![
            Span::styled("\n", Style::default()),
            Span::styled(
                format!("  ── {} ", cat.display_name()),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("─".repeat(30), Style::default().fg(Color::DarkGray)),
        ])
    }

    fn render_token_status_line(name: &str, exists: bool) -> Line<'static> {
        let (symbol, color) = if exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };
        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(
                format!("{:14}", "Token"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:34}", format!("{} ({})", symbol, name)),
                Style::default().fg(color),
            ),
        ])
    }

    fn render_field_line(&self, field: &SettingsField, is_selected: bool) -> Line<'static> {
        let (label, value, is_toggle) = self.get_field_display(field);

        let label_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let value_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        let toggle_style = if is_selected {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };

        let cursor = if is_selected && self.state.editing_text {
            "█"
        } else if is_selected {
            " ◀"
        } else {
            ""
        };

        let display_value = if self.state.editing_text && is_selected {
            self.state.text_buffer.clone()
        } else if value.len() > 30 {
            format!("{}...", &value[..27])
        } else {
            value.clone()
        };

        let final_style = if is_toggle { toggle_style } else { value_style };

        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(format!("{:14}", label), label_style),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:34}", display_value), final_style),
            Span::styled(cursor.to_string(), Style::default().fg(Color::White)),
        ])
    }

    fn get_field_display(&self, field: &SettingsField) -> (String, String, bool) {
        match field {
            SettingsField::AiAgent => (
                "AI Agent".to_string(),
                self.ai_agent.display_name().to_string(),
                false,
            ),
            SettingsField::LogLevel => (
                "Log Level".to_string(),
                self.log_level.display_name().to_string(),
                false,
            ),
            SettingsField::WorktreeLocation => (
                "Worktree Loc".to_string(),
                self.worktree_location.display_name().to_string(),
                false,
            ),
            SettingsField::ShowPreview => (
                "Preview".to_string(),
                if self.ui_config.show_preview {
                    "[x]"
                } else {
                    "[ ]"
                }
                .to_string(),
                true,
            ),
            SettingsField::ShowMetrics => (
                "Metrics".to_string(),
                if self.ui_config.show_metrics {
                    "[x]"
                } else {
                    "[ ]"
                }
                .to_string(),
                true,
            ),
            SettingsField::ShowLogs => (
                "Logs".to_string(),
                if self.ui_config.show_logs {
                    "[x]"
                } else {
                    "[ ]"
                }
                .to_string(),
                true,
            ),
            SettingsField::ShowBanner => (
                "Banner".to_string(),
                if self.ui_config.show_banner {
                    "[x]"
                } else {
                    "[ ]"
                }
                .to_string(),
                true,
            ),
            SettingsField::GitProvider => (
                "Provider".to_string(),
                self.state
                    .repo_config
                    .git
                    .provider
                    .display_name()
                    .to_string(),
                false,
            ),
            SettingsField::GitLabProjectId => (
                "Project ID".to_string(),
                self.state
                    .repo_config
                    .git
                    .gitlab
                    .project_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::GitLabBaseUrl => (
                "Base URL".to_string(),
                self.state.repo_config.git.gitlab.base_url.clone(),
                false,
            ),
            SettingsField::GitHubOwner => (
                "Owner".to_string(),
                self.state
                    .repo_config
                    .git
                    .github
                    .owner
                    .clone()
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::GitHubRepo => (
                "Repo".to_string(),
                self.state
                    .repo_config
                    .git
                    .github
                    .repo
                    .clone()
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::CodebergOwner => (
                "Owner".to_string(),
                self.state
                    .repo_config
                    .git
                    .codeberg
                    .owner
                    .clone()
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::CodebergRepo => (
                "Repo".to_string(),
                self.state
                    .repo_config
                    .git
                    .codeberg
                    .repo
                    .clone()
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::CodebergBaseUrl => (
                "Base URL".to_string(),
                self.state.repo_config.git.codeberg.base_url.clone(),
                false,
            ),
            SettingsField::CodebergCiProvider => (
                "CI Provider".to_string(),
                self.state
                    .repo_config
                    .git
                    .codeberg
                    .ci_provider
                    .display_name()
                    .to_string(),
                false,
            ),
            SettingsField::BranchPrefix => (
                "Branch Prefix".to_string(),
                self.state.repo_config.git.branch_prefix.clone(),
                false,
            ),
            SettingsField::MainBranch => (
                "Main Branch".to_string(),
                self.state.repo_config.git.main_branch.clone(),
                false,
            ),
            SettingsField::WorktreeSymlinks => (
                "Symlinks".to_string(),
                self.state.repo_config.git.worktree_symlinks.join(", "),
                false,
            ),
            SettingsField::AsanaProjectGid => (
                "Project GID".to_string(),
                self.state
                    .repo_config
                    .asana
                    .project_gid
                    .clone()
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::AsanaInProgressGid => (
                "In Progress GID".to_string(),
                self.state
                    .repo_config
                    .asana
                    .in_progress_section_gid
                    .clone()
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::AsanaDoneGid => (
                "Done GID".to_string(),
                self.state
                    .repo_config
                    .asana
                    .done_section_gid
                    .clone()
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::DevServerCommand => (
                "Command".to_string(),
                self.state
                    .repo_config
                    .dev_server
                    .command
                    .clone()
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::DevServerRunBefore => (
                "Run Before".to_string(),
                self.state.repo_config.dev_server.run_before.join(", "),
                false,
            ),
            SettingsField::DevServerWorkingDir => (
                "Working Dir".to_string(),
                self.state.repo_config.dev_server.working_dir.clone(),
                false,
            ),
            SettingsField::DevServerPort => (
                "Port".to_string(),
                self.state
                    .repo_config
                    .dev_server
                    .port
                    .map(|p| p.to_string())
                    .unwrap_or_default(),
                false,
            ),
            SettingsField::DevServerAutoStart => (
                "Auto Start".to_string(),
                if self.state.repo_config.dev_server.auto_start {
                    "[x]"
                } else {
                    "[ ]"
                }
                .to_string(),
                true,
            ),
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.state.editing_text {
            "[Enter] Save  [Esc] Cancel"
        } else if self.state.is_dropdown_open() {
            "[↑/↓] Navigate  [Enter] Select  [Esc] Cancel"
        } else {
            let field = self.state.current_field();
            let is_toggle = matches!(
                field,
                SettingsField::ShowPreview
                    | SettingsField::ShowMetrics
                    | SettingsField::ShowLogs
                    | SettingsField::ShowBanner
                    | SettingsField::DevServerAutoStart
            );
            if is_toggle {
                "[Tab] Switch tab  [Enter] Toggle  [↑/↓] Navigate  [Esc] Close"
            } else {
                "[Tab] Switch tab  [Enter] Edit  [↑/↓] Navigate  [Esc] Close  [q] Save"
            }
        };

        let paragraph = Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }

    fn render_dropdown(&self, frame: &mut Frame, selected_index: usize) {
        let field = self.state.current_field();
        let options: Vec<&str> = match field {
            SettingsField::AiAgent => AiAgent::all().iter().map(|a| a.display_name()).collect(),
            SettingsField::GitProvider => GitProvider::all()
                .iter()
                .map(|g| g.display_name())
                .collect(),
            SettingsField::LogLevel => ConfigLogLevel::all()
                .iter()
                .map(|l| l.display_name())
                .collect(),
            SettingsField::WorktreeLocation => WorktreeLocation::all()
                .iter()
                .map(|w| w.display_name())
                .collect(),
            SettingsField::CodebergCiProvider => CodebergCiProvider::all()
                .iter()
                .map(|c| c.display_name())
                .collect(),
            _ => return,
        };

        let navigable = self.state.navigable_items();
        let selected_item_idx = navigable
            .get(self.state.field_index)
            .map(|(idx, _)| *idx)
            .unwrap_or(0);

        let area = get_dropdown_position(frame.area(), selected_item_idx);
        frame.render_widget(Clear, area);

        let lines: Vec<Line> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let style = if i == selected_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(format!(" {} ", opt), style))
            })
            .collect();

        let height = lines.len() as u16 + 2;
        let dropdown_area = Rect::new(area.x, area.y, area.width, height.min(area.height));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, dropdown_area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn get_dropdown_position(frame_area: Rect, item_index: usize) -> Rect {
    let modal_area = centered_rect(70, 80, frame_area);
    let base_y = modal_area.y + 4;
    let row_offset = item_index as u16;
    Rect::new(modal_area.x + 22, base_y + row_offset, 20, 10)
}
