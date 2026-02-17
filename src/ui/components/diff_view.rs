use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub struct DiffViewWidget<'a> {
    title: &'a str,
    diff_content: &'a str,
    scroll: usize,
}

impl<'a> DiffViewWidget<'a> {
    pub fn new(title: &'a str, diff_content: &'a str, scroll: usize) -> Self {
        Self {
            title,
            diff_content,
            scroll,
        }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let visible_height = area.height.saturating_sub(2) as usize;

        let lines: Vec<Line> = self
            .diff_content
            .lines()
            .skip(self.scroll)
            .take(visible_height)
            .map(|line| self.colorize_diff_line(line))
            .collect();

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(" DIFF: {} ", self.title))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    fn colorize_diff_line(&self, line: &str) -> Line<'a> {
        let (style, prefix) = if line.starts_with('+') && !line.starts_with("+++") {
            (Style::default().fg(Color::Green), "+")
        } else if line.starts_with('-') && !line.starts_with("---") {
            (Style::default().fg(Color::Red), "-")
        } else if line.starts_with("@@") {
            (Style::default().fg(Color::Cyan), "@")
        } else if line.starts_with("diff ") || line.starts_with("index ") {
            (Style::default().fg(Color::Yellow), "")
        } else if line.starts_with("---") || line.starts_with("+++") {
            (Style::default().fg(Color::Yellow), "")
        } else {
            (Style::default().fg(Color::White), " ")
        };

        Line::from(Span::styled(line.to_string(), style))
    }
}

pub struct EmptyDiffWidget;

impl EmptyDiffWidget {
    pub fn render(frame: &mut Frame, area: Rect) {
        let paragraph = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No changes to display",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .title(" DIFF ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );

        frame.render_widget(paragraph, area);
    }
}
