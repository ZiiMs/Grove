# Dev Server Feature Implementation Plan

## Overview

Add a Dev Server system to the Flock TUI that allows users to start, stop, and monitor development servers for their agent worktrees. The dev server logs will be displayed in a new tab alongside the preview pane.

---

## Key Design Decisions

| Decision | Choice | Implication |
|----------|--------|-------------|
| Agent switch behavior | Keep running | Store dev servers per-agent, only switch displayed logs |
| TUI exit behavior | Always stop | Kill all dev servers on quit |
| Concurrent servers | Multiple allowed | `HashMap<Uuid, DevServer>` to track all, with warning on conflict |

---

## Configuration

### Add to `src/app/config.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevServerConfig {
    /// Command to run (e.g., "npm run dev", "cargo run")
    pub command: Option<String>,
    /// Commands to run before the main command
    #[serde(default)]
    pub run_before: Vec<String>,
    /// Working directory relative to worktree root (empty = worktree root)
    #[serde(default)]
    pub working_dir: String,
    /// Port the dev server listens on (for preview URL)
    pub port: Option<u16>,
    /// Auto-start when agent is created/resumed
    #[serde(default)]
    pub auto_start: bool,
}

impl Default for DevServerConfig {
    fn default() -> Self {
        Self {
            command: None,
            run_before: Vec::new(),
            working_dir: String::new(),
            port: None,
            auto_start: false,
        }
    }
}
```

Add to `RepoConfig`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoConfig {
    #[serde(default)]
    pub git: RepoGitConfig,
    #[serde(default)]
    pub asana: RepoAsanaConfig,
    #[serde(default)]
    pub dev_server: DevServerConfig,  // NEW
}
```

### Example `.flock/project.toml`

```toml
[dev_server]
command = "npm run dev"
run_before = ["npm install"]
working_dir = ""
port = 3000
auto_start = false
```

---

## Module Structure

### New Module: `src/devserver/`

```
src/devserver/
├── mod.rs           # Module exports
├── process.rs       # DevServer struct (single server instance)
└── manager.rs       # DevServerManager (multi-server orchestration)
```

### `src/devserver/mod.rs`

```rust
pub mod manager;
pub mod process;

pub use manager::DevServerManager;
pub use process::{DevServer, DevServerStatus};
```

### `src/devserver/process.rs`

