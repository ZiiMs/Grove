use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::DevServerWarning;
use crate::ui::helpers::centered_rect;

pub struct DevServerWarningModal<'a> {
    warning: &'a DevServerWarning,
}

impl<'a> DevServerWarningModal<'a> {
    pub fn new(warning: &'a DevServerWarning) -> Self {
        Self { warning }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(60, 50, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" WARNING ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Another dev server is already running:",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
        ];

        for (name, port) in &self.warning.running_servers {
            let port_str = port
                .map(|p| p.to_string())
                .unwrap_or_else(|| "?".to_string());
            lines.push(Line::from(Span::styled(
                format!("  - {} (port {})", name, port_str),
                Style::default().fg(Color::Yellow),
            )));
        }

        lines.extend(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Starting a new server may cause:",
                Style::default().fg(Color::White),
            )),
            Line::from(Span::styled(
                "  - Port conflicts",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  - Resource contention",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  - Unexpected behavior",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Start anyway?",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[y] Yes    [n/Esc] No",
                Style::default().fg(Color::Cyan),
            )),
        ]);

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }
}
