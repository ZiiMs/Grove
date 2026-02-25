# Grove

A terminal UI (TUI) for managing multiple Claude Code agents with git worktree isolation.

Grove allows you to run multiple instances of Claude Code simultaneously, each working on its own git branch in an isolated worktree. Monitor their progress, attach to their terminal sessions, and integrate with GitLab merge requests and Asana tasks.

## Features

- **Multi-Agent Management**: Run multiple Claude Code agents in parallel, each in its own tmux session
- **Git Worktree Isolation**: Each agent works on a separate branch in an isolated worktree, preventing conflicts
- **Real-Time Monitoring**: See live output from each agent, detect status (running, waiting, error), and track activity
- **GitLab Integration**: Automatically detect merge requests, view pipeline status, and open MRs in browser
- **Asana Integration**: Link agents to Asana tasks, automatically move tasks through workflow stages
- **Session Persistence**: Agent sessions persist across restarts; pause/resume agents without losing Claude's context
- **System Metrics**: Monitor CPU and memory usage while agents work

## Prerequisites

Before installing Grove, ensure you have the following:

### Required

1. **tmux**
    ```bash
    # macOS
    brew install tmux

    # Ubuntu/Debian
    sudo apt install tmux

    # Fedora
    sudo dnf install tmux
    ```

2. **Claude Code CLI**

   The `claude` command must be available in your PATH. Install Claude Code following Anthropic's instructions.

3. **Git** (with worktree support, version 2.5+)

### Optional (for integrations)

- **GitLab Account** with API access token (for MR tracking)
- **Asana Account** with personal access token (for task management)

## Installation

### Quick Install (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/ZiiMs/Grove/main/install.sh | bash
```

The installer will:
- Detect your platform and architecture
- Download the latest release binary
- Install to `~/.local/bin` (or specify with `--bin-dir`)
- Optionally install tmux if missing

#### Install Options

```bash
# Install to custom directory
curl -fsSL https://raw.githubusercontent.com/ZiiMs/Grove/main/install.sh | bash -s -- --bin-dir /usr/local/bin

# Install specific version
curl -fsSL https://raw.githubusercontent.com/ZiiMs/Grove/main/install.sh | bash -s -- --version abc1234

# Skip dependency installation (if you already have tmux)
curl -fsSL https://raw.githubusercontent.com/ZiiMs/Grove/main/install.sh | bash -s -- --no-deps
```

### From Source

```bash
# Clone the repository
git clone https://github.com/ZiiMs/Grove.git
cd Grove

# Build in release mode
cargo build --release

# The binary will be at ./target/release/grove
# Optionally, copy it to a directory in your PATH:
cp target/release/grove ~/.local/bin/
```

### From crates.io

```bash
cargo install grove-tui
```

## Quick Start

```bash
# Navigate to any git repository
cd /path/to/your/project

# Run grove
grove

# Or specify a repository path
grove /path/to/your/project
```

## Configuration

Grove uses a TOML configuration file located at `~/.grove/config.toml`. Create this file to customize behavior and set up integrations.

### Configuration File

Create `~/.grove/config.toml`:

```toml
# GitLab Integration
[gitlab]
# Your GitLab instance URL (default: "https://gitlab.com")
base_url = "https://gitlab.com"

# Your project's numeric ID (find it on your project's main page)
project_id = 12345678

# The main/default branch name (default: "main")
main_branch = "main"

# Asana Integration
[asana]
# Your Asana project GID (find it in the project URL)
# Example: In https://app.asana.com/0/1234567890/list the GID is 1234567890
project_gid = "1234567890123456"

# Optional: Override section GIDs for automatic task movement
# If not set, Grove will auto-detect sections named "In Progress" and "Done"
in_progress_section_gid = "1234567890123457"
done_section_gid = "1234567890123458"

# How often to poll Asana for task updates (default: 120 seconds)
refresh_secs = 120

# UI Settings
[ui]
# Target frame rate (default: 30)
frame_rate = 30

# Tick rate in milliseconds (default: 250)
tick_rate_ms = 250

# Number of output lines to buffer per agent (default: 5000)
output_buffer_lines = 5000

# Performance Settings
[performance]
# How often to poll agent status in milliseconds (default: 500)
agent_poll_ms = 500

# How often to refresh git status in seconds (default: 30)
git_refresh_secs = 30

