use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::config::Keybinds;

pub struct HelpOverlay<'a> {
    keybinds: &'a Keybinds,
}

impl<'a> HelpOverlay<'a> {
    pub fn new(keybinds: &'a Keybinds) -> Self {
        Self { keybinds }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let popup_area = centered_rect(60, 70, area);

        frame.render_widget(Clear, popup_area);

        let kb = self.keybinds;
        let help_text = vec![
            Line::from(Span::styled(
                "Flock - AI Agent Manager",
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
            Line::from(format!("  {:8} Move down", kb.nav_down.display_short())),
            Line::from(format!("  {:8} Move up", kb.nav_up.display_short())),
            Line::from(format!(
                "  {:8} Go to first agent",
                kb.nav_first.display_short()
            )),
            Line::from(format!(
                "  {:8} Go to last agent",
                kb.nav_last.display_short()
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Agent Management",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "  {:8} Create new agent",
                kb.new_agent.display_short()
            )),
            Line::from(format!(
                "  {:8} Delete selected agent",
                kb.delete_agent.display_short()
            )),
            Line::from(format!(
                "  {:8} Attach to agent's tmux session",
                kb.attach.display_short()
            )),
            Line::from(format!(
                "  {:8} Set/edit custom note",
                kb.set_note.display_short()
            )),
            Line::from(format!(
                "  {:8} Request work summary for Slack",
                kb.summary.display_short()
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Git Operations",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "  {:8} Pause & copy checkout command",
                kb.pause.display_short()
            )),
            Line::from(format!(
                "  {:8} Resume paused agent",
                kb.resume.display_short()
            )),
            Line::from(format!(
                "  {:8} Merge main into branch",
                kb.merge.display_short()
            )),
            Line::from(format!("  {:8} Push changes", kb.push.display_short())),
            Line::from(format!("  {:8} Fetch remote", kb.fetch.display_short())),
            Line::from(""),
            Line::from(Span::styled(
                "External Services",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "  {:8} Open MR/PR in browser",
                kb.open_mr.display_short()
            )),
            Line::from(format!(
                "  {:8} Open worktree in editor",
                kb.open_editor.display_short()
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Project Mgmt",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "  {:8} Assign task by URL/ID",
                kb.asana_assign.display_short()
            )),
            Line::from(format!(
                "  {:8} Open task in browser",
                kb.asana_open.display_short()
            )),
            Line::from("  t        Browse tasks from project"),
            Line::from("  T        Select task status"),
            Line::from(""),
            Line::from(Span::styled(
                "Other",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "  {:8} Refresh all status",
                kb.refresh_all.display_short()
            )),
            Line::from(format!(
                "  {:8} Toggle this help",
                kb.toggle_help.display_short()
            )),
            Line::from(format!("  {:8} Quit", kb.quit.display_short())),
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
