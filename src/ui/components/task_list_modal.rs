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
    scroll_offset: usize,
    loading: bool,
    provider_name: &'a str,
    assigned_tasks: &'a HashMap<String, String>,
    expanded_ids: &'a HashSet<String>,
}

impl<'a> TaskListModal<'a> {
    pub fn new(
        tasks: &'a [TaskListItem],
        selected_actual_idx: usize,
        scroll_offset: usize,
        loading: bool,
        provider_name: &'a str,
        assigned_tasks: &'a HashMap<String, String>,
        expanded_ids: &'a HashSet<String>,
    ) -> Self {
        Self {
            tasks,
            selected_actual_idx,
            scroll_offset,
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

        if visible_tasks.is_empty() {
            let empty_text = Paragraph::new("No visible tasks")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(empty_text, chunks[0]);
            return;
        }

        let total_tasks = visible_tasks.len();
        let selected_visible_pos = visible_tasks
            .iter()
            .position(|(actual_idx, _, _)| *actual_idx == self.selected_actual_idx)
            .unwrap_or(0);

        let available_height = chunks[0].height as usize;

        let mut scroll_offset = self.scroll_offset.min(total_tasks.saturating_sub(1));

        for _ in 0..3 {
            if selected_visible_pos < scroll_offset {
                scroll_offset = selected_visible_pos;
            }

            let has_above = scroll_offset > 0;
            let has_below_estimate = scroll_offset + available_height < total_tasks;
            let indicator_rows =
                (if has_above { 1 } else { 0 }) + (if has_below_estimate { 1 } else { 0 });

            let max_visible = available_height.saturating_sub(indicator_rows);

            let max_scroll = total_tasks.saturating_sub(max_visible);
            scroll_offset = scroll_offset.min(max_scroll);

            if selected_visible_pos >= scroll_offset + max_visible {
                scroll_offset = selected_visible_pos.saturating_sub(max_visible - 1);
            }
        }

        let has_above = scroll_offset > 0;
        let max_visible_final = available_height.saturating_sub(if has_above { 1 } else { 0 });
        let has_below = scroll_offset + max_visible_final < total_tasks;
        let effective_visible = available_height
            .saturating_sub(if has_above { 1 } else { 0 } + if has_below { 1 } else { 0 });

        if has_below && selected_visible_pos >= scroll_offset + effective_visible {
            scroll_offset =
                selected_visible_pos.saturating_sub(effective_visible.saturating_sub(1));
        }

        let has_above = scroll_offset > 0;
        let has_below = scroll_offset + effective_visible < total_tasks;

        let end_index = (scroll_offset + effective_visible).min(total_tasks);
        let visible_slice = &visible_tasks[scroll_offset..end_index];

        let content_based_width = visible_tasks
            .iter()
            .map(|(_, t, depth)| t.name.chars().count() + depth * 2)
            .max()
            .unwrap_or(20);

        let available_width = chunks[0].width as usize;
        let min_width = 30;
        let max_allowed = (available_width.saturating_sub(25)).min(60);
        let max_name_width = content_based_width.clamp(min_width, max_allowed);

        let mut items: Vec<ListItem> = Vec::new();

        if has_above {
            let hidden_above = scroll_offset;
            let indicator = ListItem::new(Line::from(vec![Span::styled(
                format!("▲ {} more above", hidden_above),
                Style::default().fg(Color::Yellow),
            )]));
            items.push(indicator);
        }

        for (i, (_actual_idx, task, depth)) in visible_slice.iter().enumerate() {
            let visible_pos = scroll_offset + i;
            let is_selected = visible_pos == selected_visible_pos;

            let (status_icon, status_color) = match &task.status {
                TaskItemStatus::NotStarted => ("○", Color::Gray),
                TaskItemStatus::InProgress => ("◐", Color::Yellow),
                TaskItemStatus::Completed => ("✓", Color::Green),
            };

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
            items.push(ListItem::new(line));
        }

        if has_below {
            let hidden_below = total_tasks - scroll_offset - effective_visible;
            if hidden_below > 0 {
                let indicator = ListItem::new(Line::from(vec![Span::styled(
                    format!("▼ {} more below", hidden_below),
                    Style::default().fg(Color::Yellow),
                )]));
                items.push(indicator);
            }
        }

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
