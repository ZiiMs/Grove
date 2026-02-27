use std::collections::{HashMap, HashSet};

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::config::{AppearanceConfig, ProjectMgmtProvider};
use crate::app::{StatusOption, TaskListItem};
use crate::ui::helpers::centered_rect;

fn status_icon_and_color(status_name: &str) -> (&'static str, Color) {
    let lower = status_name.to_lowercase();
    if lower.contains("progress")
        || lower.contains("doing")
        || lower.contains("review")
        || lower.contains("started")
    {
        ("◐", Color::Yellow)
    } else if lower.contains("done")
        || lower.contains("complete")
        || lower.contains("closed")
        || lower.contains("cancel")
    {
        ("✓", Color::Green)
    } else {
        ("○", Color::Gray)
    }
}

fn get_status_appearance(
    status_name: &str,
    appearance_config: &AppearanceConfig,
    provider: ProjectMgmtProvider,
) -> (String, Color) {
    let provider_config = appearance_config.get_for_provider(provider);
    if let Some(appearance) = provider_config.statuses.get(status_name) {
        let icon = appearance.icon.clone();
        let color = crate::ui::parse_color(&appearance.color);
        (icon, color)
    } else {
        let (icon, color) = status_icon_and_color(status_name);
        (icon.to_string(), color)
    }
}

pub struct TaskListModal<'a> {
    tasks: &'a [TaskListItem],
    selected_actual_idx: usize,
    scroll_offset: usize,
    loading: bool,
    provider_name: &'a str,
    pm_provider: ProjectMgmtProvider,
    appearance_config: &'a AppearanceConfig,
    assigned_tasks: &'a HashMap<String, String>,
    expanded_ids: &'a HashSet<String>,
    hidden_status_names: &'a [String],
    status_options: &'a [StatusOption],
    filter_open: bool,
    filter_selected: usize,
}

