# AGENTS.md

Guidelines for coding agents working in the Grove codebase.

## Project Overview

Grove is a terminal UI (TUI) for managing multiple Claude Code agents with git worktree isolation. Built with Rust using ratatui for the UI, tokio for async runtime, and git2 for git operations.

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

## Version System

Grove uses **semver + git hash versioning** - automatically generated at build time:

- **Format**: `{version} ({hash})` (e.g., `0.1.0 (abc1234)`)
- **Version**: Read from `Cargo.toml` (managed by release-plz)
- **Hash**: Short git commit hash for unique identification

### No Manual Updates Required

- Version comes from `Cargo.toml` `version` field
- Git hash is extracted at compile time via `build.rs`
- Each worktree/branch automatically gets its unique hash
- **Never edit version manually** - release-plz handles it

### Creating a Release

Releases are automated via release-plz:

1. Merge PRs to `main`
2. release-plz creates a PR with version bump and changelog
3. Merge the release PR
4. release-plz tags the release and publishes

The binary will show the semver from Cargo.toml plus the current commit hash.

### How It Works

1. `build.rs` runs before every compilation
2. Reads version from `CARGO_PKG_VERSION` environment variable (Cargo.toml)
3. Gets short git hash via `git rev-parse --short HEAD`
4. Combines into `{version} ({hash})` format
5. Writes to `$OUT_DIR/version.txt`, embedded in binary

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

Grove uses a two-level configuration system:

### Global Config (`~/.grove/config.toml`)

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

### Repo Config (`.grove/project.toml`)

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

## Adding New Keybinds

Keybinds are user-customizable and stored in `~/.grove/config.toml`. When adding a new keybind, you MUST update all of the following:

### 1. Add to `Keybinds` struct (`src/app/config.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybinds {
    // ... existing keybinds ...
    #[serde(default = "default_my_new_action")]
    pub my_new_action: Keybind,
}

fn default_my_new_action() -> Keybind {
    Keybind::new("x")  // or Keybind::with_modifiers("x", vec!["Shift".to_string()])
}
```

### 2. Add to `SettingsField` enum (`src/app/state.rs`)

```rust
pub enum SettingsField {
    // ... existing fields ...
    KbMyNewAction,
}
```

### 3. Add to `SettingsCategory` enum if needed (`src/app/state.rs`)

Choose an appropriate category or create a new one.

### 4. Update `SettingsField::tab()` to return `SettingsTab::Keybinds`

### 5. Update `SettingsField::is_keybind_field()` to include the new field

### 6. Update `SettingsField::keybind_name()` to return display name

### 7. Add to `SettingsState::get_keybind()` and `set_keybind()`

### 8. Add to `SettingsItem::all_for_tab()` under `SettingsTab::Keybinds`

### 9. Update key handler in `src/main.rs`

Use `matches_keybind()` function instead of hardcoding key codes:

```rust
if matches_keybind(key, &kb.my_new_action) {
    return Some(Action::MyNewAction);
}
```

### 10. Update visual displays

Add the new keybind to the help overlay and optionally the status bar:

**Help overlay (`src/ui/components/help_overlay.rs`):**

```rust
Line::from(format!(
    "  {:8} Description of action",
    kb.my_new_action.display_short()
)),
```

- Use `kb.my_new_action.display_short()` for the key display
- Group by category: Navigation, Agent Management, Git Operations, View Controls, External Services, Project Mgmt, Dev Server, Other
- Include a brief description of what the action does
- If help content grows, adjust `centered_rect(60, 80, area)` to show more

**Status bar (`src/ui/components/status_bar.rs`):**

Only add commonly-used global actions to the status bar shortcuts array.

### 11. Document non-configurable keybinds

Some keybinds are hardcoded (not user-customizable). These include:

- `Tab` / `Shift+Tab` - Switch preview tabs
- `Ctrl+c` - Force quit
- `j`/`k` or arrows in dropdowns/menus
- `y`/`n` in confirmation dialogs
- Dev server controls: `Ctrl+s` (start), `Ctrl+Shift+s` (restart), `C` (clear), `O` (open)

When adding a non-configurable keybind:
- Document it in `help_overlay.rs` with the hardcoded key (e.g., `"  Tab      Switch preview tab"`)
- Add a code comment explaining why it's not configurable

### 12. Test the changes

```bash
cargo build && cargo run -- /path/to/test/repo
```

Press `?` to verify the new keybind appears correctly in the help overlay.


