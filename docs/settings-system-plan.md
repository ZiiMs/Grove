# Settings System Implementation Plan

## Overview

Add a comprehensive settings system with:
- Global settings (AI agent, git provider, log level)
- Per-project settings (branch prefix, main branch)
- Large centered modal UI with dropdown selection
- First-launch setup wizard
- Persistent storage

## File Changes Overview

| File | Changes |
|------|---------|
| `src/app/config.rs` | Add GlobalConfig, ProjectConfig structs |
| `src/app/action.rs` | Add settings-related Actions |
| `src/app/state.rs` | Add settings UI state |
| `src/ui/components/select.rs` | **NEW** - Reusable dropdown widget |
| `src/ui/components/settings_modal.rs` | **NEW** - Settings modal |
| `src/storage/project.rs` | **NEW** - Per-project config persistence |
| `src/main.rs` | Key handlers, first-launch detection |
| `src/ui/app.rs` | Render settings modal |

## Configuration Structure

### Global Config (`~/.flock/config.toml`)

```toml
[global]
ai_agent = "claude-code"  # claude-code | opencode | codex | gemini
git_provider = "gitlab"   # gitlab | github | bitbucket
log_level = "info"        # debug | info | warn | error

[ui]
frame_rate = 30
tick_rate_ms = 250
output_buffer_lines = 5000

[gitlab]
base_url = "https://gitlab.com"
project_id = 123
main_branch = "main"

[asana]
project_gid = ""
in_progress_section_gid = ""
done_section_gid = ""
refresh_secs = 120
```

### Project Config (`~/.flock/projects/{hash}/config.toml`)

```toml
branch_prefix = "feature/"   # e.g., feature/, bugfix/, task/
main_branch = "main"
```

## New Enum Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AiAgent {
    ClaudeCode,
    Opencode,
    Codex,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum GitProvider {
    GitLab,
    GitHub,
    Bitbucket,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}
```

## SelectWidget Component

A reusable selection widget supporting:
- Up/Down navigation
- Enter to select
- Esc to cancel
- Visual highlight of current selection
- Optional search/filter (future)

## Settings Modal Layout

```
┌─────────────────────────────────────────────────────────┐
│                      SETTINGS                           │
├─────────────────────────────────────────────────────────┤
│  GLOBAL                                                 │
│  ┌─────────────────────────────────────────────────┐   │
│  │ AI Agent:      [Claude Code          ▼]        │   │
│  │ Git Provider:  [GitLab               ▼]        │   │
│  │ Log Level:     [Info                 ▼]        │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  PROJECT                                                │
│  ┌─────────────────────────────────────────────────┐   │
│  │ Branch Prefix: [feature/                    ]   │   │
│  │ Main Branch:   [main                           ]   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  [Tab] Switch section  [Enter] Edit  [Esc] Close       │
└─────────────────────────────────────────────────────────┘
```

When a dropdown is activated:

```
│  │ AI Agent:      [Claude Code          ▼]        │   │
│  │                ┌──────────────────────────┐    │   │
│  │                │ > Claude Code            │    │   │
│  │                │   Opencode               │    │   │
│  │                │   Codex                  │    │   │
│  │                │   Gemini                 │    │   │
│  │                └──────────────────────────┘    │   │
```

## New Actions

```rust
pub enum Action {
    // ... existing
    ToggleSettings,
    SettingsSelectNext,      // Navigate dropdown options
    SettingsSelectPrevious,
    SettingsSelectField,     // Open dropdown for current field
    SettingsUpdateField { value: String },
    SettingsSave,
    SettingsCancel,
}
```

## First-Launch Detection

In `main.rs`, before starting the TUI:

```rust
let config_path = dirs::home_dir().unwrap().join(".flock/config.toml");
if !config_path.exists() {
    // Show setup wizard, then continue
}
```

## Implementation Order

1. **Config structs** - Add AiAgent, GitProvider enums and GlobalConfig
2. **SelectWidget** - Build reusable dropdown component
3. **Settings state** - Add fields to AppState
4. **Settings modal** - Render the settings UI
5. **Key handling** - Wire up navigation and selection
6. **Persistence** - Save/load both config types
7. **First-launch** - Detect and show setup wizard

## Keybindings

| Key | Action |
|-----|--------|
| `S` | Toggle settings modal |
| `Tab` | Switch between Global/Project sections |
| `↑/↓` | Navigate fields or dropdown options |
| `Enter` | Edit field / Select dropdown option |
| `Esc` | Close dropdown / Close settings |
| `q` | Save and close (when in settings) |