```rust
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::path::Path;
use tokio::process::Child;

const MAX_LOG_LINES: usize = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevServerStatus {
    Stopped,
    Starting,
    Running { pid: u32, port: Option<u16> },
    Stopping,
    Failed(String),
}

impl DevServerStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            DevServerStatus::Stopped => "○",
            DevServerStatus::Starting => "◐",
            DevServerStatus::Running { .. } => "●",
            DevServerStatus::Stopping => "◑",
            DevServerStatus::Failed(_) => "✗",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            DevServerStatus::Stopped => "Stopped",
            DevServerStatus::Starting => "Starting",
            DevServerStatus::Running { .. } => "Running",
            DevServerStatus::Stopping => "Stopping",
            DevServerStatus::Failed(_) => "Failed",
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, DevServerStatus::Running { .. })
    }
}

pub struct DevServer {
    status: DevServerStatus,
    logs: VecDeque<String>,
    process: Option<Child>,
}

impl DevServer {
    pub fn new() -> Self {
        Self {
            status: DevServerStatus::Stopped,
            logs: VecDeque::with_capacity(MAX_LOG_LINES),
            process: None,
        }
    }

    /// Start the dev server with given config
    pub async fn start(
        &mut self,
        config: &DevServerConfig,
        worktree_path: &Path,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> Result<()> {
        // 1. Check if already running
        if self.status.is_running() {
            anyhow::bail!("Dev server is already running");
        }

        // 2. Resolve working directory
        let working_dir = if config.working_dir.is_empty() {
            worktree_path.to_path_buf()
        } else {
            worktree_path.join(&config.working_dir)
        };

        // 3. Execute run_before commands
        for cmd in &config.run_before {
            self.run_before_command(cmd, &working_dir).await?;
        }

        // 4. Get the command to run
        let command = config.command.as_ref()
            .context("No dev server command configured")?;

        // 5. Spawn the process
        self.status = DevServerStatus::Starting;
        
        #[cfg(unix)]
        let child = self.spawn_unix(command, &working_dir)?;
        
        #[cfg(windows)]
        let child = self.spawn_windows(command, &working_dir)?;

        let pid = child.id().context("Failed to get process ID")?;
        
        // 6. Start log streaming task
        self.spawn_log_streamer(child, action_tx.clone());

        // 7. Update status
        self.status = DevServerStatus::Running {
            pid,
            port: config.port,
        };

        Ok(())
    }

    /// Stop the dev server (kills process tree)
    pub async fn stop(&mut self) -> Result<()> {
        if !self.status.is_running() {
            return Ok(());
        }

        self.status = DevServerStatus::Stopping;

        if let Some(mut child) = self.process.take() {
            #[cfg(unix)]
            self.kill_process_tree_unix(&child)?;
            
            #[cfg(windows)]
            self.kill_process_tree_windows(&child)?;

            let _ = child.kill().await;
        }

        self.status = DevServerStatus::Stopped;
        Ok(())
    }

    /// Restart the dev server
    pub async fn restart(
        &mut self,
        config: &DevServerConfig,
        worktree_path: &Path,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> Result<()> {
        self.stop().await?;
        self.start(config, worktree_path, action_tx).await
    }

    /// Get current status
    pub fn status(&self) -> &DevServerStatus {
        &self.status
    }

    /// Get log buffer
    pub fn logs(&self) -> &[String] {
        self.logs.as_slices().0
    }

    /// Append a log line
    pub fn append_log(&mut self, line: String) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(line);
    }

    /// Clear logs
    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    /// Check if process is still alive
    pub fn is_alive(&mut self) -> bool {
        if let Some(child) = &mut self.process {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    // Platform-specific implementations...
    
    #[cfg(unix)]
    fn spawn_unix(&mut self, command: &str, working_dir: &Path) -> Result<Child> {
        use std::process::Stdio;
        use tokio::process::Command;

        let child = Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0) // New process group for clean kill
            .spawn()
            .context("Failed to spawn dev server")?;

        Ok(child)
    }

    #[cfg(unix)]
    fn kill_process_tree_unix(&self, child: &Child) -> Result<()> {
        if let Some(pid) = child.id() {
            // Kill the entire process group
            let _ = std::process::Command::new("kill")
                .arg(format!("-{}", pid)) // Negative PID = process group
                .output();
        }
        Ok(())
    }

    #[cfg(windows)]
    fn spawn_windows(&mut self, command: &str, working_dir: &Path) -> Result<Child> {
        use std::process::Stdio;
        use tokio::process::Command;

        let child = Command::new("cmd")
            .arg("/C")
            .arg(command)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn dev server")?;

        Ok(child)
    }

    #[cfg(windows)]
    fn kill_process_tree_windows(&self, child: &Child) -> Result<()> {
        if let Some(pid) = child.id() {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .output();
        }
        Ok(())
    }
}
```

### `src/devserver/manager.rs`

```rust
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use super::process::{DevServer, DevServerStatus};
use crate::app::{Action, DevServerConfig};

pub struct DevServerManager {
    /// All dev servers keyed by agent ID
    servers: HashMap<Uuid, DevServer>,
    /// Action channel for status updates
    action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
}

impl DevServerManager {
    pub fn new(action_tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Self {
        Self {
            servers: HashMap::new(),
            action_tx,
        }
    }

    /// Check if any dev server is currently running
    pub fn has_running_server(&self) -> bool {
        self.servers.values().any(|s| s.status().is_running())
    }

    /// Get list of running servers (for warning display)
    pub fn running_servers(&self) -> Vec<(Uuid, String, Option<u16>)> {
        self.servers
            .iter()
            .filter_map(|(id, server)| {
                if let DevServerStatus::Running { port, .. } = server.status() {
                    Some((*id, server.agent_name.clone(), *port))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Start dev server for specific agent
    pub async fn start(
        &mut self,
        agent_id: Uuid,
        agent_name: String,
        config: &DevServerConfig,
        worktree: &Path,
    ) -> Result<()> {
        let server = self.servers.entry(agent_id).or_insert_with(DevServer::new);
        server.agent_name = agent_name;
        server.start(config, worktree, self.action_tx.clone()).await
    }

    /// Stop dev server for specific agent
    pub async fn stop(&mut self, agent_id: Uuid) -> Result<()> {
        if let Some(server) = self.servers.get_mut(&agent_id) {
            server.stop().await?;
        }
        Ok(())
    }

    /// Stop ALL dev servers (for quit)
    pub async fn stop_all(&mut self) -> Result<()> {
        for server in self.servers.values_mut() {
            let _ = server.stop().await;
        }
        Ok(())
    }

    /// Get dev server for display
    pub fn get(&self, agent_id: Uuid) -> Option<&DevServer> {
        self.servers.get(&agent_id)
    }

    /// Get mutable dev server
    pub fn get_mut(&mut self, agent_id: Uuid) -> Option<&mut DevServer> {
        self.servers.get_mut(&agent_id)
    }

    /// Remove dev server entry (when agent is deleted)
    pub fn remove(&mut self, agent_id: Uuid) {
        self.servers.remove(&agent_id);
    }
}
```

