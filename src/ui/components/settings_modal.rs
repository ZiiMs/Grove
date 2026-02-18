use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{AiAgent, ConfigLogLevel, GitProvider, SettingsField, SettingsState, SettingsTab};

pub struct SettingsModal<'a> {
    state: &'a SettingsState,
    ai_agent: &'a AiAgent,
    log_level: &'a ConfigLogLevel,
}

impl<'a> SettingsModal<'a> {
    pub fn new(
        state: &'a SettingsState,
        ai_agent: &'a AiAgent,
        log_level: &'a ConfigLogLevel,
    ) -> Self {
        Self {
            state,
            ai_agent,
            log_level,
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
        match self.state.tab {
            SettingsTab::General => self.render_general_fields(frame, area),
            SettingsTab::Git => self.render_git_fields(frame, area),
            SettingsTab::ProjectMgmt => self.render_asana_fields(frame, area),
        }
    }

    fn render_general_fields(&self, frame: &mut Frame, area: Rect) {
        let fields = [
            ("AI Agent", self.ai_agent.display_name()),
            ("Log Level", self.log_level.display_name()),
        ];

        let lines: Vec<Line> = fields
            .iter()
            .enumerate()
            .map(|(i, (label, value))| {
                let is_selected = self.state.field_index == i;
                self.render_field_line(label, value, is_selected, false)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_git_fields(&self, frame: &mut Frame, area: Rect) {
        let provider = self.state.repo_config.git.provider;
        let fields = SettingsField::all_for_tab(SettingsTab::Git, provider);

        let mut lines = Vec::new();

        for (i, field) in fields.iter().enumerate() {
            let is_selected = self.state.field_index == i;
            let (label, value) = self.get_git_field_display(field);
            let is_editable = !matches!(field, SettingsField::GitProvider);
            lines.push(self.render_field_line(&label, &value, is_selected, is_editable));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn get_git_field_display(&self, field: &SettingsField) -> (String, String) {
        match field {
            SettingsField::GitProvider => (
                "Provider".to_string(),
                self.state
                    .repo_config
                    .git
                    .provider
                    .display_name()
                    .to_string(),
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
            ),
            SettingsField::GitLabBaseUrl => (
                "Base URL".to_string(),
                self.state.repo_config.git.gitlab.base_url.clone(),
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
            ),
            SettingsField::BranchPrefix => (
                "Branch Prefix".to_string(),
                self.state.repo_config.git.branch_prefix.clone(),
            ),
            SettingsField::MainBranch => (
                "Main Branch".to_string(),
                self.state.repo_config.git.main_branch.clone(),
            ),
            SettingsField::WorktreeSymlinks => (
                "Symlinks".to_string(),
                self.state.repo_config.git.worktree_symlinks.join(", "),
            ),
            _ => ("Unknown".to_string(), String::new()),
        }
    }

    fn render_asana_fields(&self, frame: &mut Frame, area: Rect) {
        let project_gid = self
            .state
            .repo_config
            .asana
            .project_gid
            .as_deref()
            .unwrap_or("");
        let in_progress_gid = self
            .state
            .repo_config
            .asana
            .in_progress_section_gid
            .as_deref()
            .unwrap_or("");
        let done_gid = self
            .state
            .repo_config
            .asana
            .done_section_gid
            .as_deref()
            .unwrap_or("");

        let fields = [
            ("Project GID", project_gid),
            ("In Progress GID", in_progress_gid),
            ("Done GID", done_gid),
        ];

        let lines: Vec<Line> = fields
            .iter()
            .enumerate()
            .map(|(i, (label, value))| {
                let is_selected = self.state.field_index == i;
                let display_value = if self.state.editing_text && is_selected {
                    &self.state.text_buffer
                } else {
                    *value
                };
                self.render_field_line(label, display_value, is_selected, true)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_field_line(
        &self,
        label: &str,
        value: &str,
        is_selected: bool,
        _is_editable: bool,
    ) -> Line<'static> {
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

        let cursor = if is_selected && self.state.editing_text {
            "█"
        } else if is_selected {
            " ◀"
        } else {
            ""
        };

        let truncated_value = if value.len() > 30 {
            format!("{}...", &value[..27])
        } else if self.state.editing_text && is_selected {
            self.state.text_buffer.clone()
        } else {
            value.to_string()
        };

        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(format!("{:16}", label), label_style),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:34}", truncated_value), value_style),
            Span::styled(cursor.to_string(), Style::default().fg(Color::White)),
        ])
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.state.editing_text {
            "[Enter] Save  [Esc] Cancel"
        } else if self.state.is_dropdown_open() {
            "[↑/↓] Navigate  [Enter] Select  [Esc] Cancel"
        } else {
            "[Tab] Switch tab  [Enter] Edit  [↑/↓] Navigate  [Esc] Close  [q] Save & Close"
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
            _ => return,
        };

        let area = get_dropdown_position(frame.area(), self.state.field_index);
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

fn get_dropdown_position(frame_area: Rect, field_index: usize) -> Rect {
    let modal_area = centered_rect(70, 80, frame_area);
    let base_y = modal_area.y + 4;
    let row_offset = field_index as u16;
    Rect::new(modal_area.x + 22, base_y + row_offset, 20, 10)
}
