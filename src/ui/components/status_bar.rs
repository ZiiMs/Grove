use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub struct StatusBarWidget<'a> {
    message: Option<&'a str>,
    is_error: bool,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(message: Option<&'a str>, is_error: bool) -> Self {
        Self { message, is_error }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let shortcuts = vec![
            ("n", "new"),
            ("d", "del"),
            ("Enter", "attach"),
            ("s", "summary"),
            ("m", "merge"),
            ("p", "push"),
            ("a", "asana"),
            ("N", "note"),
            ("R", "refresh"),
            ("?", "help"),
            ("q", "quit"),
        ];

        let mut spans: Vec<Span> = Vec::new();

        for (i, (key, action)) in shortcuts.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("[{}]", key),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                action.to_string(),
                Style::default().fg(Color::White),
            ));
        }

        // If there's a message, show it instead of shortcuts
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