---

## State Management

### Add to `src/app/state.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewTab {
    #[default]
    Preview,
    DevServer,
}

pub struct AppState {
    // ... existing fields ...

    /// Current tab in the preview pane
    pub preview_tab: PreviewTab,
    
    /// Dev server log scroll position
    pub devserver_scroll: usize,
    
    /// Warning modal state for dev server conflicts
    pub devserver_warning: Option<DevServerWarning>,
}

#[derive(Debug, Clone)]
pub struct DevServerWarning {
    /// Which agent would start the new server
    pub agent_id: Uuid,
    /// List of currently running servers (agent_name, port)
    pub running_servers: Vec<(String, Option<u16>)>,
}
```

---

## Actions

### Add to `src/app/action.rs`

```rust
pub enum Action {
    // ... existing actions ...

    // Dev Server
    RequestStartDevServer,              // User pressed 'D' - check for conflicts first
    ConfirmStartDevServer,              // User confirmed after warning
    StartDevServer,                     // Internal action to actually start
    StopDevServer,
    RestartDevServer,
    
    // Preview pane navigation
    NextPreviewTab,                     // Tab key
    PrevPreviewTab,                     // Shift+Tab
    
    // Dev server controls
    ClearDevServerLogs,
    OpenDevServerInBrowser,
    
    // Dismiss warning
    DismissDevServerWarning,
    
    // Background updates (from log streamer)
    AppendDevServerLog {
        agent_id: Uuid,
        line: String,
    },
    UpdateDevServerStatus {
        agent_id: Uuid,
        status: DevServerStatus,
    },
}
```

---

## UI Components

### Preview Pane with Tabs

```
┌─────────────────────────────────────────┐
│ [Preview] [Dev Server ●]                │  ← Tab bar (● = running)
├─────────────────────────────────────────┤
│ Dev Server: feature-auth (Running)      │  ← Status line
│ Port: 3000                              │
├─────────────────────────────────────────┤
│ $ npm run dev                           │
│ > my-app@1.0.0 dev                      │
│ > vite                                  │
│                                         │
│   VITE v5.0.0  ready in 234 ms          │
│                                         │
│   ➜  Local:   http://localhost:3000/    │
│   ➜  Network: use --host to expose      │
│                                         │
└─────────────────────────────────────────┘
```

### New File: `src/ui/components/devserver_view.rs`

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::devserver::DevServerStatus;

pub struct DevServerViewWidget<'a> {
    status: &'a DevServerStatus,
    logs: &'a [String],
    agent_name: &'a str,
    scroll: usize,
}

impl<'a> DevServerViewWidget<'a> {
    pub fn new(
        status: &'a DevServerStatus,
        logs: &'a [String],
        agent_name: &'a str,
    ) -> Self {
        Self {
            status,
            logs,
            agent_name,
            scroll: 0,
        }
    }

    pub fn with_scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        let visible_height = area.height.saturating_sub(4) as usize;

        // Status line
        let status_line = self.render_status_line();

        // Log lines
        let total_lines = self.logs.len();
        let start = if total_lines > visible_height {
            total_lines.saturating_sub(visible_height).saturating_sub(self.scroll)
        } else {
            0
        };

        let log_lines: Vec<Line> = self.logs
            .iter()
            .skip(start)
            .take(visible_height)
            .map(|line| Line::from(Span::raw(line.clone())))
            .collect();

        let mut lines = vec![status_line, Line::from("")];
        lines.extend(log_lines);

        let border_color = match self.status {
            DevServerStatus::Running { .. } => Color::Green,
            DevServerStatus::Starting | DevServerStatus::Stopping => Color::Yellow,
            DevServerStatus::Failed(_) => Color::Red,
            DevServerStatus::Stopped => Color::DarkGray,
        };

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .title(format!(" DEV SERVER: {} ", self.agent_name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_status_line(&self) -> Line<'static> {
        let (status_text, status_color) = match self.status {
            DevServerStatus::Stopped => ("Stopped", Color::DarkGray),
            DevServerStatus::Starting => ("Starting...", Color::Yellow),
            DevServerStatus::Running { port, .. } => {
                if let Some(p) = port {
                    (format!("Running on port {}", p).as_str(), Color::Green)
                } else {
                    ("Running", Color::Green)
                }
            }
            DevServerStatus::Stopping => ("Stopping...", Color::Yellow),
            DevServerStatus::Failed(msg) => (msg.as_str(), Color::Red),
        };

        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                status_text,
                Style::default().fg(status_color).add_modifier(Modifier::BOLD),
            ),
        ])
    }
}

pub struct EmptyDevServerWidget;

impl EmptyDevServerWidget {
    pub fn render(frame: &mut Frame, area: Rect) {
        let paragraph = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No dev server configured",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'D' to start",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Configure in Settings (S) → Dev Server",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .title(" DEV SERVER ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(paragraph, area);
    }
}
```

