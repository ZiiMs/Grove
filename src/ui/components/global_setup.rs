use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::config::{AiAgent, LogLevel, WorktreeLocation};
use crate::app::{GlobalSetupState, GlobalSetupStep};

pub struct GlobalSetupWizard<'a> {
    state: &'a GlobalSetupState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    WorktreeLocation,
    AiAgent,
    LogLevel,
}

impl<'a> GlobalSetupWizard<'a> {
    pub fn new(state: &'a GlobalSetupState) -> Self {
        Self { state }
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = centered_rect(60, 65, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Welcome to Flock! ")
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
        let step_text = match self.state.step {
            GlobalSetupStep::WorktreeLocation => "Step 1 of 2: Storage Location",
            GlobalSetupStep::AgentSettings => "Step 2 of 2: Agent Settings",
        };

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

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        match self.state.step {
            GlobalSetupStep::WorktreeLocation => self.render_worktree_step(frame, area),
            GlobalSetupStep::AgentSettings => self.render_agent_step(frame, area),
        }
    }

    fn render_worktree_step(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Where should Flock store worktrees?",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        for loc in WorktreeLocation::all().iter() {
            let is_selected = *loc == self.state.worktree_location;
            let radio = if is_selected { "  ● " } else { "  ○ " };
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let desc_style = Style::default().fg(Color::DarkGray);

            lines.push(Line::from(vec![
                Span::styled(radio, name_style),
                Span::styled(loc.display_name(), name_style),
            ]));
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(loc.description(), desc_style),
            ]));
            lines.push(Line::from(""));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_agent_step(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configure your AI agent preferences:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        let ai_agent_selected = self.state.field_index == 0;
        let log_level_selected = self.state.field_index == 1;

        let ai_agent_line = self.render_field_line(
            "AI Agent",
            self.state.ai_agent.display_name(),
            ai_agent_selected,
        );
        let log_level_line = self.render_field_line(
            "Log Level",
            self.state.log_level.display_name(),
            log_level_selected,
        );

        lines.push(ai_agent_line);
        lines.push(Line::from(""));
        lines.push(log_level_line);

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
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

        let cursor = if is_selected { " ◀" } else { "" };

        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(format!("{:12}", label), label_style),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(value.to_string(), value_style),
            Span::styled(cursor.to_string(), Style::default().fg(Color::White)),
        ])
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = match self.state.step {
            GlobalSetupStep::WorktreeLocation => "[↑/↓] Select  [Enter] Next",
            GlobalSetupStep::AgentSettings => {
                if self.state.dropdown_open {
                    "[↑/↓] Navigate  [Enter] Select  [Esc] Cancel"
                } else {
                    "[↑/↓] Navigate  [Enter] Edit  [Esc] Back  [c] Complete"
                }
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
        let options: Vec<&str> = if self.state.field_index == 0 {
            AiAgent::all().iter().map(|a| a.display_name()).collect()
        } else {
            LogLevel::all().iter().map(|l| l.display_name()).collect()
        };

        let y_offset = if self.state.field_index == 0 { 9 } else { 12 };

        let area = Rect::new(
            frame.area().x + frame.area().width / 3,
            frame.area().y + y_offset,
            25,
            8,
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
