use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::config::GitProvider;
use crate::app::ProjectSetupState;

pub struct ProjectSetupWizard<'a> {
    state: &'a ProjectSetupState,
    repo_name: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectField {
    GitProvider,
    GitLabProjectId,
    GitLabBaseUrl,
    GitHubOwner,
    GitHubRepo,
    CodebergOwner,
    CodebergRepo,
    CodebergBaseUrl,
    BranchPrefix,
    MainBranch,
    AsanaProjectGid,
}

impl<'a> ProjectSetupWizard<'a> {
    pub fn new(state: &'a ProjectSetupState, repo_name: &'a str) -> Self {
        Self { state, repo_name }
    }

    pub fn fields(&self) -> Vec<ProjectField> {
        let mut fields = vec![ProjectField::GitProvider];
        match self.state.config.git.provider {
            GitProvider::GitLab => {
                fields.push(ProjectField::GitLabProjectId);
                fields.push(ProjectField::GitLabBaseUrl);
            }
            GitProvider::GitHub => {
                fields.push(ProjectField::GitHubOwner);
                fields.push(ProjectField::GitHubRepo);
            }
            GitProvider::Codeberg => {
                fields.push(ProjectField::CodebergOwner);
                fields.push(ProjectField::CodebergRepo);
                fields.push(ProjectField::CodebergBaseUrl);
            }
        }
        fields.push(ProjectField::BranchPrefix);
        fields.push(ProjectField::MainBranch);
        fields.push(ProjectField::AsanaProjectGid);
        fields
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = centered_rect(60, 60, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Project Setup ")
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

        self.render_header(frame, chunks[0]);
        self.render_fields(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);

        if self.state.dropdown_open {
            self.render_dropdown(frame);
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    self.repo_name,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        ];

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_fields(&self, frame: &mut Frame, area: Rect) {
        let fields = self.fields();
        let mut lines = vec![Line::from("")];

        lines.push(Line::from(Span::styled(
            "  Git Provider",
            Style::default().fg(Color::DarkGray),
        )));

        for (idx, field) in fields.iter().enumerate() {
            let is_selected = idx == self.state.field_index;
            // Add section headers
            if matches!(field, ProjectField::BranchPrefix) {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  Configuration",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            if matches!(field, ProjectField::AsanaProjectGid) {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  Asana (optional)",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            lines.push(self.render_field_line(field, is_selected));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_field_line(&self, field: &ProjectField, is_selected: bool) -> Line<'static> {
        let (label, value) = self.get_field_display(field);

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

        let display_value = if self.state.editing_text && is_selected {
            self.state.text_buffer.clone()
        } else {
            value
        };

        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(format!("{:14}", label), label_style),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(display_value, value_style),
            Span::styled(cursor.to_string(), Style::default().fg(Color::White)),
        ])
    }

    fn get_field_display(&self, field: &ProjectField) -> (String, String) {
        match field {
            ProjectField::GitProvider => (
                "Provider".to_string(),
                self.state.config.git.provider.display_name().to_string(),
            ),
            ProjectField::GitLabProjectId => (
                "Project ID".to_string(),
                self.state
                    .config
                    .git
                    .gitlab
                    .project_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            ),
            ProjectField::GitLabBaseUrl => (
                "Base URL".to_string(),
                self.state.config.git.gitlab.base_url.clone(),
            ),
            ProjectField::GitHubOwner => (
                "Owner".to_string(),
                self.state
                    .config
                    .git
                    .github
                    .owner
                    .clone()
                    .unwrap_or_default(),
            ),
            ProjectField::GitHubRepo => (
                "Repo".to_string(),
                self.state
                    .config
                    .git
                    .github
                    .repo
                    .clone()
                    .unwrap_or_default(),
            ),
            ProjectField::CodebergOwner => (
                "Owner".to_string(),
                self.state
                    .config
                    .git
                    .codeberg
                    .owner
                    .clone()
                    .unwrap_or_default(),
            ),
            ProjectField::CodebergRepo => (
                "Repo".to_string(),
                self.state
                    .config
                    .git
                    .codeberg
                    .repo
                    .clone()
                    .unwrap_or_default(),
            ),
            ProjectField::CodebergBaseUrl => (
                "Base URL".to_string(),
                self.state.config.git.codeberg.base_url.clone(),
            ),
            ProjectField::BranchPrefix => (
                "Branch Prefix".to_string(),
                self.state.config.git.branch_prefix.clone(),
            ),
            ProjectField::MainBranch => (
                "Main Branch".to_string(),
                self.state.config.git.main_branch.clone(),
            ),
            ProjectField::AsanaProjectGid => (
                "Project GID".to_string(),
                self.state
                    .config
                    .project_mgmt
                    .asana
                    .project_gid
                    .clone()
                    .unwrap_or_default(),
            ),
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.state.editing_text {
            "[Enter] Save  [Esc] Cancel"
        } else if self.state.dropdown_open {
            "[↑/k][↓/j] Navigate  [Enter] Select  [Esc] Cancel"
        } else {
            "[↑/k][↓/j] Navigate  [Enter] Edit  [c] Save  [Esc] Skip"
        };

        let paragraph = Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }

    fn render_dropdown(&self, frame: &mut Frame) {
        let options: Vec<&str> = match self.fields().get(self.state.field_index) {
            Some(ProjectField::GitProvider) => GitProvider::all()
                .iter()
                .map(|g| g.display_name())
                .collect(),
            _ => return,
        };

        let area = Rect::new(
            frame.area().x + frame.area().width / 3,
            frame.area().y + 10,
            20,
            6,
        );
        frame.render_widget(Clear, area);

        let lines: Vec<Line> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let style = if i == self.state.dropdown_index {
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

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
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
