use std::collections::{HashMap, HashSet};

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
    selected_actual_idx: usize,
    loading: bool,
    provider_name: &'a str,
    assigned_tasks: &'a HashMap<String, String>,
    expanded_ids: &'a HashSet<String>,
}

impl<'a> TaskListModal<'a> {
    pub fn new(
        tasks: &'a [TaskListItem],
        selected_actual_idx: usize,
        loading: bool,
        provider_name: &'a str,
        assigned_tasks: &'a HashMap<String, String>,
        expanded_ids: &'a HashSet<String>,
    ) -> Self {
        Self {
            tasks,
            selected_actual_idx,
            loading,
            provider_name,
            assigned_tasks,
            expanded_ids,
        }
    }

    fn compute_visible_tasks(&self) -> Vec<(usize, &'a TaskListItem, usize)> {
        let child_to_parent: HashMap<&str, &str> = self
            .tasks
            .iter()
            .filter_map(|t| t.parent_id.as_ref().map(|p| (t.id.as_str(), p.as_str())))
            .collect();

        fn get_depth_by_id(id: &str, child_to_parent: &HashMap<&str, &str>) -> usize {
            match child_to_parent.get(id) {
                None => 0,
                Some(&parent_id) => get_depth_by_id(parent_id, child_to_parent) + 1,
            }
        }

        fn get_depth(task: &TaskListItem, child_to_parent: &HashMap<&str, &str>) -> usize {
            match &task.parent_id {
                None => 0,
                Some(_) => {
                    let parent = child_to_parent.get(task.id.as_str());
                    match parent {
                        None => 1,
                        Some(&pid) => get_depth_by_id(pid, child_to_parent) + 1,
                    }
                }
            }
        }

        let mut visible = Vec::new();
        for (idx, task) in self.tasks.iter().enumerate() {
            let is_visible = if task.parent_id.is_none() {
                true
            } else {
                self.is_ancestor_expanded(task, &child_to_parent)
            };

            if is_visible {
                let depth = get_depth(task, &child_to_parent);
                visible.push((idx, task, depth));
            }
        }

        visible
    }

    fn is_ancestor_expanded(
        &self,
        task: &TaskListItem,
        child_to_parent: &HashMap<&str, &str>,
    ) -> bool {
        let mut current_id = task.id.as_str();
        loop {
            match child_to_parent.get(current_id) {
                None => return true,
                Some(&parent_id) => {
                    if !self.expanded_ids.contains(parent_id) {
                        return false;
                    }
                    current_id = parent_id;
                }
            }
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

        let visible_tasks = self.compute_visible_tasks();

        let selected_visible_pos = visible_tasks
            .iter()
            .position(|(actual_idx, _, _)| *actual_idx == self.selected_actual_idx)
            .unwrap_or(0);

        let content_based_width = visible_tasks
            .iter()
            .map(|(_, t, depth)| t.name.chars().count() + depth * 2)
            .max()
            .unwrap_or(20);

        let available_width = chunks[0].width as usize;
        let min_width = 30;
        let max_allowed = (available_width.saturating_sub(25)).min(60);
        let max_name_width = content_based_width.clamp(min_width, max_allowed);

        let items: Vec<ListItem> = visible_tasks
            .iter()
            .enumerate()
            .map(|(visible_pos, (_actual_idx, task, depth))| {
                let (status_icon, status_color) = match &task.status {
                    TaskItemStatus::NotStarted => ("○", Color::Gray),
                    TaskItemStatus::InProgress => ("◐", Color::Yellow),
                    TaskItemStatus::Completed => ("✓", Color::Green),
                };

                let is_selected = visible_pos == selected_visible_pos;

                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let status_style = if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(status_color)
                };

                let status_name_style = if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let normalized_id = task.id.replace('-', "").to_lowercase();
                let assigned_info = self.assigned_tasks.get(&normalized_id);
                let assigned_style = if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::Green)
                };

                let indent = " ".repeat(*depth * 2);
                let expand_indicator = if task.has_children {
                    if self.expanded_ids.contains(&task.id) {
                        "▼ "
                    } else {
                        "► "
                    }
                } else {
                    "  "
                };

                let display_name = format!("{}{}{}", indent, expand_indicator, task.name);
                let truncated_name = if display_name.chars().count() > max_name_width {
                    format!(
                        "{}…",
                        display_name
                            .chars()
                            .take(max_name_width - 1)
                            .collect::<String>()
                    )
                } else {
                    format!("{:width$}", display_name, width = max_name_width)
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
            Span::styled(
                "[↑/k][↓/j] Navigate  ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "[Enter] Create Agent  ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("[a] Assign  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[s] Toggle Status  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[r] Refresh  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "[←/→] Collapse/Expand  ",
                Style::default().fg(Color::DarkGray),
            ),
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
