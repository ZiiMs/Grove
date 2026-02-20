# AGENTS.md

Guidelines for coding agents working in the Flock codebase.

## Project Overview

Flock is a terminal UI (TUI) for managing multiple Claude Code agents with git worktree isolation. Built with Rust using ratatui for the UI, tokio for async runtime, and git2 for git operations.

## Build/Lint/Test Commands

```bash
cargo build                    # Development build
cargo build --release          # Production build
cargo test                     # Run all tests
cargo test test_name           # Run a single test by name
cargo test -- --nocapture      # Run tests with output visible
cargo clippy --all-targets --all-features -- -D warnings  # Lint
cargo fmt -- --check           # Format check
cargo fmt                      # Auto-format
cargo run -- /path/to/repo     # Run the application
```

## Code Style Guidelines

### Error Handling

Use `anyhow` for error handling:

```rust
use anyhow::{Context, Result, bail};

fn load_config() -> Result<Config> {
    let content = std::fs::read_to_string(&path)
        .context("Failed to read config file")?;
    Ok(toml::from_str(&content).context("Failed to parse config")?)
}
```

### Async Patterns

```rust
#[tokio::main]
async fn main() -> Result<()> { /* ... */ }

let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
tokio::spawn(async move { let _ = tx.send(Action::UpdateStatus { ... }); });
```

### Module Organization

Each module has a `mod.rs` that re-exports public items:

```rust
// src/agent/mod.rs
pub mod detector;
pub mod manager;
pub mod model;
pub use detector::detect_status;
pub use manager::AgentManager;
pub use model::{Agent, AgentStatus};
```

### Naming Conventions

- Functions/variables: `snake_case` (`select_next`, `agent_list`)
- Types/traits: `PascalCase` (`AppState`, `AgentStatus`)
- Constants: `SCREAMING_SNAKE_CASE` (`MAX_BUFFER_SIZE`)

### Serde Patterns

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub gitlab: GitLabConfig,
    #[serde(skip)]
    pub git_status: Option<GitSyncStatus>,  // Runtime-only
}
```

## Architecture Patterns

### Action-Based State Management

All state mutations go through the `Action` enum:

```rust
#[derive(Debug, Clone)]
pub enum Action {
    SelectNext,
    CreateAgent { name: String, branch: String },
    DeleteAgent { id: Uuid },
    UpdateAgentStatus { id: Uuid, status: AgentStatus },
    Quit,
}
```

### Widget Pattern

UI components follow the builder pattern:

```rust
pub struct AgentListWidget<'a> { agents: &'a [&'a Agent], selected: usize }

impl<'a> AgentListWidget<'a> {
    pub fn new(agents: &'a [&'a Agent], selected: usize) -> Self { /* ... */ }
    pub fn render(self, frame: &mut Frame, area: Rect) { /* ... */ }
}

AgentListWidget::new(&agents, selected).render(frame, area);
```

## TUI Rendering Patterns

```rust
use ratatui::layout::{Layout, Direction, Constraint};

let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(8), Constraint::Min(10)])
    .split(area);

let block = Block::default().title(" AGENTS ").borders(Borders::ALL);
let style = Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);
```

## Git Workflow

Always run `cargo fmt` before pushing changes to ensure consistent code formatting:

```bash
cargo fmt && git add . && git commit
```

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_status_detection() {
        assert!(matches!(detect_status("⠋ Reading..."), AgentStatus::Running));
    }
}
```

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` | Terminal UI rendering |
| `crossterm` | Terminal events |
| `tokio` | Async runtime |
| `anyhow` | Error handling |
| `serde` | Serialization |
| `git2` | Git operations |

## File Structure

```
src/
├── main.rs          # Entry point, event loop
├── agent/           # Agent model, status detection
├── app/             # AppState, Config, Action enum
├── git/             # Git operations, worktree
├── gitlab/          # GitLab API client
├── asana/           # Asana API client
├── storage/         # Session persistence
├── tmux/            # tmux session management
└── ui/              # TUI components
```

## Configuration

Flock uses a two-level configuration system:

### Global Config (`~/.flock/config.toml`)

User preferences stored globally:

```toml
[global]
ai_agent = "claude-code"  # claude-code, opencode, codex, gemini
log_level = "info"

[ui]
frame_rate = 30
tick_rate_ms = 250
output_buffer_lines = 5000

[performance]
agent_poll_ms = 500
git_refresh_secs = 30
gitlab_refresh_secs = 60
```

### Repo Config (`.flock/project.toml`)

Project-specific settings stored in the repo (can be committed):

```toml
[git]
provider = "gitlab"           # gitlab, github, bitbucket
branch_prefix = "feature/"
main_branch = "main"
worktree_symlinks = ["node_modules", ".env"]

[git.gitlab]
project_id = 12345
base_url = "https://gitlab.com"

[git.github]
owner = "myorg"
repo = "myrepo"

[asana]
project_gid = "1201234567890"
in_progress_section_gid = "1201234567891"
done_section_gid = "1201234567892"
```

### Secrets (Environment Variables)

API tokens are read from environment variables (never stored in config files):

- `GITLAB_TOKEN` - GitLab personal access token
- `ASANA_TOKEN` - Asana personal access token

### Config Merge Order

1. Global config provides defaults
2. Repo config overrides project-specific fields
3. Environment variables provide secrets

