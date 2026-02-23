use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::config::{GitProvider, ProjectMgmtProvider};
use crate::app::ProjectSetupState;

pub struct ProjectSetupWizard<'a> {
    state: &'a ProjectSetupState,
    repo_name: &'a str,
}

impl<'a> ProjectSetupWizard<'a> {
    pub fn new(state: &'a ProjectSetupState, repo_name: &'a str) -> Self {
        Self { state, repo_name }
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
                Constraint::Min(14),
                Constraint::Length(3),
            ])
            .split(inner);

        self.render_header(frame, chunks[0]);
        self.render_rows(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);

        if self.state.git_provider_dropdown_open {
            self.render_git_dropdown(frame);
        }
        if self.state.pm_provider_dropdown_open {
            self.render_pm_dropdown(frame);
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

    fn render_rows(&self, frame: &mut Frame, area: Rect) {
        let git_configured = self.is_git_configured();
        let pm_configured = self.is_pm_configured();

        let lines = vec![
            Line::from(""),
            self.render_git_dropdown_row(),
            self.render_git_setup_row(git_configured),
            Line::from(""),
            self.render_pm_dropdown_row(),
            self.render_pm_setup_row(pm_configured),
            Line::from(""),
            self.render_buttons(),
        ];

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_git_dropdown_row(&self) -> Line<'static> {
        let is_selected = self.state.selected_index == 0;
        let provider_name = self.state.config.git.provider.display_name();

        let label_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let dropdown_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        Line::from(vec![
            Span::styled("  Git Provider: ", label_style),
            Span::styled(format!("[{} ▼]", provider_name), dropdown_style),
        ])
    }

    fn render_git_setup_row(&self, configured: bool) -> Line<'static> {
        let is_selected = self.state.selected_index == 1;

        let button_style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };

        let status_style = if configured {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Red)
        };

        let status_text = if configured {
            "✓ Configured"
        } else {
            "✗ Not configured"
        };

        Line::from(vec![
            Span::styled("           ", Style::default()),
            Span::styled("[ Setup ]", button_style),
            Span::styled("  ", Style::default()),
            Span::styled(status_text, status_style),
        ])
    }

    fn render_pm_dropdown_row(&self) -> Line<'static> {
        let is_selected = self.state.selected_index == 2;
        let provider_name = self.state.config.project_mgmt.provider.display_name();

        let label_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let dropdown_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        Line::from(vec![
            Span::styled("  Project Mgmt: ", label_style),
            Span::styled(format!("[{} ▼]", provider_name), dropdown_style),
        ])
    }

    fn render_pm_setup_row(&self, configured: bool) -> Line<'static> {
        let is_selected = self.state.selected_index == 3;

        let button_style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };

        let status_style = if configured {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Red)
        };

        let status_text = if configured {
            "✓ Configured"
        } else {
            "✗ Not configured"
        };

        Line::from(vec![
            Span::styled("           ", Style::default()),
            Span::styled("[ Setup ]", button_style),
            Span::styled("  ", Style::default()),
            Span::styled(status_text, status_style),
        ])
    }

    fn render_buttons(&self) -> Line<'static> {
        let save_selected = self.state.selected_index == 4;
        let close_selected = self.state.selected_index == 5;

        let save_style = if save_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };

        let close_style = if close_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };

        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("[ Save ]", save_style),
            Span::styled("      ", Style::default()),
            Span::styled("[ Close ]", close_style),
        ])
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.state.git_provider_dropdown_open || self.state.pm_provider_dropdown_open
        {
            "[↑/k][↓/j] Navigate  [Enter] Select  [Esc] Cancel"
        } else {
            "[↑/k][↓/j] Navigate  [Enter] Select/Dropdown  [c] Save  [Esc] Close"
        };

        let paragraph = Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }

    fn render_git_dropdown(&self, frame: &mut Frame) {
        let options: Vec<&str> = GitProvider::all()
            .iter()
            .map(|g| g.display_name())
            .collect();

        let area = Rect::new(
            frame.area().x + 15,
            frame.area().y + 6,
            15,
            (options.len() + 2) as u16,
        );
        frame.render_widget(Clear, area);

        let lines: Vec<Line> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let style = if i == self.state.git_provider_dropdown_index {
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

    fn render_pm_dropdown(&self, frame: &mut Frame) {
        let options: Vec<&str> = ProjectMgmtProvider::all()
            .iter()
            .map(|p| p.display_name())
            .collect();

        let area = Rect::new(
            frame.area().x + 15,
            frame.area().y + 11,
            15,
            (options.len() + 2) as u16,
        );
        frame.render_widget(Clear, area);

        let lines: Vec<Line> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let style = if i == self.state.pm_provider_dropdown_index {
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

    fn is_git_configured(&self) -> bool {
        match self.state.config.git.provider {
            GitProvider::GitLab => self.state.config.git.gitlab.project_id.is_some(),
            GitProvider::GitHub => {
                self.state.config.git.github.owner.is_some()
                    && self.state.config.git.github.repo.is_some()
            }
            GitProvider::Codeberg => {
                self.state.config.git.codeberg.owner.is_some()
                    && self.state.config.git.codeberg.repo.is_some()
            }
        }
    }

    fn is_pm_configured(&self) -> bool {
        match self.state.config.project_mgmt.provider {
            ProjectMgmtProvider::Asana => {
                self.state.config.project_mgmt.asana.project_gid.is_some()
            }
            ProjectMgmtProvider::Notion => {
                self.state.config.project_mgmt.notion.database_id.is_some()
            }
            ProjectMgmtProvider::Clickup => {
                self.state.config.project_mgmt.clickup.list_id.is_some()
            }
            ProjectMgmtProvider::Airtable => {
                self.state.config.project_mgmt.airtable.base_id.is_some()
            }
            ProjectMgmtProvider::Linear => self.state.config.project_mgmt.linear.team_id.is_some(),
        }
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
