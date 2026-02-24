use std::collections::HashMap;
use uuid::Uuid;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::agent::Agent;
use crate::app::TaskReassignmentWarning;
use crate::ui::helpers::centered_rect;

pub struct TaskReassignmentWarningModal<'a> {
    warning: &'a TaskReassignmentWarning,
    agents: &'a HashMap<Uuid, Agent>,
}

impl<'a> TaskReassignmentWarningModal<'a> {
    pub fn new(warning: &'a TaskReassignmentWarning, agents: &'a HashMap<Uuid, Agent>) -> Self {
        Self { warning, agents }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(60, 50, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" TASK REASSIGNMENT ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let target_agent_name = self
            .agents
            .get(&self.warning.target_agent_id)
            .map(|a| a.name.as_str())
            .unwrap_or("Unknown");

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("Assigning task: {}", self.warning.task_name),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("To agent: {}", target_agent_name),
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
        ];

        let has_agent_conflict = self.warning.agent_current_task.is_some();
        let has_task_conflict = self.warning.task_current_agent.is_some();

        if has_agent_conflict || has_task_conflict {
            lines.push(Line::from(Span::styled(
                "This will:",
                Style::default().fg(Color::White),
            )));
        }

        if let Some((_task_id, task_name)) = &self.warning.agent_current_task {
            lines.push(Line::from(Span::styled(
                format!("  - Unassign '{}' from {}", task_name, target_agent_name),
                Style::default().fg(Color::DarkGray),
            )));
        }

        if let Some((_agent_id, agent_name)) = &self.warning.task_current_agent {
            lines.push(Line::from(Span::styled(
                format!("  - Remove this task from agent '{}'", agent_name),
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines.extend(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Continue with reassignment?",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[y] Yes    [n/Esc] Cancel",
                Style::default().fg(Color::Cyan),
            )),
        ]);

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }
}
