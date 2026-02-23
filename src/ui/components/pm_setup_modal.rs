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
        let step_count = 4;
        let current_step = match self.state.step {
            PmSetupStep::Token => 1,
            PmSetupStep::Workspace => 2,
            PmSetupStep::Project => 3,
            PmSetupStep::Advanced => 4,
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
        match self.provider {
            ProjectMgmtProvider::Airtable => match self.state.step {
                PmSetupStep::Token => "API Token",
                PmSetupStep::Workspace => "Base",
                PmSetupStep::Project => "Table",
                PmSetupStep::Advanced => "Advanced Settings",
            },
            ProjectMgmtProvider::Notion => match self.state.step {
                PmSetupStep::Token => "API Token",
                PmSetupStep::Workspace => "Parent Page",
                PmSetupStep::Project => "Database",
                PmSetupStep::Advanced => "Advanced Settings",
            },
            _ => match self.state.step {
                PmSetupStep::Token => "API Token",
                PmSetupStep::Workspace => "Workspace",
                PmSetupStep::Project => "Project",
                PmSetupStep::Advanced => "Advanced Settings",
            },
        }
    }

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let lines = match self.provider {
            ProjectMgmtProvider::Linear => self.render_linear_content(),
            ProjectMgmtProvider::Notion => self.render_notion_content(),
            ProjectMgmtProvider::Asana => self.render_asana_content(),
            ProjectMgmtProvider::Clickup => self.render_clickup_content(),
            ProjectMgmtProvider::Airtable => self.render_airtable_content(),
        };

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_asana_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            PmSetupStep::Token => self.render_asana_token_step(),
            PmSetupStep::Workspace => self.render_asana_workspace_step(),
            PmSetupStep::Project => self.render_asana_project_step(),
            PmSetupStep::Advanced => self.render_asana_advanced_step(),
        }
    }

    fn render_asana_token_step(&self) -> Vec<Line<'static>> {
        let token_exists = Config::asana_token().is_some();
        let (status_symbol, status_color) = if token_exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };

        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Asana uses a Personal Access Token for authentication.",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Go to: https://app.asana.com/0/developer-console",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"+ Create new token\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Give it a name (e.g., \"Grove\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Copy the token (you won't see it again!)",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Add to your shell profile (~/.zshrc or ~/.bashrc):",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "    export ASANA_TOKEN=\"your_token_here\"",
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
                    format!("{} (ASANA_TOKEN)", status_symbol),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(""),
        ]
    }

    fn render_asana_workspace_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select your Asana workspace:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.teams_loading {
            lines.push(Line::from(Span::styled(
                "  Loading workspaces...",
                Style::default().fg(Color::Yellow),
            )));
        } else if self.state.teams.is_empty() {
            if Config::asana_token().is_none() {
                lines.push(Line::from(Span::styled(
                    "  No token set. Go back to set ASANA_TOKEN first.",
                    Style::default().fg(Color::Red),
                )));
            } else if let Some(ref err) = self.state.error {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  No workspaces found.",
                    Style::default().fg(Color::Yellow),
                )));
            }
        } else {
            let selected_idx = self.state.selected_team_index;
            let workspace_display = if let Some(ws) = self.state.teams.get(selected_idx) {
                ws.1.clone()
            } else {
                "Select workspace...".to_string()
            };

            let is_selected = self.state.field_index == 0;
            lines.push(self.render_field_line("Workspace", &workspace_display, is_selected, true));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Select a workspace to see its projects",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_asana_project_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select your Asana project:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.teams_loading {
            lines.push(Line::from(Span::styled(
                "  Loading projects...",
                Style::default().fg(Color::Yellow),
            )));
        } else if self.state.teams.is_empty() {
            if Config::asana_token().is_none() {
                lines.push(Line::from(Span::styled(
                    "  No token set. Go back to set ASANA_TOKEN first.",
                    Style::default().fg(Color::Red),
                )));
            } else if self.state.selected_workspace_gid.is_none() {
                lines.push(Line::from(Span::styled(
                    "  No workspace selected. Go back to select one.",
                    Style::default().fg(Color::Yellow),
                )));
            } else if let Some(ref err) = self.state.error {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  No projects found in this workspace.",
                    Style::default().fg(Color::Yellow),
                )));
            }
        } else {
            let selected_idx = self.state.selected_team_index;
            let project_display = if let Some(proj) = self.state.teams.get(selected_idx) {
                proj.1.clone()
            } else {
                "Select project...".to_string()
            };

            let is_selected = self.state.field_index == 0;
            lines.push(self.render_field_line("Project", &project_display, is_selected, true));
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
                    "    Tip: Leave blank to auto-detect from section names",
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
            "  Project GID will be saved to .grove/project.toml",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_asana_advanced_step(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure section overrides (optional):",
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
                "  you can leave them blank and Grove will detect sections",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  by name (e.g., \"In Progress\", \"Done\").",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
        ]
    }

    fn render_notion_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            PmSetupStep::Token => self.render_notion_token_step(),
            PmSetupStep::Workspace => self.render_notion_parent_page_step(),
            PmSetupStep::Project => self.render_notion_database_step(),
            PmSetupStep::Advanced => self.render_notion_advanced_step(),
        }
    }

    fn render_linear_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            PmSetupStep::Token => self.render_linear_token_step(),
            PmSetupStep::Workspace | PmSetupStep::Project => self.render_linear_team_step(),
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
                "  1. Go to: https://linear.app/settings/account/security",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"New API Key\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Give it a name (e.g., \"Grove\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Scope: Full access, or Read & Write + Issues",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  5. Teams: All teams, or select specific ones",
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

                let team_id_selected = self.state.field_index == 1;
                let in_progress_selected = self.state.field_index == 2;
                let done_selected = self.state.field_index == 3;

                let team_id_val = if self.state.manual_team_id.is_empty() {
                    "(from selection)".to_string()
                } else {
                    self.state.manual_team_id.clone()
                };
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
                    "Team ID",
                    &team_id_val,
                    team_id_selected,
                    false,
                ));
                lines.push(self.render_field_line(
                    "In Progress",
                    &in_progress_val,
                    in_progress_selected,
                    false,
                ));
                lines.push(self.render_field_line("Done", &done_val, done_selected, false));
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

    fn render_notion_token_step(&self) -> Vec<Line<'static>> {
        let token_exists = Config::notion_token().is_some();
        let (status_symbol, status_color) = if token_exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };

        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Notion uses an Integration Secret for authentication.",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Go to: https://www.notion.so/profile/integrations/internal",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"+ New integration\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Give it a name (e.g., \"Grove\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Select the workspace and capabilities",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  5. Copy the \"Internal Integration Secret\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Important: Share your database with the integration!",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(Span::styled(
                "  In Notion, open your database → ... → Connections → Add",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Add to your shell profile (~/.zshrc or ~/.bashrc):",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "    export NOTION_TOKEN=\"secret_your_token_here\"",
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
                    format!("{} (NOTION_TOKEN)", status_symbol),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(""),
        ]
    }

    fn render_notion_parent_page_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select the parent page containing your database:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.teams_loading {
            lines.push(Line::from(Span::styled(
                "  Loading pages...",
                Style::default().fg(Color::Yellow),
            )));
        } else if self.state.teams.is_empty() {
            if Config::notion_token().is_none() {
                lines.push(Line::from(Span::styled(
                    "  No token set. Go back to set NOTION_TOKEN first.",
                    Style::default().fg(Color::Red),
                )));
            } else if let Some(ref err) = self.state.error {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  No parent pages found with databases.",
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(Span::styled(
                    "  Make sure you've shared a database with your integration.",
                    Style::default().fg(Color::Yellow),
                )));
            }
        } else {
            let selected_idx = self.state.selected_team_index;
            let page_display = if let Some(page) = self.state.teams.get(selected_idx) {
                page.1.clone()
            } else {
                "Select page...".to_string()
            };

            let is_selected = self.state.field_index == 0;
            lines.push(self.render_field_line("Parent Page", &page_display, is_selected, true));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Select a parent page to see its databases",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_notion_database_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select your Notion database:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.teams_loading {
            lines.push(Line::from(Span::styled(
                "  Loading databases...",
                Style::default().fg(Color::Yellow),
            )));
        } else if self.state.teams.is_empty() {
            if Config::notion_token().is_none() {
                lines.push(Line::from(Span::styled(
                    "  No token set. Go back to set NOTION_TOKEN first.",
                    Style::default().fg(Color::Red),
                )));
            } else if self.state.selected_workspace_gid.is_none() {
                lines.push(Line::from(Span::styled(
                    "  No parent page selected. Go back to select one.",
                    Style::default().fg(Color::Yellow),
                )));
            } else if let Some(ref err) = self.state.error {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  No databases found under this page.",
                    Style::default().fg(Color::Yellow),
                )));
            }
        } else {
            let selected_idx = self.state.selected_team_index;
            let db_display = if let Some(db) = self.state.teams.get(selected_idx) {
                db.1.clone()
            } else {
                "Select database...".to_string()
            };

            let is_selected = self.state.field_index == 0;
            lines.push(self.render_field_line("Database", &db_display, is_selected, true));
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

                let db_id_selected = self.state.field_index == 1;
                let in_progress_selected = self.state.field_index == 2;
                let done_selected = self.state.field_index == 3;

                let db_id_val = if self.state.manual_team_id.is_empty() {
                    "(from selection)".to_string()
                } else {
                    self.state.manual_team_id.clone()
                };
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
                    "Database ID",
                    &db_id_val,
                    db_id_selected,
                    false,
                ));
                lines.push(self.render_field_line(
                    "In Progress",
                    &in_progress_val,
                    in_progress_selected,
                    false,
                ));
                lines.push(self.render_field_line("Done", &done_val, done_selected, false));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "    Tip: Leave blank to auto-detect from status options",
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
            "  Database ID will be saved to .grove/project.toml",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_notion_advanced_step(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure status options (optional):",
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
                "  you can leave them blank and Grove will detect statuses automatically.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
        ]
    }

    fn render_clickup_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            PmSetupStep::Token => self.render_clickup_token_step(),
            PmSetupStep::Workspace => self.render_clickup_team_step(),
            PmSetupStep::Project => self.render_clickup_list_step(),
            PmSetupStep::Advanced => self.render_clickup_advanced_step(),
        }
    }

    fn render_clickup_token_step(&self) -> Vec<Line<'static>> {
        let token_exists = Config::clickup_token().is_some();
        let (status_symbol, status_color) = if token_exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };

        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  ClickUp uses an API Token for authentication.",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Go to: https://app.clickup.com/settings/apps",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"Create App\" or use an existing personal token",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Give it a name (e.g., \"Grove\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Copy the API Token (starts with \"pk_\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Add to your shell profile (~/.zshrc or ~/.bashrc):",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "    export CLICKUP_TOKEN=\"pk_your_token_here\"",
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
                    format!("{} (CLICKUP_TOKEN)", status_symbol),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(""),
        ]
    }

    fn render_clickup_team_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select your ClickUp team (workspace):",
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
            if Config::clickup_token().is_none() {
                lines.push(Line::from(Span::styled(
                    "  No token set. Go back to set CLICKUP_TOKEN first.",
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
                team.1.clone()
            } else {
                "Select team...".to_string()
            };

            let is_selected = self.state.field_index == 0;
            lines.push(self.render_field_line("Team", &team_display, is_selected, true));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Select a team to see its lists",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_clickup_list_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select your ClickUp list:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.teams_loading {
            lines.push(Line::from(Span::styled(
                "  Loading lists...",
                Style::default().fg(Color::Yellow),
            )));
        } else if self.state.teams.is_empty() {
            if Config::clickup_token().is_none() {
                lines.push(Line::from(Span::styled(
                    "  No token set. Go back to set CLICKUP_TOKEN first.",
                    Style::default().fg(Color::Red),
                )));
            } else if self.state.selected_workspace_gid.is_none() {
                lines.push(Line::from(Span::styled(
                    "  No team selected. Go back to select one.",
                    Style::default().fg(Color::Yellow),
                )));
            } else if let Some(ref err) = self.state.error {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  No lists found in this team.",
                    Style::default().fg(Color::Yellow),
                )));
            }
        } else {
            let selected_idx = self.state.selected_team_index;
            let list_display = if let Some(list) = self.state.teams.get(selected_idx) {
                list.2.clone()
            } else {
                "Select list...".to_string()
            };

            let is_selected = self.state.field_index == 0;
            lines.push(self.render_field_line("List", &list_display, is_selected, true));
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
                    "    Tip: Leave blank to auto-detect from list statuses",
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
            "  List ID will be saved to .grove/project.toml",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_clickup_advanced_step(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure status overrides (optional):",
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
                "  you can leave them blank and Grove will detect statuses",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  by name (e.g., \"In Progress\", \"Complete\").",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
        ]
    }

    fn render_airtable_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            PmSetupStep::Token => self.render_airtable_token_step(),
            PmSetupStep::Workspace => self.render_airtable_base_step(),
            PmSetupStep::Project => self.render_airtable_table_step(),
            PmSetupStep::Advanced => self.render_airtable_advanced_step(),
        }
    }

    fn render_airtable_token_step(&self) -> Vec<Line<'static>> {
        let token_exists = Config::airtable_token().is_some();
        let (status_symbol, status_color) = if token_exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };

        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Airtable uses a Personal Access Token for authentication.",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Go to: https://airtable.com/create/tokens",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"Create new token\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Give it a name (e.g., \"Grove\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Add access: All current and future bases in workspace",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "     Or grant read access to specific bases",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  5. Copy the token (starts with \"pat\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Add to your shell profile (~/.zshrc or ~/.bashrc):",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "    export AIRTABLE_TOKEN=\"pat_your_token_here\"",
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
                    format!("{} (AIRTABLE_TOKEN)", status_symbol),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(""),
        ]
    }

    fn render_airtable_base_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select your Airtable base:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.teams_loading {
            lines.push(Line::from(Span::styled(
                "  Loading bases...",
                Style::default().fg(Color::Yellow),
            )));
        } else if self.state.teams.is_empty() {
            if Config::airtable_token().is_none() {
                lines.push(Line::from(Span::styled(
                    "  No token set. Go back to set AIRTABLE_TOKEN first.",
                    Style::default().fg(Color::Red),
                )));
            } else if let Some(ref err) = self.state.error {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  No bases found.",
                    Style::default().fg(Color::Yellow),
                )));
            }
        } else {
            let selected_idx = self.state.selected_team_index;
            let base_display = if let Some(base) = self.state.teams.get(selected_idx) {
                if base.2.is_empty() {
                    base.1.clone()
                } else {
                    format!("{} ({})", base.1, base.2)
                }
            } else {
                "Select base...".to_string()
            };

            let is_selected = self.state.field_index == 0;
            lines.push(self.render_field_line("Base", &base_display, is_selected, true));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Select a base to see its tables",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_airtable_table_step(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select your Airtable table:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if self.state.teams_loading {
            lines.push(Line::from(Span::styled(
                "  Loading tables...",
                Style::default().fg(Color::Yellow),
            )));
        } else if self.state.teams.is_empty() {
            if Config::airtable_token().is_none() {
                lines.push(Line::from(Span::styled(
                    "  No token set. Go back to set AIRTABLE_TOKEN first.",
                    Style::default().fg(Color::Red),
                )));
            } else if self.state.selected_workspace_gid.is_none() {
                lines.push(Line::from(Span::styled(
                    "  No base selected. Go back to select one.",
                    Style::default().fg(Color::Yellow),
                )));
            } else if let Some(ref err) = self.state.error {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  No tables found in this base.",
                    Style::default().fg(Color::Yellow),
                )));
            }
        } else {
            let selected_idx = self.state.selected_team_index;
            let table_display = if let Some(table) = self.state.teams.get(selected_idx) {
                table.1.clone()
            } else {
                "Select table...".to_string()
            };

            let is_selected = self.state.field_index == 0;
            lines.push(self.render_field_line("Table", &table_display, is_selected, true));
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
                    "    Tip: Leave blank to auto-detect from status options",
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
            "  Table name will be saved to .grove/project.toml",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn render_airtable_advanced_step(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure status options (optional):",
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
                "  you can leave them blank and Grove will detect statuses",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  by name (e.g., \"In Progress\", \"Done\").",
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
                PmSetupStep::Workspace => "[Enter] Open Dropdown  [c] Continue  [Esc] Back",
                PmSetupStep::Project => {
                    if self.state.advanced_expanded {
                        "[c] Finish  [←][→] Toggle Advanced  [Esc] Back"
                    } else {
                        "[Enter] Open Dropdown  [c] Finish  [→] Expand Advanced  [Esc] Back"
                    }
                }
                PmSetupStep::Advanced => "[c] Finish  [Esc] Back",
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
            frame.area().x + frame.area().width / 4,
            frame.area().y + 12,
            50,
            (self.state.teams.len() + 2) as u16,
        );
        frame.render_widget(Clear, area);

        let is_notion = matches!(self.provider, ProjectMgmtProvider::Notion);

        let lines: Vec<Line> = self
            .state
            .teams
            .iter()
            .enumerate()
            .map(|(i, (_, name, parent))| {
                let style = if i == self.state.dropdown_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let display = if is_notion && !parent.is_empty() {
                    format!(" {} > {} ", parent, name)
                } else if !parent.is_empty() {
                    format!(" {} ({}) ", name, parent)
                } else {
                    format!(" {} ", name)
                };
                Line::from(Span::styled(display, style))
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
        matches!(self.step, PmSetupStep::Project) && self.field_index > 0
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
