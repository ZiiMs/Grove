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
    let toast_height = 3u16;
    let bottom_padding = 2u16;

    let max_width = frame_area.width.saturating_sub(4);
    let desired_width = (message_len as u16 + 6).min(max_width);
    let toast_width = desired_width.max(20.min(max_width));

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(toast_height),
            Constraint::Length(bottom_padding),
        ])
        .split(frame_area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(toast_width),
            Constraint::Min(0),
        ])
        .split(popup_layout[1])[1]
}
