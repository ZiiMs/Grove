use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::TaskStatusDropdownState;
use crate::ui::helpers::centered_rect;

pub struct StatusDropdown<'a> {
    state: &'a TaskStatusDropdownState,
}

impl<'a> StatusDropdown<'a> {
    pub fn new(state: &'a TaskStatusDropdownState) -> Self {
        Self { state }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(40, 50, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" Select Status ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        if self.state.status_options.is_empty() {
            let empty_text = Paragraph::new("No status options available")
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
            .state
            .status_options
            .iter()
            .enumerate()
            .map(|(i, option)| {
                let style = if i == self.state.selected_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let line = Line::from(vec![Span::styled(format!("  {}  ", option.name), style)]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, chunks[0]);

        let help_text = Paragraph::new(Line::from(vec![
            Span::styled("[j/k] Navigate  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Enter] Select  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Esc] Cancel", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(help_text, chunks[1]);
    }
}
