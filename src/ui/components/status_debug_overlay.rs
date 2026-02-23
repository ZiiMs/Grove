use chrono::Utc;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::agent::Agent;

pub struct StatusDebugOverlay<'a> {
    agent: &'a Agent,
}

impl<'a> StatusDebugOverlay<'a> {
    pub fn new(agent: &'a Agent) -> Self {
        Self { agent }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let popup_area = centered_rect(60, 50, area);

        frame.render_widget(Clear, popup_area);

        let status_reason = &self.agent.status_reason;

        let mut lines = vec![
            Line::from(Span::styled(
                "Status Debug",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(format!("Agent: {}", self.agent.name)),
            Line::from(format!("Branch: {}", self.agent.branch)),
            Line::from(""),
        ];

        match status_reason {
            Some(sr) => {
                lines.push(Line::from(format!(
                    "Status: {} {}",
                    sr.status.symbol(),
                    sr.status.label()
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Reason:",
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(format!("  {}", sr.reason)));

                if let Some(ref pattern) = sr.pattern {
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!("Pattern: {}", pattern)));
                }

                lines.push(Line::from(""));
                lines.push(Line::from(format!(
                    "Detected: {}",
                    format_time_since(sr.timestamp)
                )));
            }
            None => {
                lines.push(Line::from(Span::styled(
                    "No status reason recorded",
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(
                    "Status reasons are only captured when debug mode",
                ));
                lines.push(Line::from("is enabled and the status changes."));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press D or Esc to close",
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .title(" Debug ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        );

        frame.render_widget(paragraph, popup_area);
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

fn format_time_since(timestamp: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(timestamp);

    if duration.num_seconds() < 60 {
        format!("{}s ago", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else {
        format!("{}d ago", duration.num_days())
    }
}
