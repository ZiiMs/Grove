use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::config::ProjectMgmtProvider;
use crate::app::state::{PmStatusDebugState, PmStatusDebugStep};
use crate::ui::helpers::centered_rect;

pub struct PmStatusDebugOverlay<'a> {
    state: &'a PmStatusDebugState,
    configured_providers: &'a [ProjectMgmtProvider],
}

impl<'a> PmStatusDebugOverlay<'a> {
    pub fn new(
        state: &'a PmStatusDebugState,
        configured_providers: &'a [ProjectMgmtProvider],
    ) -> Self {
        Self {
            state,
            configured_providers,
        }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        match self.state.step {
            PmStatusDebugStep::SelectProvider => self.render_provider_selection(frame, area),
            PmStatusDebugStep::ShowPayload => self.render_payload(frame, area),
        }
    }

    fn render_provider_selection(&self, frame: &mut Frame, area: Rect) {
        let popup_area = centered_rect(50, 50, area);
        frame.render_widget(Clear, popup_area);

        let providers = ProjectMgmtProvider::all();
        let mut lines = vec![
            Line::from(Span::styled(
                "PM Status Debug",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Select a provider to fetch statuses:",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
        ];

        for (i, provider) in providers.iter().enumerate() {
            let is_selected = i == self.state.selected_index;
            let is_configured = self.configured_providers.contains(provider);

            let selector = if is_selected { "▶ " } else { "  " };
            let name = provider.display_name();
            let status = if is_configured {
                Span::styled("(configured)", Style::default().fg(Color::Green))
            } else {
                Span::styled("(not configured)", Style::default().fg(Color::DarkGray))
            };

            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            lines.push(Line::from(vec![
                Span::styled(selector, style),
                Span::styled(name, style),
                Span::raw("  "),
                status,
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "j/k: navigate  Enter: fetch  Esc: close",
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .title(" Debug ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        );

        frame.render_widget(paragraph, popup_area);
    }

    fn render_payload(&self, frame: &mut Frame, area: Rect) {
        let popup_area = centered_rect(70, 70, area);
        frame.render_widget(Clear, popup_area);

        let mut lines = Vec::new();

        if self.state.loading {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Loading...",
                Style::default().fg(Color::Yellow),
            )));
        } else if let Some(ref error) = self.state.error {
            let provider_name = self
                .state
                .selected_provider
                .map(|p| p.display_name())
                .unwrap_or("Provider");

            lines.push(Line::from(Span::styled(
                format!("{} Error", provider_name),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                error.clone(),
                Style::default().fg(Color::Red),
            )));
        } else if let Some(ref payload) = self.state.payload {
            let provider_name = self
                .state
                .selected_provider
                .map(|p| p.display_name())
                .unwrap_or("Provider");

            lines.push(Line::from(Span::styled(
                format!("{} Status Payload", provider_name),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            for line in payload.lines() {
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::White),
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "c: copy  Esc: close",
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Debug Result ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, popup_area);
    }
}
