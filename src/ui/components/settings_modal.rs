use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{
    AiAgent, ConfigLogLevel, GitProvider, SettingsField, SettingsSection, SettingsState,
};

pub struct SettingsModal<'a> {
    state: &'a SettingsState,
    ai_agent: &'a AiAgent,
    git_provider: &'a GitProvider,
    log_level: &'a ConfigLogLevel,
}

impl<'a> SettingsModal<'a> {
    pub fn new(
        state: &'a SettingsState,
        ai_agent: &'a AiAgent,
        git_provider: &'a GitProvider,
        log_level: &'a ConfigLogLevel,
    ) -> Self {
        Self {
            state,
            ai_agent,
            git_provider,
            log_level,
        }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(60, 60, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" SETTINGS ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(8),
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Min(2),
            ])
            .split(inner);

        self.render_section_header(frame, chunks[0], SettingsSection::Global);
        self.render_global_fields(frame, chunks[1]);
        self.render_section_header(frame, chunks[2], SettingsSection::Project);
        self.render_project_fields(frame, chunks[3]);
        self.render_footer(frame, chunks[4]);

        if let crate::app::DropdownState::Open { selected_index } = self.state.dropdown {
            self.render_dropdown(frame, selected_index);
        }
    }

    fn render_section_header(&self, frame: &mut Frame, area: Rect, section: SettingsSection) {
        let is_active = self.state.section == section;
        let style = if is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let title = match section {
            SettingsSection::Global => "GLOBAL",
            SettingsSection::Project => "PROJECT",
        };

        let paragraph =
            Paragraph::new(Line::from(Span::styled(title, style))).alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

    fn render_global_fields(&self, frame: &mut Frame, area: Rect) {
        let fields = [
            ("AI Agent", self.ai_agent.display_name()),
            ("Git Provider", self.git_provider.display_name()),
            ("Log Level", self.log_level.display_name()),
        ];

        let lines: Vec<Line> = fields
            .iter()
            .enumerate()
            .map(|(i, (label, value))| {
                let is_selected =
                    self.state.section == SettingsSection::Global && self.state.field_index == i;
                self.render_field_line(label, value, is_selected)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_project_fields(&self, frame: &mut Frame, area: Rect) {
        let fields = [
            (
                "Branch Prefix",
                self.state.project_config.branch_prefix.as_str(),
            ),
            (
                "Main Branch",
                self.state.project_config.main_branch.as_str(),
            ),
        ];

        let lines: Vec<Line> = fields
            .iter()
            .enumerate()
            .map(|(i, (label, value))| {
                let is_selected =
                    self.state.section == SettingsSection::Project && self.state.field_index == i;
                let display_value = if self.state.editing_text && is_selected {
                    &self.state.text_buffer
                } else {
                    *value
                };
                self.render_field_line(label, display_value, is_selected)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_field_line(&self, label: &str, value: &str, is_selected: bool) -> Line<'static> {
        let label_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let value_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        let cursor = if is_selected && self.state.editing_text {
            "█"
        } else if is_selected {
            " ◀"
        } else {
            ""
        };

        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(format!("{:14}", label), label_style),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:20}", value), value_style),
            Span::styled(cursor.to_string(), Style::default().fg(Color::White)),
        ])
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.state.editing_text {
            "[Enter] Save  [Esc] Cancel"
        } else if self.state.is_dropdown_open() {
            "[↑/↓] Navigate  [Enter] Select  [Esc] Cancel"
        } else {
            "[Tab] Switch section  [Enter] Edit  [↑/↓] Navigate  [Esc] Close  [q] Save & Close"
        };

        let paragraph = Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }

    fn render_dropdown(&self, frame: &mut Frame, selected_index: usize) {
        let field = self.state.current_field();
        let options: Vec<&str> = match field {
            SettingsField::AiAgent => AiAgent::all().iter().map(|a| a.display_name()).collect(),
            SettingsField::GitProvider => GitProvider::all()
                .iter()
                .map(|g| g.display_name())
                .collect(),
            SettingsField::LogLevel => ConfigLogLevel::all()
                .iter()
                .map(|l| l.display_name())
                .collect(),
            _ => return,
        };

        let area = get_dropdown_position(frame.area(), self.state.section, self.state.field_index);
        frame.render_widget(Clear, area);

        let lines: Vec<Line> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let style = if i == selected_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(format!(" {} ", opt), style))
            })
            .collect();

        let height = lines.len() as u16 + 2;
        let dropdown_area = Rect::new(area.x, area.y, area.width, height.min(area.height));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, dropdown_area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn get_dropdown_position(frame_area: Rect, section: SettingsSection, field_index: usize) -> Rect {
    let modal_area = centered_rect(60, 60, frame_area);

    let base_y = match section {
        SettingsSection::Global => modal_area.y + 4,
        SettingsSection::Project => modal_area.y + 14,
    };

    let row_offset = field_index as u16;

    Rect::new(modal_area.x + 20, base_y + row_offset, 20, 10)
}
