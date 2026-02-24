use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::SubtaskStatusDropdownState;
use crate::ui::helpers::centered_rect;

pub struct SubtaskStatusDropdown<'a> {
    state: &'a SubtaskStatusDropdownState,
}

impl<'a> SubtaskStatusDropdown<'a> {
    pub fn new(state: &'a SubtaskStatusDropdownState) -> Self {
        Self { state }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(40, 35, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Toggle Subtask Status ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(inner_area);

        let task_text = Paragraph::new(Line::from(vec![Span::styled(
            format!("  {}", self.state.task_name),
            Style::default().fg(Color::Yellow),
        )]));
        frame.render_widget(task_text, chunks[0]);

        let options = ["Not Complete", "Complete"];
        let items: Vec<ListItem> = options
            .iter()
            .enumerate()
            .map(|(i, &option)| {
                let is_selected = i == self.state.selected_index;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let check = if (i == 1) == self.state.current_completed {
                    "✓ "
                } else {
                    "  "
                };

                let line = Line::from(vec![Span::styled(format!("{} {}", check, option), style)]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, chunks[1]);

        let help_text = Paragraph::new(Line::from(vec![
            Span::styled("[j/k] Navigate  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Enter] Select  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Esc] Cancel", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(help_text, chunks[2]);
    }
}
