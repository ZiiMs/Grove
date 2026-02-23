use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::config::{Config, GitProvider};
use crate::app::state::{GitSetupState, GitSetupStep};

pub struct GitSetupModal<'a> {
    state: &'a GitSetupState,
    provider: GitProvider,
}

impl<'a> GitSetupModal<'a> {
    pub fn new(state: &'a GitSetupState, provider: GitProvider) -> Self {
        Self { state, provider }
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = centered_rect(70, 75, frame.area());
        frame.render_widget(Clear, area);

        let title = format!(" {} Setup ", self.provider.display_name());
        let block = Block::default()
            .title(title)
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
        self.render_content(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let step_count = 2;
        let current_step = match self.state.step {
            GitSetupStep::Token => 1,
            GitSetupStep::Repository => 2,
            GitSetupStep::Advanced => 2,
        };

        let step_text = format!(
            "Step {} of {}: {}",
            current_step,
            step_count,
            self.step_name()
        );

        let paragraph = Paragraph::new(Line::from(vec![Span::styled(
            step_text,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);

        let divider_line = Rect::new(area.x, area.y + 2, area.width, 1);
        let divider = Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(divider, divider_line);
    }

    fn step_name(&self) -> &'static str {
        match self.state.step {
            GitSetupStep::Token => "API Token",
            GitSetupStep::Repository => "Repository",
            GitSetupStep::Advanced => "Advanced Settings",
        }
    }

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let lines = match self.provider {
            GitProvider::GitLab => self.render_gitlab_content(),
            GitProvider::GitHub => self.render_github_content(),
            GitProvider::Codeberg => self.render_codeberg_content(),
        };

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_gitlab_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            GitSetupStep::Token => self.render_gitlab_token_step(),
            GitSetupStep::Repository => self.render_gitlab_repository_step(),
            GitSetupStep::Advanced => self.render_gitlab_advanced_step(),
        }
    }

    fn render_gitlab_token_step(&self) -> Vec<Line<'static>> {
        let token_exists = Config::gitlab_token().is_some();
        let (status_symbol, status_color) = if token_exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };

        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  GitLab uses a Personal Access Token for authentication.",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Go to: https://gitlab.com/-/profile/personal_access_tokens",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"Add new token\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Name it (e.g., \"Grove\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Expiration: Choose a reasonable timeframe",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  5. Select scopes: api, read_repository",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  6. Copy the token (starts with \"glpat-\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Add to your shell profile (~/.zshrc or ~/.bashrc):",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "    export GITLAB_TOKEN=\"glpat-your_token_here\"",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Then restart Grove or run: source ~/.zshrc",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Token Status: ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("{} (GITLAB_TOKEN)", status_symbol),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(""),
        ]
    }

    fn render_gitlab_repository_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure your GitLab project:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.detected_from_remote {
            lines.push(Line::from(Span::styled(
                format!("  ↳ Detected: {}/{}", self.state.owner, self.state.repo),
                Style::default().fg(Color::Green),
            )));
            lines.push(Line::from(""));
        }

        if self.state.loading {
            lines.push(Line::from(Span::styled(
                "  Fetching project ID from GitLab...",
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(""));
            return lines;
        }

        if let Some(ref name) = self.state.project_name {
            lines.push(Line::from(vec![
                Span::styled("  Project: ", Style::default().fg(Color::Green)),
                Span::styled(name.clone(), Style::default().fg(Color::White)),
            ]));
            lines.push(Line::from(""));
        }

        // Field 0: Owner
        let owner_display = if self.state.editing_text && self.state.field_index == 0 {
            if self.state.text_buffer.is_empty() {
                "Enter owner...█".to_string()
            } else {
                format!("{}█", self.state.text_buffer)
            }
        } else if self.state.owner.is_empty() {
            "Enter owner...".to_string()
        } else {
            self.state.owner.clone()
        };
        let owner_selected = self.state.field_index == 0;
        lines.push(self.render_field_line("Owner", &owner_display, owner_selected));

        // Field 1: Repo
        let repo_display = if self.state.editing_text && self.state.field_index == 1 {
            if self.state.text_buffer.is_empty() {
                "Enter repo...█".to_string()
            } else {
                format!("{}█", self.state.text_buffer)
            }
        } else if self.state.repo.is_empty() {
            "Enter repo...".to_string()
        } else {
            self.state.repo.clone()
        };
        let repo_selected = self.state.field_index == 1;
        lines.push(self.render_field_line("Repo", &repo_display, repo_selected));

        // Field 2: Project ID (always visible)
        let project_id_display = if self.state.editing_text
            && self.state.field_index == 2
            && self.state.project_id.is_empty()
        {
            "(auto or enter manually)█".to_string()
        } else if self.state.editing_text && self.state.field_index == 2 {
            format!("{}█", self.state.text_buffer)
        } else if self.state.project_id.is_empty() {
            "(auto or enter manually)".to_string()
        } else {
            self.state.project_id.clone()
        };
        let project_id_selected = self.state.field_index == 2;
        lines.push(self.render_field_line("Project ID", &project_id_display, project_id_selected));

        // Auto-fetch hint
        if self.state.project_id.is_empty()
            && Config::gitlab_token().is_some()
            && !self.state.owner.is_empty()
            && !self.state.repo.is_empty()
        {
            lines.push(Line::from(Span::styled(
                "  [f] Auto-fetch Project ID from GitLab",
                Style::default().fg(Color::Cyan),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Find Project ID: GitLab Project → Settings → General",
            Style::default().fg(Color::DarkGray),
        )));

        if self.state.advanced_expanded {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  ▼ Advanced (optional)",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "    ─────────────────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));

            // Field 3: Base URL (when advanced expanded)
            let base_url_display = if self.state.editing_text && self.state.field_index == 3 {
                if self.state.text_buffer.is_empty() {
                    "(default: gitlab.com)█".to_string()
                } else {
                    format!("{}█", self.state.text_buffer)
                }
            } else if self.state.base_url.is_empty() {
                "(default: gitlab.com)".to_string()
            } else {
                self.state.base_url.clone()
            };

            let base_url_selected = self.state.field_index == 3;
            lines.push(self.render_field_line("Base URL", &base_url_display, base_url_selected));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "    For self-hosted GitLab, enter your instance URL",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  ▶ Advanced (optional)",
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines
    }

    fn render_gitlab_advanced_step(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure advanced settings (optional):",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  These settings are usually auto-detected or optional.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  You can skip this step if everything is configured.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
        ]
    }

    fn render_github_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            GitSetupStep::Token => self.render_github_token_step(),
            GitSetupStep::Repository => self.render_github_repository_step(),
            GitSetupStep::Advanced => self.render_github_advanced_step(),
        }
    }

    fn render_github_token_step(&self) -> Vec<Line<'static>> {
        let token_exists = Config::github_token().is_some();
        let (status_symbol, status_color) = if token_exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };

        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  GitHub uses a Personal Access Token for authentication.",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Go to: https://github.com/settings/tokens",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"Generate new token (classic)\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Note: \"Grove\" (or any name)",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Select scopes: repo, read:org",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  5. Click \"Generate token\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  6. Copy the token (starts with \"ghp_\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Add to your shell profile (~/.zshrc or ~/.bashrc):",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "    export GITHUB_TOKEN=\"ghp_your_token_here\"",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Then restart Grove or run: source ~/.zshrc",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Token Status: ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("{} (GITHUB_TOKEN)", status_symbol),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(""),
        ]
    }

    fn render_github_repository_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure your GitHub repository:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.detected_from_remote {
            lines.push(Line::from(Span::styled(
                "  ↳ Detected from git remote",
                Style::default().fg(Color::Green),
            )));
            lines.push(Line::from(""));
        }

        let owner_display = if self.state.editing_text && self.state.field_index == 0 {
            if self.state.text_buffer.is_empty() {
                "Enter owner...█".to_string()
            } else {
                format!("{}█", self.state.text_buffer)
            }
        } else if self.state.owner.is_empty() {
            "Enter owner...".to_string()
        } else {
            self.state.owner.clone()
        };

        let repo_display = if self.state.editing_text && self.state.field_index == 1 {
            if self.state.text_buffer.is_empty() {
                "Enter repo...█".to_string()
            } else {
                format!("{}█", self.state.text_buffer)
            }
        } else if self.state.repo.is_empty() {
            "Enter repo...".to_string()
        } else {
            self.state.repo.clone()
        };

        let owner_selected = self.state.field_index == 0;
        let repo_selected = self.state.field_index == 1;

        lines.push(self.render_field_line("Owner", &owner_display, owner_selected));
        lines.push(self.render_field_line("Repo", &repo_display, repo_selected));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Owner is your username or organization name",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_github_advanced_step(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure advanced settings (optional):",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  These settings are usually auto-detected or optional.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  You can skip this step if everything is configured.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Note: For GitHub Enterprise, base URL can be set",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  directly in Settings → Git → Base URL.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
        ]
    }

    fn render_codeberg_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            GitSetupStep::Token => self.render_codeberg_token_step(),
            GitSetupStep::Repository => self.render_codeberg_repository_step(),
            GitSetupStep::Advanced => self.render_codeberg_advanced_step(),
        }
    }

    fn render_codeberg_token_step(&self) -> Vec<Line<'static>> {
        let token_exists = Config::codeberg_token().is_some();
        let (status_symbol, status_color) = if token_exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };

        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Codeberg uses an Access Token for authentication.",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Go to: https://codeberg.org/user/settings/applications",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"Generate New Token\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Name it (e.g., \"Grove\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Select scopes: repo, read:organization",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  5. Click \"Generate Token\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  6. Copy the token",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Add to your shell profile (~/.zshrc or ~/.bashrc):",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "    export CODEBERG_TOKEN=\"your_token_here\"",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Then restart Grove or run: source ~/.zshrc",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Token Status: ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("{} (CODEBERG_TOKEN)", status_symbol),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(""),
        ]
    }

    fn render_codeberg_repository_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure your Codeberg repository:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.detected_from_remote {
            lines.push(Line::from(Span::styled(
                "  ↳ Detected from git remote",
                Style::default().fg(Color::Green),
            )));
            lines.push(Line::from(""));
        }

        let owner_display = if self.state.editing_text && self.state.field_index == 0 {
            if self.state.text_buffer.is_empty() {
                "Enter owner...█".to_string()
            } else {
                format!("{}█", self.state.text_buffer)
            }
        } else if self.state.owner.is_empty() {
            "Enter owner...".to_string()
        } else {
            self.state.owner.clone()
        };

        let repo_display = if self.state.editing_text && self.state.field_index == 1 {
            if self.state.text_buffer.is_empty() {
                "Enter repo...█".to_string()
            } else {
                format!("{}█", self.state.text_buffer)
            }
        } else if self.state.repo.is_empty() {
            "Enter repo...".to_string()
        } else {
            self.state.repo.clone()
        };

        let owner_selected = self.state.field_index == 0;
        let repo_selected = self.state.field_index == 1;

        lines.push(self.render_field_line("Owner", &owner_display, owner_selected));
        lines.push(self.render_field_line("Repo", &repo_display, repo_selected));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Owner is your username or organization name",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_codeberg_advanced_step(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure advanced settings (optional):",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  These settings are usually auto-detected or optional.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  You can skip this step if everything is configured.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  For Woodpecker CI integration, ensure WOODPECKER_TOKEN",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  is set if using Woodpecker instead of Forgejo Actions.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
        ]
    }

    fn render_field_line(&self, label: &str, value: &str, is_selected: bool) -> Line<'static> {
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

        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(format!("{:14}", label), label_style),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(value.to_string(), value_style),
        ])
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.state.editing_text {
            "[Enter] Save  [Esc] Cancel"
        } else {
            match self.state.step {
                GitSetupStep::Token => "[Enter] Continue  [Esc] Cancel",
                GitSetupStep::Repository => {
                    if self.state.advanced_expanded {
                        "[↑/k][↓/j] Navigate  [Enter] Edit  [a] Collapse Advanced  [→/l] Finish  [Esc] Cancel"
                    } else {
                        "[↑/k][↓/j] Navigate  [Enter] Edit  [a] Expand Advanced  [→/l] Finish  [Esc] Cancel"
                    }
                }
                GitSetupStep::Advanced => "[←/h] Back  [Enter] Finish  [Esc] Cancel",
            }
        };

        let mut spans = vec![Span::styled(hint, Style::default().fg(Color::DarkGray))];

        if let Some(ref error) = self.state.error {
            spans.push(Span::styled(
                format!("   ⚠ {}", error),
                Style::default().fg(Color::Red),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
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
