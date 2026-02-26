use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::config::Keybinds;
use crate::ui::helpers::centered_rect;

pub struct HelpOverlay<'a> {
    keybinds: &'a Keybinds,
}

impl<'a> HelpOverlay<'a> {
    pub fn new(keybinds: &'a Keybinds) -> Self {
        Self { keybinds }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let popup_area = centered_rect(60, 80, area);

        frame.render_widget(Clear, popup_area);

        let kb = self.keybinds;
        let help_text = vec![
            Line::from(Span::styled(
                "Grove - AI Agent Manager",
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
            Line::from("  Tab      Switch preview tab"),
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
                "  {:8} Copy agent name to clipboard",
                kb.yank.display_short()
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
                "  {:8} Copy cd command to worktree",
                kb.copy_path.display_short()
            )),
            Line::from(format!(
                "  {:8} Toggle auto-continue session",
                kb.toggle_continue.display_short()
            )),
            Line::from(format!(
                "  {:8} Merge main into branch",
                kb.merge.display_short()
            )),
            Line::from(format!("  {:8} Push changes", kb.push.display_short())),
            Line::from(format!("  {:8} Fetch remote", kb.fetch.display_short())),
            Line::from(""),
            Line::from(Span::styled(
                "View Controls",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "  {:8} Toggle diff view",
                kb.toggle_diff.display_short()
            )),
            Line::from(format!(
                "  {:8} Toggle logs panel",
                kb.toggle_logs.display_short()
            )),
            Line::from(format!(
                "  {:8} Open settings",
                kb.toggle_settings.display_short()
            )),
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
            Line::from(format!(
                "  {:8} Browse tasks from project",
                kb.show_tasks.display_short()
            )),
            Line::from("  T        Change linked task status"),
            Line::from(format!(
                "  {:8} Filter tasks (in task list)",
                kb.toggle_task_filter.display_short()
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Dev Server (DevServer tab)",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  C-s      Start dev server"),
            Line::from("  C-S      Restart dev server"),
            Line::from("  C        Clear logs"),
            Line::from("  O        Open in browser"),
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
            Line::from(format!(
                "  {:8} Debug status (when enabled)",
                kb.debug_status.display_short()
            )),
            Line::from(format!("  {:8} Quit", kb.quit.display_short())),
            Line::from("  Esc      Cancel/close dialogs"),
            Line::from("  C-c      Force quit"),
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