### New File: `src/ui/components/devserver_warning.rs`

```rust
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::DevServerWarning;

pub struct DevServerWarningModal<'a> {
    warning: &'a DevServerWarning,
}

impl<'a> DevServerWarningModal<'a> {
    pub fn new(warning: &'a DevServerWarning) -> Self {
        Self { warning }
    }

    pub fn render(self, frame: &mut Frame) {
        let area = centered_rect(60, 50, frame.area());
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" ⚠ WARNING ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Another dev server is already running:",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
        ];

        for (name, port) in &self.warning.running_servers {
            let port_str = port.map(|p| p.to_string()).unwrap_or("?".to_string());
            lines.push(Line::from(Span::styled(
                format!("  • {} (port {})", name, port_str),
                Style::default().fg(Color::Yellow),
            )));
        }

        lines.extend(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Starting a new server may cause:",
                Style::default().fg(Color::White),
            )),
            Line::from(Span::styled(
                "  • Port conflicts",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  • Resource contention",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  • Unexpected behavior",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Start anyway?",
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[y] Yes    [n/Esc] No",
                Style::default().fg(Color::Cyan),
            )),
        ]);

        let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
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
```

### Modify `src/ui/app.rs`

Update `render_preview()` to include tab bar:

```rust
fn render_preview(&self, frame: &mut Frame, area: Rect) {
    // Split area into tab bar and content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Tab bar
            Constraint::Min(8),     // Content
        ])
        .split(area);

    self.render_preview_tabs(frame, chunks[0]);
    
    match self.state.preview_tab {
        PreviewTab::Preview => self.render_preview_content(frame, chunks[1]),
        PreviewTab::DevServer => self.render_devserver_content(frame, chunks[1]),
    }
}

fn render_preview_tabs(&self, frame: &mut Frame, area: Rect) {
    let preview_style = if self.state.preview_tab == PreviewTab::Preview {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let has_running = /* check if any dev server running */;
    let devserver_indicator = if has_running { " ●" } else { "" };
    
    let devserver_style = if self.state.preview_tab == PreviewTab::DevServer {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let tabs = Line::from(vec![
        Span::styled(" Preview ", preview_style),
        Span::raw(" "),
        Span::styled(format!(" Dev Server{} ", devserver_indicator), devserver_style),
    ]);

    let paragraph = Paragraph::new(tabs);
    frame.render_widget(paragraph, area);
}
```

---

## Key Bindings

| Key | Action | Context |
|-----|--------|---------|
| `Tab` | `NextPreviewTab` | Normal mode |
| `Shift+Tab` | `PrevPreviewTab` | Normal mode |
| `D` | `RequestStartDevServer` | When stopped (starts) or running (stops) |
| `Ctrl+D` | `RestartDevServer` | Always available |
| `C` | `ClearDevServerLogs` | Dev Server tab |
| `O` | `OpenDevServerInBrowser` | Dev Server tab, running |

### Add to `src/main.rs` key handler

