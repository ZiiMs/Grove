use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{Toast, ToastLevel};

pub struct ToastWidget<'a> {
    toast: &'a Toast,
}

impl<'a> ToastWidget<'a> {
    pub fn new(toast: &'a Toast) -> Self {
        Self { toast }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = get_toast_area(frame.area(), self.toast.message.len());
        frame.render_widget(Clear, area);

        let (border_color, text_color, icon) = match self.toast.level {
            ToastLevel::Success => (Color::Green, Color::Green, "✓"),
            ToastLevel::Info => (Color::Cyan, Color::Cyan, "ℹ"),
            ToastLevel::Warning => (Color::Yellow, Color::Yellow, "⚠"),
            ToastLevel::Error => (Color::Red, Color::Red, "✗"),
        };

        let paragraph = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", icon),
                Style::default().fg(text_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(&self.toast.message, Style::default().fg(text_color)),
        ]))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );

        frame.render_widget(paragraph, area);
    }
}

fn get_toast_area(frame_area: Rect, message_len: usize) -> Rect {
    let toast_width = (message_len as u16 + 6).min(frame_area.width - 4).max(20);
    let toast_height = 3u16;

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(frame_area.height.saturating_sub(toast_height + 2)),
            Constraint::Length(toast_height),
            Constraint::Length(2),
        ])
        .split(frame_area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((frame_area.width.saturating_sub(toast_width)) / 2),
            Constraint::Length(toast_width),
            Constraint::Length((frame_area.width.saturating_sub(toast_width)) / 2),
        ])
        .split(popup_layout[1])[1]
}