impl<'a> TaskListModal<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tasks: &'a [TaskListItem],
        selected_actual_idx: usize,
        scroll_offset: usize,
        loading: bool,
        provider_name: &'a str,
        pm_provider: ProjectMgmtProvider,
        appearance_config: &'a AppearanceConfig,
        assigned_tasks: &'a HashMap<String, String>,
        expanded_ids: &'a HashSet<String>,
        hidden_status_names: &'a [String],
        status_options: &'a [StatusOption],
        filter_open: bool,
        filter_selected: usize,
    ) -> Self {
        Self {
            tasks,
            selected_actual_idx,
            scroll_offset,
            loading,
            provider_name,
            pm_provider,
            appearance_config,
            assigned_tasks,
            expanded_ids,
            hidden_status_names,
            status_options,
            filter_open,
            filter_selected,
        }
    }

    fn is_status_hidden(&self, status_name: &str) -> bool {
        self.hidden_status_names.contains(&status_name.to_string())
    }

    fn filter_tasks(&self) -> Vec<&'a TaskListItem> {
        self.tasks
            .iter()
            .filter(|task| !self.is_status_hidden(&task.status_name))
            .collect()
    }

    fn compute_visible_tasks(
        &self,
        filtered_tasks: &[&'a TaskListItem],
    ) -> Vec<(usize, &'a TaskListItem, usize)> {
        let task_indices: HashMap<&str, usize> = self
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.as_str(), i))
            .collect();

        let child_to_parent: HashMap<&str, &str> = self
            .tasks
            .iter()
            .filter_map(|t| t.parent_id.as_ref().map(|p| (t.id.as_str(), p.as_str())))
            .collect();

        fn get_depth_by_id<'a>(
            id: &'a str,
            child_to_parent: &HashMap<&'a str, &'a str>,
            visited: &mut HashSet<&'a str>,
        ) -> usize {
            if visited.contains(id) {
                return 0;
            }
            visited.insert(id);
            match child_to_parent.get(id) {
                None => 0,
                Some(&parent_id) => get_depth_by_id(parent_id, child_to_parent, visited) + 1,
            }
        }

        fn get_depth<'a>(
            task: &'a TaskListItem,
            child_to_parent: &HashMap<&'a str, &'a str>,
        ) -> usize {
            match &task.parent_id {
                None => 0,
                Some(_) => {
                    let parent = child_to_parent.get(task.id.as_str());
                    match parent {
                        None => 1,
                        Some(&pid) => {
                            let mut visited = HashSet::new();
                            visited.insert(task.id.as_str());
                            get_depth_by_id(pid, child_to_parent, &mut visited) + 1
                        }
                    }
                }
            }
        }

        let mut visible = Vec::new();
        for task in filtered_tasks {
            let is_visible = if task.parent_id.is_none() {
                true
            } else {
                self.is_ancestor_expanded_and_visible(task, &child_to_parent)
            };

            if is_visible {
                let actual_idx = *task_indices.get(task.id.as_str()).unwrap_or(&0);
                let depth = get_depth(task, &child_to_parent);
                visible.push((actual_idx, *task, depth));
            }
        }

        visible
    }

    fn is_ancestor_expanded_and_visible(
        &self,
        task: &TaskListItem,
        child_to_parent: &HashMap<&str, &str>,
    ) -> bool {
        let mut current_id = task.id.as_str();
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(current_id) {
                return true;
            }
            match child_to_parent.get(current_id) {
                None => return true,
                Some(&parent_id) => {
                    if !self.expanded_ids.contains(parent_id) {
                        return false;
                    }
                    let parent_task = self.tasks.iter().find(|t| t.id == parent_id);
                    if let Some(parent) = parent_task {
                        if self.is_status_hidden(&parent.status_name) {
                            return false;
                        }
                    }
                    current_id = parent_id;
                }
            }
        }
    }

    fn render_filter_bar(&self, frame: &mut Frame, area: Rect) {
        let mut spans = Vec::new();

        if self.hidden_status_names.is_empty() {
            spans.push(Span::styled(
                "[f] Filter",
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            spans.push(Span::styled("Hidden: ", Style::default().fg(Color::White)));
            for (i, name) in self.hidden_status_names.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled(", ", Style::default().fg(Color::DarkGray)));
                }
                spans.push(Span::styled(
                    name.clone(),
                    Style::default().fg(Color::Yellow),
                ));
            }
            spans.push(Span::styled("  ", Style::default()));
            spans.push(Span::styled(
                "[f] Filter",
                Style::default().fg(Color::DarkGray),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans));
        frame.render_widget(paragraph, area);
    }

    fn render_filter_modal(&self, frame: &mut Frame, area: Rect) {
        if self.status_options.is_empty() {
            return;
        }

        let modal_area = centered_rect(45, 35, area);
        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title(" Filter Tasks ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner_area = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(inner_area);

        let parent_count = self.status_options.iter().filter(|o| !o.is_child).count();
        let has_children = self.status_options.iter().any(|o| o.is_child);

        let mut items: Vec<ListItem> = self
            .status_options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let is_hidden = self.is_status_hidden(&opt.name);
                let is_selected = i == self.filter_selected;

                let (status_icon, status_color) =
                    get_status_appearance(&opt.name, self.appearance_config, self.pm_provider);

                let check = if is_hidden { "[ ]" } else { "[✓]" };
                let text = format!("  {} {} {}", check, status_icon, opt.name);

                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if is_hidden {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(status_color)
                };

                ListItem::new(Line::from(Span::styled(text, style)))
            })
            .collect();

        if has_children && parent_count > 0 && parent_count < items.len() {
            let separator = ListItem::new(Line::from(Span::styled(
                "  ── Subtasks ──",
                Style::default().fg(Color::DarkGray),
            )));
            items.insert(parent_count, separator);
        }

        let list = List::new(items);
        frame.render_widget(list, chunks[0]);

        let help = Paragraph::new(Line::from(vec![
            Span::styled("[j/k] Navigate  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Space] Toggle  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Esc/f] Close", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(help, chunks[1]);
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(75, 65, frame.area());
        frame.render_widget(Clear, area);

        let title = format!(" {} Tasks ", self.provider_name);

        let block = Block::default()
            .title(title)
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
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(inner_area);

        self.render_filter_bar(frame, chunks[0]);

        let filtered_tasks = self.filter_tasks();

        if filtered_tasks.is_empty() {
            let empty_text = Paragraph::new("No tasks match current filter")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(empty_text, chunks[1]);
        } else {
            let visible_tasks = self.compute_visible_tasks(&filtered_tasks);

            if visible_tasks.is_empty() {
                let empty_text = Paragraph::new("No visible tasks")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(ratatui::layout::Alignment::Center);
                frame.render_widget(empty_text, chunks[1]);
            } else {
                self.render_task_list(frame, chunks[1], &visible_tasks);
            }
        }

        let help_spans = if self.filter_open {
            vec![
                Span::styled("[j/k] Navigate  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[Space] Toggle  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[Esc/f] Close", Style::default().fg(Color::DarkGray)),
            ]
        } else {
            vec![
                Span::styled("[j/k] Navigate  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[Enter] Create  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[a] Assign  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[s] Status  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[f] Filter  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[r] Refresh  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[←/→] Expand  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[Esc] Close", Style::default().fg(Color::DarkGray)),
            ]
        };

        let help_text =
            Paragraph::new(Line::from(help_spans)).alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(help_text, chunks[2]);

        if self.filter_open {
            self.render_filter_modal(frame, area);
        }
    }

    fn render_task_list(
        &self,
        frame: &mut Frame,
        area: Rect,
        visible_tasks: &[(usize, &TaskListItem, usize)],
    ) {
        let total_tasks = visible_tasks.len();
        let selected_visible_pos = visible_tasks
            .iter()
            .position(|(actual_idx, _, _)| *actual_idx == self.selected_actual_idx)
            .unwrap_or(0);

        let available_height = area.height as usize;

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

        let available_width = area.width as usize;
        let min_width = 30;
        let max_allowed = (available_width.saturating_sub(25)).min(60).max(min_width);
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

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let (status_icon, status_color) =
                get_status_appearance(&task.status_name, self.appearance_config, self.pm_provider);

            let status_name_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(status_color)
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
                Span::styled(truncated_name, style),
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("[{} {}]", status_icon, task.status_name),
                    status_name_style,
                ),
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
        frame.render_widget(list, area);
    }
}
