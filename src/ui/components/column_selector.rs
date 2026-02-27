use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::state::ColumnOption;
use crate::ui::helpers::centered_rect;

pub struct ColumnSelectorWidget<'a> {
    columns: &'a [ColumnOption],
    selected_index: usize,
}

impl<'a> ColumnSelectorWidget<'a> {
    pub fn new(columns: &'a [ColumnOption], selected_index: usize) -> Self {
        Self {
            columns,
            selected_index,
        }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(50, 60, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" TOGGLE COLUMNS ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(inner);

        self.render_header(frame, chunks[0]);
        self.render_list(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let text = Line::from("  Toggle which columns to show in the agents list");
        let paragraph = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(paragraph, area);
    }

    fn render_list(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let checkbox = if col.visible { "[x]" } else { "[ ]" };
                let text = format!("  {}  {}", checkbox, col.label);
                let style = if i == self.selected_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::NONE))
            .style(Style::default());

        frame.render_widget(list, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let key_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let text = Line::from(vec![
            Span::styled("Space", key_style),
            Span::styled(" Toggle  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", key_style),
            Span::styled(" Close", Style::default().fg(Color::DarkGray)),
        ]);
        let paragraph = Paragraph::new(text);
        frame.render_widget(paragraph, area);
    }
}
