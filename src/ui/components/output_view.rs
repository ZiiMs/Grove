use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub struct OutputViewWidget<'a> {
    title: &'a str,
    content: &'a str,
    scroll: usize,
}

impl<'a> OutputViewWidget<'a> {
    pub fn new(title: &'a str, content: &'a str) -> Self {
        Self {
            title,
            content,
            scroll: 0,
        }
    }

    pub fn with_scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let visible_height = area.height.saturating_sub(2) as usize; // Account for borders

        // Preprocess content: convert tabs to spaces and handle carriage returns
        // This fixes alignment issues where tabs render as single characters in ratatui
        let processed_content = self
            .content
            .replace('\t', "    ") // Convert tabs to 4 spaces
            .replace("\r\n", "\n") // Normalize Windows line endings
            .replace('\r', ""); // Remove carriage returns (some programs use \r to overwrite lines)

        // Since we capture without ANSI codes (-e), just use raw text
        let lines: Vec<Line> = processed_content.lines().map(Line::from).collect();

        // Calculate scroll position (show latest content by default)
        let total_lines = lines.len();
        let start = if total_lines > visible_height {
            total_lines
                .saturating_sub(visible_height)
                .saturating_sub(self.scroll)
        } else {
            0
        };

        // Take only visible lines
        let visible_lines: Vec<Line> = lines.into_iter().skip(start).take(visible_height).collect();

        let paragraph = Paragraph::new(visible_lines)
            .block(
                Block::default()
                    .title(format!(" {} ", self.title))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }
}

pub struct EmptyOutputWidget;

impl EmptyOutputWidget {
    pub fn render(frame: &mut Frame, area: Rect) {
        let paragraph = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No agent selected",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'n' to create a new agent",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .title(" PREVIEW ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(paragraph, area);
    }
}
