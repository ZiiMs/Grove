use std::collections::HashMap;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{TaskItemStatus, TaskListItem};

pub struct TaskListModal<'a> {
    tasks: &'a [TaskListItem],
    selected: usize,
    loading: bool,
    provider_name: &'a str,
    assigned_tasks: &'a HashMap<String, String>,
}

impl<'a> TaskListModal<'a> {
    pub fn new(
        tasks: &'a [TaskListItem],
        selected: usize,
        loading: bool,
        provider_name: &'a str,
        assigned_tasks: &'a HashMap<String, String>,
    ) -> Self {
        Self {
            tasks,
            selected,
            loading,
            provider_name,
            assigned_tasks,
        }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(75, 65, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(format!(" {} Tasks ", self.provider_name))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        if self.loading {
            let loading_text = Paragraph::new("Loading tasks...")
                .style(Style::default().fg(Color::Yellow))
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(loading_text, inner_area);
            return;
        }

        if self.tasks.is_empty() {
            let empty_text = Paragraph::new(
                "No tasks found\n\nMake sure your database is configured correctly.",
            )
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(empty_text, inner_area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(2)])
            .split(inner_area);

        let max_name_width = self
            .tasks
            .iter()
            .map(|t| t.name.chars().count())
            .max()
            .unwrap_or(20)
            .min(50);

        let items: Vec<ListItem> = self
            .tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let (status_icon, status_color) = match &task.status {
                    TaskItemStatus::NotStarted => ("○", Color::Gray),
                    TaskItemStatus::InProgress => ("◐", Color::Yellow),
                };

                let style = if i == self.selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let status_style = if i == self.selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(status_color)
                };

                let status_name_style = if i == self.selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let normalized_id = task.id.replace('-', "").to_lowercase();
                let assigned_info = self.assigned_tasks.get(&normalized_id);
                let assigned_style = if i == self.selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::Green)
                };

                let padded_name = format!("{:width$}", task.name, width = max_name_width);
                let truncated_name = if padded_name.chars().count() > 50 {
                    format!("{}…", padded_name.chars().take(49).collect::<String>())
                } else {
                    padded_name
                };

                let mut spans = vec![
                    Span::styled(format!("{} ", status_icon), status_style),
                    Span::styled(truncated_name, style),
                    Span::styled("  ", Style::default()),
                    Span::styled(format!("[{}]", task.status_name), status_name_style),
                ];

                if let Some(agent_name) = assigned_info {
                    spans.push(Span::styled("  ", Style::default()));
                    spans.push(Span::styled(format!("✓ @{}", agent_name), assigned_style));
                }

                let line = Line::from(spans);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, chunks[0]);

        let help_text = Paragraph::new(Line::from(vec![
            Span::styled("[j/k] Navigate  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "[Enter] Create Agent  ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("[a] Assign  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Esc] Cancel", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(help_text, chunks[1]);
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
