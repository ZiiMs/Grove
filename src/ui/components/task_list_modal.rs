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
}

impl<'a> TaskListModal<'a> {
    pub fn new(
        tasks: &'a [TaskListItem],
        selected: usize,
        loading: bool,
        provider_name: &'a str,
    ) -> Self {
        Self {
            tasks,
            selected,
            loading,
            provider_name,
        }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(70, 60, frame.area());
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
            let empty_text = Paragraph::new("No tasks found")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(empty_text, inner_area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(2)])
            .split(inner_area);

        let items: Vec<ListItem> = self
            .tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let status_style = match &task.status {
                    TaskItemStatus::NotStarted => Style::default().fg(Color::Gray),
                    TaskItemStatus::InProgress => Style::default().fg(Color::Yellow),
                };

                let status_text = match &task.status {
                    TaskItemStatus::NotStarted => "○",
                    TaskItemStatus::InProgress => "◐",
                };

                let style = if i == self.selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let line = Line::from(vec![
                    Span::styled(format!("{} ", status_text), status_style),
                    Span::styled(&task.name, style),
                ]);

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