# How often to poll GitLab for MR status in seconds (default: 60)
gitlab_refresh_secs = 60
```

## Environment Variables

Grove uses environment variables for sensitive credentials. Set these in your shell profile (`~/.bashrc`, `~/.zshrc`, etc.):

### GitLab Token

```bash
export GITLAB_TOKEN="your-gitlab-personal-access-token"
```

**How to create a GitLab token:**
1. Go to GitLab > User Settings > Access Tokens
2. Click "Add new token"
3. Name: `grove` (or any name you prefer)
4. Expiration date: Set as needed
5. Scopes: Select `api` (for full API access) or `read_api` (for read-only)
6. Click "Create personal access token"
7. Copy the token and set it as `GITLAB_TOKEN`

### Asana Token

```bash
export ASANA_TOKEN="your-asana-personal-access-token"
```

**How to create an Asana token:**
1. Go to [Asana Developer Console](https://app.asana.com/0/developer-console)
2. Click "Create new token"
3. Name: `grove` (or any name you prefer)
4. Click "Create token"
5. Copy the token and set it as `ASANA_TOKEN`

### Example Shell Configuration

Add to your `~/.zshrc` or `~/.bashrc`:

```bash
# Grove Configuration
export GITLAB_TOKEN="glpat-xxxxxxxxxxxxxxxxxxxx"
export ASANA_TOKEN="1/1234567890123456:abcdefghijklmnopqrstuvwxyz"
```

Then reload your shell:
```bash
source ~/.zshrc  # or source ~/.bashrc
```

## Usage

### Keyboard Shortcuts

#### Navigation
| Key | Action |
|-----|--------|
| `j` / `↓` | Move to next agent |
| `k` / `↑` | Move to previous agent |
| `g` | Go to first agent |
| `G` | Go to last agent |

#### Agent Management
| Key | Action |
|-----|--------|
| `n` | Create new agent (prompts for branch name) |
| `d` | Delete selected agent |
| `Enter` | Attach to agent's tmux session |
| `N` | Set/edit custom note for agent |
| `s` | Request work summary (for sharing on Slack) |
| `y` | Copy agent/branch name to clipboard |

#### Git Operations
| Key | Action |
|-----|--------|
| `c` | Pause agent & copy branch (checkout elsewhere) |
| `r` | Resume paused agent |
| `m` | Send merge main request to Claude |
| `p` | Send /push command to Claude |
| `f` | Fetch remote |

#### GitLab
| Key | Action |
|-----|--------|
| `o` | Open merge request in browser |

#### Asana
| Key | Action |
|-----|--------|
| `a` | Assign Asana task to agent |
| `A` | Open Asana task in browser |

#### Other
| Key | Action |
|-----|--------|
| `R` | Refresh all status |
| `?` | Toggle help overlay |
| `L` | Toggle logs view |
| `q` | Quit |
| `Esc` | Cancel current action / close dialogs |

### Creating an Agent

1. Press `n` to create a new agent
2. Enter a branch name (this will be both the agent name and git branch)
3. Grove will:
   - Create a new git branch (if it doesn't exist)
   - Create a git worktree for isolated work
   - Start a tmux session
   - Launch Claude Code in that session

### Attaching to an Agent

Press `Enter` on a selected agent to attach to its tmux session. You'll be connected directly to Claude's terminal.

To detach and return to Grove: press `Ctrl+B` then `D` (standard tmux detach).

### Pause/Resume Workflow

The pause/resume feature lets you temporarily free up a worktree while preserving Claude's context:

1. **Pause** (`c`):
   - Commits any uncommitted changes
   - Removes the worktree (but keeps the branch)
   - Copies the branch name to clipboard
   - The tmux session stays alive (Claude's context preserved!)

2. **Resume** (`r`):
   - Recreates the worktree
   - Claude picks up right where it left off

This is useful when you need to checkout the branch elsewhere for testing or code review.

### Asana Integration Workflow

1. Create an agent for your task (`n`)
2. Assign an Asana task (`a`) by entering the task URL or GID
3. When Claude starts working (detected as "Running"), the task automatically moves to "In Progress"
4. When you delete the agent with `y` at the confirmation prompt, the task moves to "Done" and is marked complete

### GitLab Integration

When configured, Grove will:
- Automatically detect when Claude creates a merge request
- Show MR status in the agent list
- Display pipeline status (pending, running, passed, failed)
- Allow quick access to MRs in browser (`o`)

## Troubleshooting

### "tmux is not installed or not in PATH"

Install tmux using your package manager (see Prerequisites).

### "Not a git repository"

Grove must be run from within a git repository, or you must specify a git repository path as an argument.

### GitLab integration not working

1. Verify `GITLAB_TOKEN` is set: `echo $GITLAB_TOKEN`
2. Check that `project_id` is set in `~/.grove/config.toml`
3. Ensure your token has `api` or `read_api` scope
4. Test the token: `curl --header "PRIVATE-TOKEN: $GITLAB_TOKEN" "https://gitlab.com/api/v4/user"`

### Asana integration not working

1. Verify `ASANA_TOKEN` is set: `echo $ASANA_TOKEN`
2. Check the logs for error messages (press `L` to view logs)
3. Verify the token is valid by testing the API:
   ```bash
   curl -H "Authorization: Bearer $ASANA_TOKEN" "https://app.asana.com/api/1.0/users/me"
   ```

### Agent stuck in wrong status

Press `R` to force refresh all agent statuses.

### Session persistence

Agent sessions are saved to `~/.grove/sessions/<repo-hash>.json`. If you experience issues, you can delete this file to start fresh (note: this won't affect running tmux sessions).

## Architecture

```
grove/
├── src/
│   ├── main.rs          # Entry point, event loop, action processing
│   ├── lib.rs           # Module exports
│   ├── agent/           # Agent model, status detection, lifecycle
│   ├── app/             # Application state, config, actions
│   ├── asana/           # Asana API client and types
│   ├── git/             # Git operations, worktree management
│   ├── gitlab/          # GitLab API client and types
│   ├── storage/         # Session persistence
│   ├── tmux/            # tmux session management
│   └── ui/              # TUI components (ratatui)
└── Cargo.toml
```

## License

MIT
