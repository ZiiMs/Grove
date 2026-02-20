use ansi_to_tui::IntoText;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
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

        // Parse ANSI content into styled Text
        let text = match processed_content.as_bytes().into_text() {
            Ok(text) => text,
            Err(_) => Text::raw(processed_content),
        };

        // Compress the output: strip trailing blank lines, collapse consecutive blanks
        let lines = compress_lines(text.lines);

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

/// Strip trailing blank lines and collapse runs of 2+ consecutive blank lines into 1.
fn compress_lines(lines: Vec<Line<'_>>) -> Vec<Line<'_>> {
    // Strip trailing blank lines
    let mut end = lines.len();
    while end > 0 && is_blank_line(&lines[end - 1]) {
        end -= 1;
    }
    let lines = &lines[..end];

    // Collapse consecutive blank lines (keep at most 1)
    let mut result: Vec<Line> = Vec::with_capacity(lines.len());
    let mut prev_blank = false;

    for line in lines {
        let blank = is_blank_line(line);
        if blank && prev_blank {
            continue; // Skip consecutive blank lines
        }
        prev_blank = blank;
        result.push(line.clone());
    }

    result
}

/// Check if a Line is visually blank (empty or only whitespace).
fn is_blank_line(line: &Line<'_>) -> bool {
    if line.spans.is_empty() {
        return true;
    }
    line.spans.iter().all(|span| span.content.trim().is_empty())
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