```rust
// In handle_key_event()
KeyCode::Tab => Some(Action::NextPreviewTab),
KeyCode::BackTab => Some(Action::PrevPreviewTab),
KeyCode::Char('D') => Some(Action::RestartDevServer),
KeyCode::Char('d') => {
    // Toggle based on current state
    if let Some(agent) = state.selected_agent() {
        if /* dev server running for this agent */ {
            Some(Action::StopDevServer)
        } else {
            Some(Action::RequestStartDevServer)
        }
    } else {
        None
    }
}
KeyCode::Char('C') if state.preview_tab == PreviewTab::DevServer => {
    Some(Action::ClearDevServerLogs)
}
KeyCode::Char('O') if state.preview_tab == PreviewTab::DevServer => {
    Some(Action::OpenDevServerInBrowser)
}
```

---

## Action Handlers

### Add to `src/main.rs` in `process_action()`

```rust
// Dev Server Actions
Action::RequestStartDevServer => {
    if let Some(agent) = state.selected_agent() {
        if devserver_manager.has_running_server() {
            // Show warning modal
            state.devserver_warning = Some(DevServerWarning {
                agent_id: agent.id,
                running_servers: devserver_manager.running_servers()
                    .into_iter()
                    .map(|(_, name, port)| (name, port))
                    .collect(),
            });
        } else {
            // No conflict, start directly
            action_tx.send(Action::StartDevServer)?;
        }
    }
}

Action::ConfirmStartDevServer => {
    state.devserver_warning = None;
    action_tx.send(Action::StartDevServer)?;
}

Action::DismissDevServerWarning => {
    state.devserver_warning = None;
}

Action::StartDevServer => {
    if let Some(agent) = state.selected_agent() {
        let config = state.settings.repo_config.dev_server.clone();
        let worktree = agent.worktree_path.clone();
        let agent_id = agent.id;
        let agent_name = agent.name.clone();
        
        let manager = Arc::clone(&devserver_manager);
        tokio::spawn(async move {
            if let Err(e) = manager.start(agent_id, agent_name, &config, &PathBuf::from(worktree)).await {
                // Send error action
            }
        });
        
        state.log_info(format!("Starting dev server for '{}'", agent.name));
    }
}

Action::StopDevServer => {
    if let Some(agent) = state.selected_agent() {
        let manager = Arc::clone(&devserver_manager);
        let agent_id = agent.id;
        tokio::spawn(async move {
            let _ = manager.stop(agent_id).await;
        });
        state.log_info(format!("Stopping dev server for '{}'", agent.name));
    }
}

Action::RestartDevServer => {
    if let Some(agent) = state.selected_agent() {
        let config = state.settings.repo_config.dev_server.clone();
        let worktree = agent.worktree_path.clone();
        let agent_id = agent.id;
        let agent_name = agent.name.clone();
        
        let manager = Arc::clone(&devserver_manager);
        tokio::spawn(async move {
            let _ = manager.stop(agent_id).await;
            if let Err(e) = manager.start(agent_id, agent_name, &config, &PathBuf::from(worktree)).await {
                // Handle error
            }
        });
        state.log_info(format!("Restarting dev server for '{}'", agent.name));
    }
}

Action::NextPreviewTab => {
    state.preview_tab = match state.preview_tab {
        PreviewTab::Preview => PreviewTab::DevServer,
        PreviewTab::DevServer => PreviewTab::Preview,
    };
}

Action::PrevPreviewTab => {
    state.preview_tab = match state.preview_tab {
        PreviewTab::Preview => PreviewTab::DevServer,
        PreviewTab::DevServer => PreviewTab::Preview,
    };
}

Action::ClearDevServerLogs => {
    if let Some(agent) = state.selected_agent() {
        if let Some(server) = devserver_manager.get_mut(agent.id) {
            server.clear_logs();
        }
    }
}

Action::OpenDevServerInBrowser => {
    if let Some(agent) = state.selected_agent() {
        if let Some(server) = devserver_manager.get(agent.id) {
            if let DevServerStatus::Running { port, .. } = server.status() {
                if let Some(p) = port {
                    let url = format!("http://localhost:{}", p);
                    match open::that(&url) {
                        Ok(_) => state.log_info("Opening dev server in browser"),
                        Err(e) => state.log_error(format!("Failed to open browser: {}", e)),
                    }
                }
            }
        }
    }
}

Action::AppendDevServerLog { agent_id, line } => {
    if let Some(server) = devserver_manager.get_mut(agent_id) {
        server.append_log(line);
    }
}

// On quit - stop all dev servers
Action::Quit => {
    let _ = devserver_manager.stop_all().await;
    state.running = false;
    return Ok(true);
}
```

---

## Settings Integration

