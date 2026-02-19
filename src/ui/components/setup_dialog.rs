use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{AiAgent, GitProvider, SetupDialogState, WorktreeLocation};

pub struct SetupDialog<'a> {
    state: &'a SetupDialogState,
}

impl<'a> SetupDialog<'a> {
    pub fn new(state: &'a SetupDialogState) -> Self {
        Self { state }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(60, 70, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Flock Setup ")
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
        let text = "Configure Flock for this repository";
        let paragraph = Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(Color::Gray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }

    fn render_fields(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            self.render_category("Storage"),
            self.render_field(
                0,
                "Worktree Location",
                self.state.worktree_location.display_name(),
                false,
            ),
            self.render_category("Agent"),
            self.render_field(1, "AI Agent", self.state.ai_agent.display_name(), false),
            self.render_category("Git (Optional)"),
            self.render_field(2, "Provider", self.state.git_provider.display_name(), false),
        ];

        match self.state.git_provider {
            GitProvider::GitLab => {
                lines.push(self.render_field(
                    3,
                    "Project ID",
                    &self.state.gitlab_project_id.clone().unwrap_or_default(),
                    true,
                ));
            }
            GitProvider::GitHub => {
                lines.push(self.render_field(
                    3,
                    "Owner",
                    &self.state.github_owner.clone().unwrap_or_default(),
                    true,
                ));
                lines.push(self.render_field(
                    4,
                    "Repo",
                    &self.state.github_repo.clone().unwrap_or_default(),
                    true,
                ));
            }
            GitProvider::Bitbucket => {}
        }

        lines.push(self.render_field(5, "Branch Prefix", &self.state.branch_prefix, true));
        lines.push(self.render_field(6, "Main Branch", &self.state.main_branch, true));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_category(&self, name: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled("\n", Style::default()),
            Span::styled(
                format!("  ── {} ", name),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("─".repeat(30), Style::default().fg(Color::DarkGray)),
        ])
    }

    fn render_field(
        &self,
        index: usize,
        label: &str,
        value: &str,
        _is_text: bool,
    ) -> Line<'static> {
        let is_selected = index == self.state.field_index;

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
        } else if value.len() > 30 {
            format!("{}...", &value[..27])
        } else {
            value.to_string()
        };

        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(format!("{:16}", label), label_style),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:30}", display_value), value_style),
            Span::styled(cursor.to_string(), Style::default().fg(Color::White)),
        ])
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.state.editing_text {
            "[Enter] Save  [Esc] Cancel"
        } else if self.state.dropdown_open {
            "[↑/↓] Navigate  [Enter] Select  [Esc] Cancel"
        } else {
            "[↑/↓] Navigate  [Enter] Edit  [Esc] Skip  [q] Save & Continue"
        };

        let paragraph = Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }

    fn render_dropdown(&self, frame: &mut Frame) {
        let options: Vec<&str> = match self.state.field_index {
            0 => WorktreeLocation::all()
                .iter()
                .map(|w| w.display_name())
                .collect(),
            1 => AiAgent::all().iter().map(|a| a.display_name()).collect(),
            2 => GitProvider::all()
                .iter()
                .map(|g| g.display_name())
                .collect(),
            _ => return,
        };

        let area = get_dropdown_position(frame.area(), self.state.field_index);
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
    let modal_area = centered_rect(60, 70, frame_area);
    let base_y = modal_area.y + 5;
    let row_offset = field_index as u16;
    Rect::new(modal_area.x + 24, base_y + row_offset * 2, 22, 10)
}
