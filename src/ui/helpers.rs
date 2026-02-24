use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub const STYLE_LABEL: Style = Style::new().fg(Color::White);
pub const STYLE_LABEL_SELECTED: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
pub const STYLE_VALUE: Style = Style::new().fg(Color::Gray);
pub const STYLE_VALUE_SELECTED: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub const STYLE_TOGGLE: Style = Style::new().fg(Color::Green);
pub const STYLE_TOGGLE_SELECTED: Style = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
pub const STYLE_SEPARATOR: Style = Style::new().fg(Color::DarkGray);
pub const STYLE_INDENT: Style = Style::new();
pub const STYLE_OK: Style = Style::new().fg(Color::Green);
pub const STYLE_ERROR: Style = Style::new().fg(Color::Red);
pub const STYLE_CURSOR: Style = Style::new().fg(Color::White);
pub const STYLE_FOOTER: Style = Style::new().fg(Color::DarkGray);

#[derive(Debug, Clone, Copy, Default)]
pub enum CursorType {
    #[default]
    None,
    Arrow,
    Dropdown,
    Edit,
}

#[derive(Debug, Clone)]
pub struct FieldLineOptions {
    pub label_width: usize,
    pub value_width: Option<usize>,
    pub cursor: CursorType,
    pub is_toggle: bool,
    pub is_editing: bool,
}

impl Default for FieldLineOptions {
    fn default() -> Self {
        Self {
            label_width: 12,
            value_width: None,
            cursor: CursorType::None,
            is_toggle: false,
            is_editing: false,
        }
    }
}

impl FieldLineOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn label_width(mut self, width: usize) -> Self {
        self.label_width = width;
        self
    }

    pub fn value_width(mut self, width: usize) -> Self {
        self.value_width = Some(width);
        self
    }

    pub fn cursor(mut self, cursor: CursorType) -> Self {
        self.cursor = cursor;
        self
    }

    pub fn is_toggle(mut self, is_toggle: bool) -> Self {
        self.is_toggle = is_toggle;
        self
    }

    pub fn is_editing(mut self, is_editing: bool) -> Self {
        self.is_editing = is_editing;
        self
    }
}

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

pub fn token_status(exists: bool) -> (&'static str, Color) {
    if exists {
        ("✓ OK", Color::Green)
    } else {
        ("✗ Missing", Color::Red)
    }
}

pub fn token_status_line(name: &str, exists: bool) -> Line<'static> {
    let (symbol, color) = token_status(exists);
    Line::from(vec![
        Span::styled("    ", STYLE_INDENT),
        Span::styled(format!("{:14}", "Token"), STYLE_SEPARATOR),
        Span::styled(": ", STYLE_SEPARATOR),
        Span::styled(
            format!("{:34}", format!("{} ({})", symbol, name)),
            Style::default().fg(color),
        ),
    ])
}

pub fn render_field_line(
    label: &str,
    value: &str,
    is_selected: bool,
    options: FieldLineOptions,
) -> Line<'static> {
    let label_style = if is_selected {
        STYLE_LABEL_SELECTED
    } else {
        STYLE_LABEL
    };

    let value_style = if options.is_toggle {
        if is_selected {
            STYLE_TOGGLE_SELECTED
        } else {
            STYLE_TOGGLE
        }
    } else if is_selected {
        STYLE_VALUE_SELECTED
    } else {
        STYLE_VALUE
    };

    let cursor = match options.cursor {
        CursorType::None => "",
        CursorType::Arrow => " ◀",
        CursorType::Dropdown => " ▼",
        CursorType::Edit => "█",
    };

    let display_cursor = if is_selected { cursor } else { "" };

    let display_value = if let Some(width) = options.value_width {
        if value.len() > width {
            format!(
                "{:width$}",
                format!("{}...", &value[..width.saturating_sub(3)]),
                width = width
            )
        } else {
            format!("{:width$}", value, width = width)
        }
    } else {
        value.to_string()
    };

    Line::from(vec![
        Span::styled("    ", STYLE_INDENT),
        Span::styled(
            format!("{:width$}", label, width = options.label_width),
            label_style,
        ),
        Span::styled(": ", STYLE_SEPARATOR),
        Span::styled(display_value, value_style),
        Span::styled(display_cursor.to_string(), STYLE_CURSOR),
    ])
}
