use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame, area: Rect) {
        // Create a centered popup
        let popup_area = centered_rect(60, 70, area);

        // Clear the background
        frame.render_widget(Clear, popup_area);

        let help_text = vec![
            Line::from(Span::styled(
                "Flock - Claude Agent Manager",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Navigation",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  j/↓      Move down"),
            Line::from("  k/↑      Move up"),
            Line::from("  g        Go to first agent"),
            Line::from("  G        Go to last agent"),
            Line::from(""),
            Line::from(Span::styled(
                "Agent Management",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  n        Create new agent"),
            Line::from("  d        Delete selected agent"),
            Line::from("  Enter    Attach to agent's tmux session"),
            Line::from("  N        Set/edit custom note"),
            Line::from("  s        Request work summary for Slack"),
            Line::from(""),
            Line::from(Span::styled(
                "Git Operations",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  c        Pause & copy checkout command"),
            Line::from("  r        Resume paused agent"),
            Line::from("  m        Send merge main request to Claude"),
            Line::from("  p        Send /push to Claude"),
            Line::from("  f        Fetch remote"),
            Line::from(""),
            Line::from(Span::styled(
                "GitLab",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  o        Open MR in browser"),
            Line::from(""),
            Line::from(Span::styled(
                "Asana",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  a        Assign Asana task"),
            Line::from("  A        Open Asana task in browser"),
            Line::from(""),
            Line::from(Span::styled(
                "Other",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  R        Refresh all status"),
            Line::from("  ?        Toggle this help"),
            Line::from("  q        Quit"),
            Line::from("  Esc      Cancel/close"),
            Line::from(""),
            Line::from(Span::styled(
                "Press any key to close",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let paragraph = Paragraph::new(help_text).block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );

        frame.render_widget(paragraph, popup_area);
    }
}

/// Create a centered rectangle with percentage width and height.
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