### Add to Settings Tab Enum (`src/app/state.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Git,
    ProjectMgmt,
    DevServer,  // NEW
}

impl SettingsTab {
    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::General,
            SettingsTab::Git,
            SettingsTab::ProjectMgmt,
            SettingsTab::DevServer,
        ]
    }

    pub fn next(&self) -> Self {
        match self {
            SettingsTab::General => SettingsTab::Git,
            SettingsTab::Git => SettingsTab::ProjectMgmt,
            SettingsTab::ProjectMgmt => SettingsTab::DevServer,
            SettingsTab::DevServer => SettingsTab::General,
        }
    }
}
```

### Add Settings Fields

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    // ... existing fields ...
    
    // Dev Server
    DevServerCommand,
    DevServerRunBefore,
    DevServerWorkingDir,
    DevServerPort,
    DevServerAutoStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    // ... existing ...
    DevServer,
}
```

### Update Settings Modal

Add Dev Server tab rendering to `src/ui/components/settings_modal.rs`:

```rust
fn get_field_display(&self, field: &SettingsField) -> (String, String, bool) {
    match field {
        // ... existing matches ...
        
        SettingsField::DevServerCommand => (
            "Command".to_string(),
            self.state.repo_config.dev_server.command.clone().unwrap_or_default(),
            false,
        ),
        SettingsField::DevServerRunBefore => (
            "Run Before".to_string(),
            self.state.repo_config.dev_server.run_before.join(", "),
            false,
        ),
        SettingsField::DevServerWorkingDir => (
            "Working Dir".to_string(),
            self.state.repo_config.dev_server.working_dir.clone(),
            false,
        ),
        SettingsField::DevServerPort => (
            "Port".to_string(),
            self.state.repo_config.dev_server.port.map(|p| p.to_string()).unwrap_or_default(),
            false,
        ),
        SettingsField::DevServerAutoStart => (
            "Auto Start".to_string(),
            if self.state.repo_config.dev_server.auto_start { "[x]" } else { "[ ]" }.to_string(),
            true,
        ),
    }
}
```

---

## File Structure Summary

```
src/
├── devserver/                    # NEW MODULE
│   ├── mod.rs                    # Module exports
│   ├── process.rs                # DevServer struct, process management
│   └── manager.rs                # DevServerManager for multi-server
├── app/
│   ├── action.rs                 # + Dev server actions
│   ├── config.rs                 # + DevServerConfig in RepoConfig
│   └── state.rs                  # + PreviewTab, DevServerWarning
├── ui/
│   ├── app.rs                    # + tab bar rendering, routing
│   └── components/
│       ├── mod.rs                # + devserver exports
│       ├── devserver_view.rs     # NEW: Dev server log viewer
│       ├── devserver_warning.rs  # NEW: Conflict warning modal
│       └── settings_modal.rs     # + Dev Server tab
├── lib.rs                        # + pub mod devserver;
└── main.rs                       # + DevServerManager, action handlers
```

---

## Acceptance Criteria

| Criteria | Implementation |
|----------|----------------|
| Start dev server | `DevServer::start()` with config |
| Stop dev server | `DevServer::stop()` with process tree kill |
| Restart dev server | `DevServer::restart()` |
| Log streaming | Background task with stdout/stderr readers |
| Status tracking | `DevServerStatus` enum |
| Tab navigation | `PreviewTab` + Tab/Shift+Tab |
| Preview integration | Tab bar in preview pane |
| Worktree awareness | Per-agent dev server instance |
| Process cleanup | Kill process tree on stop/quit |
| Settings UI | Dev Server tab in settings modal |
| Multiple servers | `HashMap<Uuid, DevServer>` |
| Conflict warning | `DevServerWarningModal` |
| Browser preview | `OpenDevServerInBrowser` action |

---

## Testing Checklist

- [ ] Dev server starts with configured command
- [ ] Pre-run commands execute before main command
- [ ] Dev server stops cleanly (no orphan processes)
- [ ] Logs stream correctly to UI
- [ ] Status updates correctly (Starting → Running → Stopped)
- [ ] Tab navigation works (Tab/Shift+Tab)
- [ ] Restart flow works correctly
- [ ] Multiple dev servers can run concurrently
- [ ] Warning modal shows when starting with existing server
- [ ] All dev servers stop on TUI quit
- [ ] Dev server persists across agent selection changes
- [ ] Settings save/load correctly
- [ ] Browser opens to correct port
- [ ] Works on macOS, Linux, Windows
