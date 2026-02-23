use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::config::{Config, ProjectMgmtProvider};
use crate::app::state::{PmSetupState, PmSetupStep};

pub struct PmSetupModal<'a> {
    state: &'a PmSetupState,
    provider: ProjectMgmtProvider,
}

impl<'a> PmSetupModal<'a> {
    pub fn new(state: &'a PmSetupState, provider: ProjectMgmtProvider) -> Self {
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

        if self.state.dropdown_open {
            self.render_dropdown(frame);
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let step_count = 3;
        let current_step = match self.state.step {
            PmSetupStep::Token => 1,
            PmSetupStep::Team => 2,
            PmSetupStep::Advanced => 3,
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
            PmSetupStep::Token => "API Token",
            PmSetupStep::Team => "Team Selection",
            PmSetupStep::Advanced => "Advanced Settings",
        }
    }

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let lines = match self.provider {
            ProjectMgmtProvider::Linear => self.render_linear_content(),
            _ => self.render_generic_content(),
        };

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_linear_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            PmSetupStep::Token => self.render_linear_token_step(),
            PmSetupStep::Team => self.render_linear_team_step(),
            PmSetupStep::Advanced => self.render_linear_advanced_step(),
        }
    }

    fn render_linear_token_step(&self) -> Vec<Line<'static>> {
        let token_exists = Config::linear_token().is_some();
        let (status_symbol, status_color) = if token_exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };

        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Linear uses a Personal API Key for authentication.",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Go to: https://linear.app/settings/api",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"New Key\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Give it a name (e.g., \"Grove\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Copy the generated token",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Add to your shell profile (~/.zshrc or ~/.bashrc):",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "    export LINEAR_TOKEN=\"lin_api_your_token_here\"",
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
                    format!("{} (LINEAR_TOKEN)", status_symbol),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(""),
        ]
    }

    fn render_linear_team_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select your Linear team:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.teams_loading {
            lines.push(Line::from(Span::styled(
                "  Loading teams...",
                Style::default().fg(Color::Yellow),
            )));
        } else if self.state.teams.is_empty() {
            if Config::linear_token().is_none() {
                lines.push(Line::from(Span::styled(
                    "  No token set. Go back to set LINEAR_TOKEN first.",
                    Style::default().fg(Color::Red),
                )));
            } else if let Some(ref err) = self.state.error {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  No teams found.",
                    Style::default().fg(Color::Yellow),
                )));
            }
        } else {
            let selected_idx = self.state.selected_team_index;
            let team_display = if let Some(team) = self.state.teams.get(selected_idx) {
                format!("{} ({})", team.1, team.2)
            } else {
                "Select team...".to_string()
            };

            let is_selected = self.state.field_index == 0;
            lines.push(self.render_field_line("Team", &team_display, is_selected, true));
            lines.push(Line::from(""));

            if self.state.advanced_expanded {
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

                let in_progress_selected = self.state.field_index == 1;
                let done_selected = self.state.field_index == 2;

                let in_progress_val = if self.state.in_progress_state.is_empty() {
                    "(auto-detect)".to_string()
                } else {
                    self.state.in_progress_state.clone()
                };
                let done_val = if self.state.done_state.is_empty() {
                    "(auto-detect)".to_string()
                } else {
                    self.state.done_state.clone()
                };

                lines.push(self.render_field_line(
                    "In Progress",
                    &in_progress_val,
                    in_progress_selected,
                    false,
                ));
                lines.push(self.render_field_line("Done", &done_val, done_selected, false));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "    Tip: Leave blank to auto-detect from workflow states",
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  ▶ Advanced (optional)",
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Team ID will be saved to .grove/project.toml",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_linear_advanced_step(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure workflow states (optional):",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  These settings override auto-detection. In most cases,",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  you can leave them blank and Grove will detect states automatically.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
        ]
    }

    fn render_generic_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                format!(
                    "  Setup for {} is not yet implemented.",
                    self.provider.display_name()
                ),
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Please configure manually in the settings.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
        ]
    }

    fn render_field_line(
        &self,
        label: &str,
        value: &str,
        is_selected: bool,
        is_dropdown: bool,
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

        let cursor = if is_selected {
            if is_dropdown {
                " ▼"
            } else {
                " ◀"
            }
        } else {
            ""
        };

        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(format!("{:12}", label), label_style),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:30}", value), value_style),
            Span::styled(cursor.to_string(), Style::default().fg(Color::White)),
        ])
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.state.dropdown_open {
            "[↑/k][↓/j] Navigate  [Enter] Select  [Esc] Cancel"
        } else {
            match self.state.step {
                PmSetupStep::Token => "[Enter] Next  [Esc] Cancel",
                PmSetupStep::Team => {
                    if self.state.editing_field() {
                        "[Enter] Save  [Esc] Cancel"
                    } else if self.state.advanced_expanded {
                        "[Enter] Finish  [←][→] Toggle Advanced  [Esc] Back"
                    } else {
                        "[Enter] Finish  [→] Expand Advanced  [Esc] Back"
                    }
                }
                PmSetupStep::Advanced => "[Enter] Finish  [Esc] Back",
            }
        };

        let paragraph = Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }

    fn render_dropdown(&self, frame: &mut Frame) {
        if self.state.teams.is_empty() {
            return;
        }

        let area = Rect::new(
            frame.area().x + frame.area().width / 3,
            frame.area().y + 12,
            30,
            (self.state.teams.len() + 2) as u16,
        );
        frame.render_widget(Clear, area);

        let lines: Vec<Line> = self
            .state
            .teams
            .iter()
            .enumerate()
            .map(|(i, (_, name, key))| {
                let style = if i == self.state.dropdown_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(format!(" {} ({}) ", name, key), style))
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}

impl PmSetupState {
    pub fn editing_field(&self) -> bool {
        self.step == PmSetupStep::Team && self.field_index > 0
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
