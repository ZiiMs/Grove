use ratatui::style::Color;

pub const COLOR_PALETTE: &[(&str, Color)] = &[
    ("Gray", Color::Gray),
    ("Dark Gray", Color::DarkGray),
    ("Red", Color::Red),
    ("Light Red", Color::LightRed),
    ("Green", Color::Green),
    ("Light Green", Color::LightGreen),
    ("Yellow", Color::Yellow),
    ("Light Yellow", Color::LightYellow),
    ("Blue", Color::Blue),
    ("Light Blue", Color::LightBlue),
    ("Magenta", Color::Magenta),
    ("Light Magenta", Color::LightMagenta),
    ("Cyan", Color::Cyan),
    ("Light Cyan", Color::LightCyan),
    ("White", Color::White),
    ("Black", Color::Black),
];

pub const ICON_PRESETS: &[(&str, &str)] = &[
    ("○ Empty circle", "○"),
    ("● Filled circle", "●"),
    ("◐ Half circle", "◐"),
    ("◑ Three-quarter", "◑"),
    ("◉ Bullseye", "◉"),
    ("✓ Check", "✓"),
    ("✗ Cross", "✗"),
    ("⊘ Cancelled", "⊘"),
    ("⚡ Lightning", "⚡"),
    ("★ Star", "★"),
    ("☆ Empty star", "☆"),
    ("► Arrow", "►"),
    ("▷ Empty arrow", "▷"),
    ("■ Square", "■"),
    ("□ Empty square", "□"),
    ("◆ Diamond", "◆"),
    ("◇ Empty diamond", "◇"),
    ("▶ Play", "▶"),
    ("⏸ Pause", "⏸"),
    ("⏹ Stop", "⏹"),
];

pub fn parse_color(name: &str) -> Color {
    let lower = name.to_lowercase().replace(['_', '-'], " ");
    for (label, color) in COLOR_PALETTE {
        if label.to_lowercase().replace(' ', "") == lower.replace(' ', "") {
            return *color;
        }
    }
    Color::Gray
}

pub fn color_to_string(color: Color) -> &'static str {
    match color {
        Color::Gray => "gray",
        Color::DarkGray => "dark_gray",
        Color::Red => "red",
        Color::LightRed => "light_red",
        Color::Green => "green",
        Color::LightGreen => "light_green",
        Color::Yellow => "yellow",
        Color::LightYellow => "light_yellow",
        Color::Blue => "blue",
        Color::LightBlue => "light_blue",
        Color::Magenta => "magenta",
        Color::LightMagenta => "light_magenta",
        Color::Cyan => "cyan",
        Color::LightCyan => "light_cyan",
        Color::White => "white",
        Color::Black => "black",
        _ => "gray",
    }
}

pub fn color_display_name(color_name: &str) -> &str {
    for (label, c) in COLOR_PALETTE {
        if color_to_string(*c) == color_name {
            return label;
        }
    }
    color_name
}

pub fn get_color_by_index(index: usize) -> Option<Color> {
    COLOR_PALETTE.get(index).map(|(_, c)| *c)
}

pub fn get_color_name_by_index(index: usize) -> Option<&'static str> {
    COLOR_PALETTE.get(index).map(|(name, _)| *name)
}

pub fn get_icon_by_index(index: usize) -> Option<&'static str> {
    ICON_PRESETS.get(index).map(|(_, icon)| *icon)
}

pub fn find_icon_index(icon: &str) -> usize {
    ICON_PRESETS
        .iter()
        .position(|(_, i)| *i == icon)
        .unwrap_or(0)
}

pub fn find_color_index(color_name: &str) -> usize {
    COLOR_PALETTE
        .iter()
        .position(|(name, color)| color_to_string(*color) == color_name || *name == color_name)
        .unwrap_or(0)
}
