use ansi_to_tui::IntoText;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::devserver::DevServerStatus;

pub struct DevServerViewWidget {
    status: DevServerStatus,
    logs: Vec<String>,
    agent_name: String,
}

impl DevServerViewWidget {
    pub fn new(status: DevServerStatus, logs: Vec<String>, agent_name: String) -> Self {
        Self {
            status,
            logs,
            agent_name,
        }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let visible_height = area.height.saturating_sub(4) as usize;

        let mut lines = vec![self.render_status_line(), Line::from("")];

        // Parse ANSI content from logs and convert to styled lines
        let mut styled_lines: Vec<Line> = Vec::new();
        for log_line in self.logs.iter().rev().take(visible_height) {
            let text = match log_line.as_bytes().into_text() {
                Ok(text) => text,
                Err(_) => Text::raw(log_line.clone()),
            };
            styled_lines.extend(text.lines);
        }
        styled_lines.reverse();

        lines.extend(styled_lines);

        let border_color = match &self.status {
            DevServerStatus::Running { .. } => Color::Green,
            DevServerStatus::Starting | DevServerStatus::Stopping => Color::Yellow,
            DevServerStatus::Failed(_) => Color::Red,
            DevServerStatus::Stopped => Color::DarkGray,
        };

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .title(format!(" DEV SERVER: {} ", self.agent_name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_status_line(&self) -> Line<'static> {
        let (status_text, status_color) = match &self.status {
            DevServerStatus::Stopped => ("Stopped".to_string(), Color::DarkGray),
            DevServerStatus::Starting => ("Starting...".to_string(), Color::Yellow),
            DevServerStatus::Running { port, .. } => {
                if let Some(p) = port {
                    (format!("Running on port {}", p), Color::Green)
                } else {
                    ("Running".to_string(), Color::Green)
                }
            }
            DevServerStatus::Stopping => ("Stopping...".to_string(), Color::Yellow),
            DevServerStatus::Failed(msg) => (format!("Failed: {}", msg), Color::Red),
        };

        let mut spans = vec![
            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                status_text,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        if matches!(&self.status, DevServerStatus::Running { .. }) {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                "Enter: attach",
                Style::default().fg(Color::DarkGray),
            ));
        }

        Line::from(spans)
    }
}

pub struct EmptyDevServerWidget;

impl EmptyDevServerWidget {
    pub fn render(frame: &mut Frame, area: Rect) {
        let paragraph = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No dev server configured",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press Ctrl+S to start",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Configure in Settings (S) → Dev Server",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .title(" DEV SERVER ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(paragraph, area);
    }
}
