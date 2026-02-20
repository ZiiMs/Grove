use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_selected: bool,
}

pub struct FileBrowserWidget<'a> {
    entries: &'a [DirEntry],
    selected_index: usize,
    selected_files: &'a HashSet<PathBuf>,
    current_path: &'a Path,
    repo_path: &'a Path,
}

impl<'a> FileBrowserWidget<'a> {
    pub fn new(
        entries: &'a [DirEntry],
        selected_index: usize,
        selected_files: &'a HashSet<PathBuf>,
        current_path: &'a Path,
        repo_path: &'a Path,
    ) -> Self {
        Self {
            entries,
            selected_index,
            selected_files,
            current_path,
            repo_path,
        }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(80, 80, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" SELECT FILES TO SYMLINK ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(inner);

        self.render_file_list(frame, chunks[0]);
        self.render_selected_list(frame, chunks[1]);
        self.render_footer(frame, inner);
    }

    fn render_file_list(&self, frame: &mut Frame, area: Rect) {
        let path_display = if self.current_path == self.repo_path {
            "/".to_string()
        } else {
            self.current_path
                .strip_prefix(self.repo_path)
                .unwrap_or(self.current_path)
                .to_string_lossy()
                .to_string()
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled("Path: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                path_display,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        let list_items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let is_selected = i == self.selected_index;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if entry.is_dir {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default().fg(Color::White)
                };

                let mark = if entry.is_selected { "[x]" } else { "[ ]" };
                let mark_style = if entry.is_selected {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let suffix = if entry.is_dir { "/" } else { "" };

                ListItem::new(Line::from(vec![
                    Span::styled(mark, mark_style),
                    Span::styled(" ", Style::default()),
                    Span::styled(format!("{}{}", entry.name, suffix), style),
                ]))
            })
            .collect();

        let list = List::new(list_items).block(
            Block::default()
                .title(" Files ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);

        frame.render_widget(header, chunks[0]);
        frame.render_widget(list, chunks[1]);
    }

    fn render_selected_list(&self, frame: &mut Frame, area: Rect) {
        let mut sorted_files: Vec<_> = self.selected_files.iter().collect();
        sorted_files.sort();

        let items: Vec<ListItem> = sorted_files
            .iter()
            .map(|p| {
                let name = p.to_string_lossy();
                ListItem::new(Line::from(vec![
                    Span::styled("[x] ", Style::default().fg(Color::Green)),
                    Span::styled(name.to_string(), Style::default().fg(Color::White)),
                ]))
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .title(" Selected ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(list, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = "[↑/↓] Navigate  [Space/Enter] Toggle  [→] Enter dir  [←] Parent  [Esc] Done";
        let footer = Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(ratatui::layout::Alignment::Center);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(area);

        if let Some(footer_area) = chunks.last() {
            frame.render_widget(footer, *footer_area);
        }
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

pub fn load_directory_entries(
    path: &Path,
    selected_files: &HashSet<PathBuf>,
    repo_path: &Path,
) -> Vec<DirEntry> {
    let mut entries = Vec::new();

    if path == repo_path {
    } else if let Some(parent) = path.parent() {
        entries.push(DirEntry {
            name: "..".to_string(),
            path: parent.to_path_buf(),
            is_dir: true,
            is_selected: false,
        });
    }

    if let Ok(read_dir) = std::fs::read_dir(path) {
        let mut items: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
        items.sort_by(|a, b| {
            let a_is_dir = a.path().is_dir();
            let b_is_dir = b.path().is_dir();
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().cmp(&b.file_name()),
            }
        });

        for entry in items {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let file_path = entry.path();

            if file_name.starts_with('.') && file_name != ".env" {
                continue;
            }

            let is_dir = file_path.is_dir();
            let is_selected = selected_files.contains(&file_path);

            entries.push(DirEntry {
                name: file_name,
                path: file_path,
                is_dir,
                is_selected,
            });
        }
    }

    entries
}
