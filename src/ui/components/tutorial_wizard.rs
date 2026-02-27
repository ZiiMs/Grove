use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::{TutorialState, TutorialStep};
use crate::ui::helpers::centered_rect;

pub struct TutorialWizard<'a> {
    state: &'a TutorialState,
}

impl<'a> TutorialWizard<'a> {
    pub fn new(state: &'a TutorialState) -> Self {
        Self { state }
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = centered_rect(65, 75, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Grove Tutorial ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(inner);

        self.render_header(frame, chunks[0]);
        self.render_content(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let step = self.state.step;
        let step_text = format!(
            "Step {} of {}: {}",
            step.step_number(),
            TutorialStep::total_steps(),
            step.title()
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

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let lines = self.get_step_content();
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn get_step_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            TutorialStep::Welcome => self.welcome_content(),
            TutorialStep::UiLayout => self.ui_layout_content(),
            TutorialStep::AgentColumns => self.agent_columns_content(),
            TutorialStep::PreviewTabs => self.preview_tabs_content(),
            TutorialStep::Navigation => self.navigation_content(),
            TutorialStep::AgentManagement => self.agent_management_content(),
            TutorialStep::TaskManagement => self.task_management_content(),
            TutorialStep::GitOperations => self.git_operations_content(),
            TutorialStep::Automation => self.automation_content(),
            TutorialStep::DevServer => self.dev_server_content(),
            TutorialStep::Workflows => self.workflows_content(),
            TutorialStep::GettingHelp => self.getting_help_content(),
        }
    }

    fn welcome_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Welcome to Grove!",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Grove helps you manage multiple AI coding agents working on different",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  tasks in parallel. Each agent works in its own git worktree, keeping",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  changes isolated until you're ready to merge.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Let's take a quick tour of the interface!",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press Enter or → to continue, Esc or q to skip",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    }

    fn ui_layout_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  The Grove Interface",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  ┌─ AGENTS ───────────────────────────────────────┐",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  │ S C Name        Status   Tasks   MR   Server   │",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  │ ▶ ↻ GRE-47...   ○ Idle   2/2     None  ○ Stop  │",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  └────────────────────────────────────────────────┘",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled("Preview", Style::default().fg(Color::Cyan)),
                Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
                Span::styled("Git Diff", Style::default().fg(Color::Gray)),
                Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
                Span::styled("Dev Server", Style::default().fg(Color::Gray)),
                Span::styled("   ← Tab to switch", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(Span::styled(
                "  ┌─ PREVIEW ──────────────────────────────────────┐",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  │ Agent output appears here (scroll: PgUp/PgDn) │",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  └────────────────────────────────────────────────┘",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  ┌─ CPU/MEM ─────────────┬─ LOGS ────────────────┐",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  │ System metrics graphs │ Application messages  │",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  └───────────────────────┴───────────────────────┘",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "   [n]new [↵]attach [d]delete [?]help [q]quit",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    }

    fn agent_columns_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Agent List Columns",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            key_value_line("S", "Selection indicator (▶)"),
            key_value_line("S", "Summary requested (✓)"),
            key_value_line("C", "Auto-continue enabled (↻)"),
            key_value_line("Name", "Branch name for this agent"),
            key_value_line("Status", "Running/Idle/Completed/Error"),
            key_value_line("Active", "Time since last activity"),
            key_value_line("Rate", "Activity sparkline"),
            key_value_line("Tasks", "Checklist progress (3/5)"),
            key_value_line("MR", "Merge request status"),
            key_value_line("Pipeline", "CI/CD status"),
            key_value_line("Server", "Dev server status"),
            key_value_line("Task", "Linked project task name"),
            key_value_line("Task St", "Task status"),
            key_value_line("Note", "Your custom notes"),
        ]
    }

    fn preview_tabs_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Preview Panel Tabs",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Use Tab / Shift+Tab to switch between tabs:",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            key_value_line("Preview", "See the agent's live output"),
            key_value_line("Git Diff", "View uncommitted changes"),
            key_value_line("Dev Server", "Dev server logs & status"),
        ]
    }

    fn navigation_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Navigation Keybinds",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            key_value_line("↑/↓  or j/k", "Move between agents"),
            key_value_line("g", "Go to first agent"),
            key_value_line("G (Shift+g)", "Go to last agent"),
            key_value_line("Enter", "Attach to agent's tmux session"),
            key_value_line("Ctrl+b+d", "Detatch from a agent's tmux session"),
            key_value_line("?", "Show all keybinds"),
        ]
    }

    fn agent_management_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Managing Agents",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            key_value_line("n", "Create new agent (prompts for name/branch)"),
            key_value_line("d", "Delete selected agent"),
            key_value_line("c", "Copy cd command to worktree"),
            key_value_line("y", "Copy agent name to clipboard"),
            key_value_line("N (Shift+n)", "Set a custom note"),
            key_value_line("C (Shift+c)", "Toggle auto-continue on startup"),
            key_value_line("e", "Open worktree in editor"),
        ]
    }

    fn task_management_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Working with Tasks",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            key_value_line("t", "Browse tasks from your project management tool"),
            key_value_line("a", "Assign task to current agent"),
            key_value_line("A (Shift+a)", "Open task in browser"),
            key_value_line("T (Shift+T)", "Change task status (dropdown)"),
            Line::from(""),
            Line::from(Span::styled(
                "  Tasks integrate with: Asana, Notion, ClickUp, Airtable, Linear",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    }

    fn git_operations_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Git Operations",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            key_value_line("m", "Merge main into current branch"),
            key_value_line("p", "Push changes to remote"),
            key_value_line("f", "Fetch from remote"),
            key_value_line("o", "Open MR/PR in browser"),
            key_value_line("s", "Request work summary from agent"),
            Line::from(""),
            Line::from(Span::styled(
                "  Git integrates with: Github, Gitlab, Codeberg",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    }

    fn automation_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Automation Settings",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Configure in Settings (S) → Automation tab:",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  • On Task Assign - Auto-change task status when assigned",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  • On Push - Auto-change status when pushing",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  • On Delete - Auto-complete task when deleting agent",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Example: Set task to \"In Progress\" when assigned,",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "           \"Review\" when pushed, \"Done\" when deleted.",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    }

    fn dev_server_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Dev Server",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Each agent can run its own dev server.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Switch to Dev Server tab with Tab, then:",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            key_value_line("Ctrl+s", "Start dev server"),
            Line::from(""),
            Line::from(Span::styled(
                "  Configure in Settings → Dev Server tab",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    }

    fn workflows_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Example Workflows",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Feature Development:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "     [t] Browse tasks → [Enter] Create agent from task",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "     → Work on feature → [m] Merge main → [p] Push",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "     → [o] Open MR → [Shift+t] Update task status when done",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  2. Bug Fix:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "     [n] Create agent \"fix-login-bug\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "     → [Enter] Attach and work → [s] Get summary",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "     → [p] Push → [d] Delete when merged",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  3. Parallel Development:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "     Create multiple agents for different features",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "     → Switch between them with ↑/↓",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "     → [Shift+t] Update each task's status as you progress",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "     → Monitor all in the agent list",
                Style::default().fg(Color::Gray),
            )),
        ]
    }

    fn getting_help_content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Getting Help",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            key_value_line("?", "Show keybind reference"),
            key_value_line("S (Shift+s)", "Open settings"),
            Line::from(""),
            Line::from(Span::styled(
                "  Settings tabs:",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  • General     - AI agent, editor, display options",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  • Git         - Provider, branch settings",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  • Project Mgmt - Task integration",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  • Dev Server  - Dev server configuration",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  • Automation  - Auto-status rules",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  • Keybinds    - Customize all keybinds",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press Enter to complete the tutorial!",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
        ]
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let is_last_step = matches!(self.state.step, TutorialStep::GettingHelp);
        let hint = if is_last_step {
            "[←/h] Back  [Enter] Complete  [Esc/q] Skip"
        } else {
            "[←/h] Back  [Enter/→/l] Next  [Esc/q] Skip"
        };

        let paragraph = Paragraph::new(Line::from(vec![Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )]))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }
}

fn key_value_line(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("    ", Style::default()),
        Span::styled(format!("{:14}", key), Style::default().fg(Color::Cyan)),
        Span::styled("- ", Style::default().fg(Color::DarkGray)),
        Span::styled(value.to_string(), Style::default().fg(Color::Gray)),
    ])
}
