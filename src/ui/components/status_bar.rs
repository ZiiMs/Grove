use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::config::Keybinds;

pub struct StatusBarWidget<'a> {
    message: Option<&'a str>,
    is_error: bool,
    keybinds: &'a Keybinds,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(message: Option<&'a str>, is_error: bool, keybinds: &'a Keybinds) -> Self {
        Self {
            message,
            is_error,
            keybinds,
        }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let shortcuts = [
            (&self.keybinds.new_agent, "new"),
            (&self.keybinds.attach, "attach"),
            (&self.keybinds.delete_agent, "delete"),
            (&self.keybinds.refresh_all, "refresh"),
            (&self.keybinds.toggle_settings, "settings"),
            (&self.keybinds.toggle_help, "help"),
            (&self.keybinds.quit, "quit"),
        ];

        let mut spans: Vec<Span> = Vec::new();

        for (i, (keybind, action)) in shortcuts.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("[{}]", keybind.display_short()),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                action.to_string(),
                Style::default().fg(Color::White),
            ));
        }

        let line = if let Some(msg) = self.message {
            let style = if self.is_error {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            };
            Line::from(Span::styled(msg.to_string(), style))
        } else {
            Line::from(spans)
        };

        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }
}

pub struct InputBarWidget<'a> {
    prompt: &'a str,
    input: &'a str,
}

impl<'a> InputBarWidget<'a> {
    pub fn new(prompt: &'a str, input: &'a str) -> Self {
        Self { prompt, input }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let line = Line::from(vec![
            Span::styled(self.prompt.to_string(), Style::default().fg(Color::Yellow)),
            Span::styled(self.input.to_string(), Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(Color::White)),
        ]);

        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }
}
