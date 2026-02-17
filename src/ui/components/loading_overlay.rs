use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Braille spinner frames
const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct LoadingOverlay;

impl LoadingOverlay {
    pub fn render(frame: &mut Frame, message: &str, animation_frame: usize) {
        let area = frame.area();

        // Center the loading box
        let popup_width = 40;
        let popup_height = 5;

        let popup_area = centered_rect(popup_width, popup_height, area);

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        // Get current spinner frame
        let spinner = SPINNER_FRAMES[animation_frame % SPINNER_FRAMES.len()];

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!("  {}  ", spinner),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(message, Style::default().fg(Color::White)),
            ]),
            Line::from(""),
        ];

        let paragraph = Paragraph::new(lines).alignment(Alignment::Center).block(
            Block::default()
                .title(" Loading ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .style(Style::default().bg(Color::Rgb(30, 30, 40))),
        );

        frame.render_widget(paragraph, popup_area);
    }
}

/// Helper to create a centered rect
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1])[1]
}
