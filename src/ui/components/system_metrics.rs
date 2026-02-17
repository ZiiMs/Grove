use std::collections::VecDeque;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Unicode bar characters for sparkline rendering (bottom-aligned)
const BARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub struct SystemMetricsWidget<'a> {
    cpu_history: &'a VecDeque<f32>,
    memory_history: &'a VecDeque<f32>,
    memory_used: u64,
    memory_total: u64,
}

impl<'a> SystemMetricsWidget<'a> {
    pub fn new(
        cpu_history: &'a VecDeque<f32>,
        memory_history: &'a VecDeque<f32>,
        memory_used: u64,
        memory_total: u64,
    ) -> Self {
        Self {
            cpu_history,
            memory_history,
            memory_used,
            memory_total,
        }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        // Split area into two equal halves for CPU and Memory
        let chunks = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        self.render_cpu_graph(frame, chunks[0]);
        self.render_memory_graph(frame, chunks[1]);
    }

    fn render_cpu_graph(&self, frame: &mut Frame, area: Rect) {
        let current_cpu = self.cpu_history.back().copied().unwrap_or(0.0);

        // Determine color based on CPU usage
        let color = if current_cpu >= 80.0 {
            Color::Red
        } else if current_cpu >= 50.0 {
            Color::Yellow
        } else {
            Color::Green
        };

        let title = format!(" CPU {:5.1}% ", current_cpu);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Render the sparkline graph
        self.render_graph(frame, inner, self.cpu_history, color);
    }

    fn render_memory_graph(&self, frame: &mut Frame, area: Rect) {
        let current_mem_pct = self.memory_history.back().copied().unwrap_or(0.0);

        // Format memory as human-readable
        let mem_used_gb = self.memory_used as f64 / (1024.0 * 1024.0 * 1024.0);
        let mem_total_gb = self.memory_total as f64 / (1024.0 * 1024.0 * 1024.0);

        // Determine color based on memory usage
        let color = if current_mem_pct >= 80.0 {
            Color::Red
        } else if current_mem_pct >= 60.0 {
            Color::Yellow
        } else {
            Color::Cyan
        };

        let title = format!(
            " MEM {:.1}/{:.1}GB ({:.0}%) ",
            mem_used_gb, mem_total_gb, current_mem_pct
        );
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Render the sparkline graph
        self.render_graph(frame, inner, self.memory_history, color);
    }

    fn render_graph(&self, frame: &mut Frame, area: Rect, data: &VecDeque<f32>, color: Color) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let width = area.width as usize;
        let height = area.height as usize;

        // Take the last `width` samples (or pad with zeros if not enough)
        let samples: Vec<f32> = if data.len() >= width {
            data.iter().skip(data.len() - width).copied().collect()
        } else {
            let mut padded = vec![0.0; width - data.len()];
            padded.extend(data.iter().copied());
            padded
        };

        // Build the graph line by line (from top to bottom)
        // Each cell can show one of 8 bar heights
        let mut lines: Vec<Line> = Vec::with_capacity(height);

        for row in 0..height {
            // Calculate the threshold for this row (top row = highest values)
            // row 0 = top = values 87.5-100%
            // row height-1 = bottom = values 0-12.5%
            let row_from_bottom = height - 1 - row;
            let threshold_low = (row_from_bottom as f32 / height as f32) * 100.0;
            let threshold_high = ((row_from_bottom + 1) as f32 / height as f32) * 100.0;

            let spans: Vec<Span> = samples
                .iter()
                .map(|&val| {
                    let bar_char = if val >= threshold_high {
                        // Full bar for this cell
                        BARS[8]
                    } else if val > threshold_low {
                        // Partial bar - calculate which character to use
                        let fraction = (val - threshold_low) / (threshold_high - threshold_low);
                        let bar_index = (fraction * 8.0).round() as usize;
                        BARS[bar_index.min(8)]
                    } else {
                        // Empty
                        ' '
                    };
                    Span::styled(bar_char.to_string(), Style::default().fg(color))
                })
                .collect();

            lines.push(Line::from(spans));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }
}
