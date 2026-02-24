use std::collections::HashSet;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use arboard::Clipboard;
use sysinfo::System;

use anyhow::Result;

use crossterm::{
    event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use grove::agent::{
    detect_checklist_progress, detect_mr_url, detect_status_for_agent, Agent, AgentManager,
    AgentStatus, ForegroundProcess, ProjectMgmtTaskStatus, StatusDetection,
};
use grove::app::{
    Action, AppState, Config, InputMode, PreviewTab, ProjectMgmtProvider, StatusOption,
    SubtaskStatusDropdownState, TaskItemStatus, TaskListItem, TaskStatusDropdownState, Toast,
    ToastLevel,
};
use grove::core::git_providers::codeberg::OptionalCodebergClient;
use grove::core::git_providers::github::OptionalGitHubClient;
use grove::core::git_providers::gitlab::OptionalGitLabClient;
use grove::core::projects::airtable::{
    parse_airtable_record_id, AirtableTaskStatus, OptionalAirtableClient,
};
use grove::core::projects::asana::{AsanaTaskStatus, OptionalAsanaClient};
use grove::core::projects::clickup::{
    parse_clickup_task_id, ClickUpTaskStatus, OptionalClickUpClient,
};
use grove::core::projects::linear::{
    parse_linear_issue_id, LinearTaskStatus, OptionalLinearClient,
};
use grove::core::projects::notion::{parse_notion_page_id, NotionTaskStatus, OptionalNotionClient};
use grove::devserver::DevServerManager;
use grove::git::{GitSync, Worktree};
use grove::storage::{save_session, SessionStorage};
use grove::tmux::is_tmux_available;
use grove::ui::{AppWidget, DevServerRenderInfo};

fn matches_keybind(key: crossterm::event::KeyEvent, keybind: &grove::app::config::Keybind) -> bool {
    let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let has_alt = key.modifiers.contains(KeyModifiers::ALT);

    let expected_ctrl = keybind.modifiers.iter().any(|m| m == "Control");
    let expected_shift = keybind.modifiers.iter().any(|m| m == "Shift");
    let expected_alt = keybind.modifiers.iter().any(|m| m == "Alt");

    if has_ctrl != expected_ctrl || has_alt != expected_alt {
        return false;
    }

    let key_matches = match &keybind.key[..] {
        "Up" => key.code == KeyCode::Up,
        "Down" => key.code == KeyCode::Down,
        "Left" => key.code == KeyCode::Left,
        "Right" => key.code == KeyCode::Right,
        "Enter" => key.code == KeyCode::Enter,
        "Backspace" => key.code == KeyCode::Backspace,
        "Tab" => key.code == KeyCode::Tab,
        "Esc" => key.code == KeyCode::Esc,
        "Delete" => key.code == KeyCode::Delete,
        "Home" => key.code == KeyCode::Home,
        "End" => key.code == KeyCode::End,
        "PageUp" => key.code == KeyCode::PageUp,
        "PageDown" => key.code == KeyCode::PageDown,
        c => {
            if let Some(ch) = c.chars().next() {
                match key.code {
                    KeyCode::Char(input_ch) => {
                        if ch.is_ascii_alphabetic() {
                            let expected_ch = ch.to_ascii_lowercase();
                            let actual_ch = input_ch.to_ascii_lowercase();
                            if expected_shift {
                                expected_ch == actual_ch && has_shift
                            } else {
                                expected_ch == actual_ch && !has_shift
                            }
                        } else {
                            ch == input_ch
                        }
                    }
                    _ => false,
                }
            } else {
                false
            }
        }
    };

    key_matches
}

#[tokio::main]
async fn main() -> Result<()> {
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/grove-debug.log")
        .ok();

    if let Some(file) = log_file {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("grove=debug".parse().unwrap()),
            )
            .with_writer(std::sync::Arc::new(file))
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("grove=info".parse().unwrap()),
            )
            .with_writer(std::io::stderr)
            .init();
    }

    tracing::info!("=== Grove starting ===");

    // Check prerequisites
    if !is_tmux_available() {
        anyhow::bail!("tmux is not installed or not in PATH. Please install tmux first.");
    }

    // Get repository path from args or current directory
    let repo_path = std::env::args().nth(1).unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string()
    });

    // Verify it's a git repository
    if !std::path::Path::new(&repo_path).join(".git").exists() {
        anyhow::bail!(
            "Not a git repository: {}. Please run grove from a git repository.",
            repo_path
        );
    }

    // Load configuration
    let config = Config::load().unwrap_or_default();

    // Check if this is first launch (no ~/.grove directory exists)
    let is_first_launch = !Config::exists();

    // Check if project config exists
    let repo_config_path = grove::app::RepoConfig::config_path(&repo_path).ok();
    let project_needs_setup = repo_config_path
        .as_ref()
        .map(|p| !p.exists())
        .unwrap_or(true);

    // Initialize storage
    let storage = SessionStorage::new(&repo_path)?;

    // Create app state
    let mut state = AppState::new(config.clone(), repo_path.clone());
    state.log_info(format!("Grove started in {}", repo_path));

    // Show global setup wizard if first launch
    if is_first_launch {
        state.show_global_setup = true;
        state.global_setup = Some(grove::app::GlobalSetupState::default());
        state.log_info("First launch - showing global setup wizard".to_string());
    } else if project_needs_setup {
        // Show project setup wizard if project not configured
        state.show_project_setup = true;
        let wizard = grove::app::ProjectSetupState {
            config: state.settings.repo_config.clone(),
            ..Default::default()
        };
        state.project_setup = Some(wizard);
        state.log_info("Project not configured - showing project setup wizard".to_string());
    }

    if let Ok(Some(session)) = storage.load() {
        let count = session.agents.len();
        for mut agent in session.agents {
            agent.migrate_legacy();
            state.add_agent(agent);
        }
        state.selected_index = session
            .selected_index
            .min(state.agent_order.len().saturating_sub(1));
        state.log_info(format!("Loaded {} agents from session", count));
    }

    let agent_manager = Arc::new(AgentManager::new(&repo_path, state.worktree_base.clone()));

    let gitlab_base_url = &state.settings.repo_config.git.gitlab.base_url;
    let gitlab_project_id = state.settings.repo_config.git.gitlab.project_id;
    let asana_project_gid = state
        .settings
        .repo_config
        .project_mgmt
        .asana
        .project_gid
        .clone();
    let notion_database_id = state
        .settings
        .repo_config
        .project_mgmt
        .notion
        .database_id
        .clone();
    let notion_status_property = state
        .settings
        .repo_config
        .project_mgmt
        .notion
        .status_property_name
        .clone();

    let gitlab_client = Arc::new(OptionalGitLabClient::new(
        gitlab_base_url,
        gitlab_project_id,
        Config::gitlab_token().as_deref(),
    ));

    let github_owner = state.settings.repo_config.git.github.owner.clone();
    let github_repo = state.settings.repo_config.git.github.repo.clone();
    let github_token_set = Config::github_token().is_some();
    let github_log_msg = format!(
        "GitHub config: owner={:?}, repo={:?}, token={}",
        github_owner,
        github_repo,
        if github_token_set { "set" } else { "NOT SET" }
    );
    state.log_info(github_log_msg.clone());
    tracing::info!("{}", github_log_msg);
    let github_client = Arc::new(OptionalGitHubClient::new(
        github_owner.as_deref(),
        github_repo.as_deref(),
        Config::github_token().as_deref(),
    ));

    let codeberg_owner = state.settings.repo_config.git.codeberg.owner.clone();
    let codeberg_repo = state.settings.repo_config.git.codeberg.repo.clone();
    let codeberg_base_url = state.settings.repo_config.git.codeberg.base_url.clone();
    let codeberg_ci_provider = state.settings.repo_config.git.codeberg.ci_provider;
    let codeberg_woodpecker_repo_id = state.settings.repo_config.git.codeberg.woodpecker_repo_id;
    let codeberg_token_set = Config::codeberg_token().is_some();
    let woodpecker_token_set = Config::woodpecker_token().is_some();
    let codeberg_log_msg = format!(
        "Codeberg config: owner={:?}, repo={:?}, base_url={}, ci={:?}, token={}, woodpecker_token={}",
        codeberg_owner,
        codeberg_repo,
        codeberg_base_url,
        codeberg_ci_provider,
        if codeberg_token_set { "set" } else { "NOT SET" },
        if woodpecker_token_set { "set" } else { "NOT SET" }
    );
    state.log_info(codeberg_log_msg.clone());
    tracing::info!("{}", codeberg_log_msg);
    let codeberg_client = Arc::new(OptionalCodebergClient::new(
        codeberg_owner.as_deref(),
        codeberg_repo.as_deref(),
        Some(&codeberg_base_url),
        Config::codeberg_token().as_deref(),
        codeberg_ci_provider,
        Config::woodpecker_token().as_deref(),
        codeberg_woodpecker_repo_id,
    ));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create action channel
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();

    // Create dev server manager
    let devserver_manager = Arc::new(tokio::sync::Mutex::new(DevServerManager::new(
        action_tx.clone(),
    )));

    // Create watch channel for agent list updates (polling task needs current agents)
    let initial_agents: HashSet<Uuid> = state.agents.keys().cloned().collect();
    let (agent_watch_tx, agent_watch_rx) = watch::channel(initial_agents);

    // Create watch channel for agent branches (GitLab polling needs branch names)
    let initial_branches: Vec<(Uuid, String)> = state
        .agents
        .values()
        .map(|a| (a.id, a.branch.clone()))
        .collect();
    let (branch_watch_tx, branch_watch_rx) = watch::channel(initial_branches);

    // Create watch channel for selected agent (preview polling needs current selection)
    let initial_selected: Option<Uuid> = state.selected_agent_id();
    tracing::info!(
        "DEBUG watch channel: initial_selected={:?}, agent_order={:?}, selected_index={}",
        initial_selected,
        state.agent_order,
        state.selected_index
    );
    let (selected_watch_tx, selected_watch_rx) = watch::channel(initial_selected);

    // Start background polling task for agent status
    let agent_poll_tx = action_tx.clone();
    let selected_rx_clone = selected_watch_rx.clone();
    let ai_agent = config.global.ai_agent.clone();
    let debug_mode = config.global.debug_mode;
    tokio::spawn(async move {
        use futures::future::FutureExt;
        use std::panic::AssertUnwindSafe;

        let result = AssertUnwindSafe(async {
            poll_agents(
                agent_watch_rx,
                selected_rx_clone,
                agent_poll_tx,
                ai_agent,
                debug_mode,
            )
            .await
        })
        .catch_unwind()
        .await;

        if let Err(e) = result {
            if let Some(msg) = e.downcast_ref::<&str>() {
                tracing::error!(
                    "poll_agents task PANICKED (should not happen, inner catches): {}",
                    msg
                );
            } else if let Some(msg) = e.downcast_ref::<String>() {
                tracing::error!(
                    "poll_agents task PANICKED (should not happen, inner catches): {}",
                    msg
                );
            } else {
                tracing::error!(
                    "poll_agents task PANICKED (should not happen, inner catches): unknown error"
                );
            }
        }
    });

    // Start background polling task for global system metrics (CPU/memory)
    let system_poll_tx = action_tx.clone();
    tokio::spawn(async move {
        poll_system_metrics(system_poll_tx).await;
    });

    // Start GitLab polling task (if configured)
    if gitlab_client.is_configured().await {
        let gitlab_poll_tx = action_tx.clone();
        let gitlab_client_clone = Arc::clone(&gitlab_client);
        let gitlab_refresh_secs = config.performance.gitlab_refresh_secs;
        let branch_rx_clone = branch_watch_rx.clone();
        tokio::spawn(async move {
            poll_gitlab_mrs(
                branch_rx_clone,
                gitlab_client_clone,
                gitlab_poll_tx,
                gitlab_refresh_secs,
            )
            .await;
        });
        state.log_info("GitLab integration enabled".to_string());
    } else {
        state.log_debug("GitLab not configured (set GITLAB_TOKEN and project_id)".to_string());
    }

    // Start GitHub polling task (if configured)
    if github_client.is_configured().await {
        let github_poll_tx = action_tx.clone();
        let github_client_clone = Arc::clone(&github_client);
        let github_refresh_secs = config.performance.github_refresh_secs;
        let branch_rx_clone = branch_watch_rx.clone();
        state.log_info("GitHub integration enabled".to_string());
        tokio::spawn(async move {
            poll_github_prs(
                branch_rx_clone,
                github_client_clone,
                github_poll_tx,
                github_refresh_secs,
            )
            .await;
        });
    } else {
        let msg = format!(
            "GitHub not configured (owner={:?}, repo={:?}, token={})",
            github_owner,
            github_repo,
            if github_token_set { "set" } else { "NOT SET" }
        );
        state.log_debug(msg);
    }

    // Start Codeberg polling task (if configured)
    if codeberg_client.is_configured().await {
        let codeberg_poll_tx = action_tx.clone();
        let codeberg_client_clone = Arc::clone(&codeberg_client);
        let codeberg_refresh_secs = config.performance.codeberg_refresh_secs;
        let branch_rx_clone = branch_watch_rx.clone();
        state.log_info("Codeberg integration enabled".to_string());
        tokio::spawn(async move {
            poll_codeberg_prs(
                branch_rx_clone,
                codeberg_client_clone,
                codeberg_poll_tx,
                codeberg_refresh_secs,
            )
            .await;
        });
    } else {
        let msg = format!(
            "Codeberg not configured (owner={:?}, repo={:?}, token={})",
            codeberg_owner,
            codeberg_repo,
            if codeberg_token_set { "set" } else { "NOT SET" }
        );
        state.log_debug(msg);
    }

    let asana_client = Arc::new(OptionalAsanaClient::new(
        Config::asana_token().as_deref(),
        asana_project_gid,
        config.asana.cache_ttl_secs,
    ));

    let notion_client = Arc::new(OptionalNotionClient::new(
        Config::notion_token().as_deref(),
        notion_database_id,
        notion_status_property,
        config.notion.cache_ttl_secs,
    ));

    let clickup_list_id = state
        .settings
        .repo_config
        .project_mgmt
        .clickup
        .list_id
        .clone();
    let clickup_client = Arc::new(OptionalClickUpClient::new(
        Config::clickup_token().as_deref(),
        clickup_list_id,
        config.clickup.cache_ttl_secs,
    ));

    let airtable_base_id = state
        .settings
        .repo_config
        .project_mgmt
        .airtable
        .base_id
        .clone();
    let airtable_table_name = state
        .settings
        .repo_config
        .project_mgmt
        .airtable
        .table_name
        .clone();
    let airtable_status_field = state
        .settings
        .repo_config
        .project_mgmt
        .airtable
        .status_field_name
        .clone();
    let airtable_client = Arc::new(OptionalAirtableClient::new(
        Config::airtable_token().as_deref(),
        airtable_base_id,
        airtable_table_name,
        airtable_status_field,
        config.airtable.cache_ttl_secs,
    ));

    let linear_team_id = state
        .settings
        .repo_config
        .project_mgmt
        .linear
        .team_id
        .clone();
    let linear_client = Arc::new(OptionalLinearClient::new(
        Config::linear_token().as_deref(),
        linear_team_id,
        config.linear.cache_ttl_secs,
    ));

    let pm_provider = state.settings.repo_config.project_mgmt.provider;

    let initial_asana_tasks: Vec<(Uuid, String)> = state
        .agents
        .values()
        .filter_map(|a| {
            a.pm_task_status
                .as_asana()
                .and_then(|s| s.gid().map(|gid| (a.id, gid.to_string())))
        })
        .collect();
    let (asana_watch_tx, asana_watch_rx) = watch::channel(initial_asana_tasks);

    let initial_notion_tasks: Vec<(Uuid, String)> = state
        .agents
        .values()
        .filter_map(|a| {
            a.pm_task_status
                .as_notion()
                .and_then(|s| s.page_id().map(|id| (a.id, id.to_string())))
        })
        .collect();
    let (notion_watch_tx, notion_watch_rx) = watch::channel(initial_notion_tasks);

    if asana_client.is_configured().await && matches!(pm_provider, ProjectMgmtProvider::Asana) {
        let asana_poll_tx = action_tx.clone();
        let asana_client_clone = Arc::clone(&asana_client);
        let refresh_secs = config.asana.refresh_secs;
        tokio::spawn(async move {
            poll_asana_tasks(
                asana_watch_rx,
                asana_client_clone,
                asana_poll_tx,
                refresh_secs,
            )
            .await;
        });
        state.log_info("Asana integration enabled".to_string());
    } else {
        state.log_debug("Asana not configured (set ASANA_TOKEN)".to_string());
    }

    if notion_client.is_configured().await && matches!(pm_provider, ProjectMgmtProvider::Notion) {
        let notion_poll_tx = action_tx.clone();
        let notion_client_clone = Arc::clone(&notion_client);
        let refresh_secs = config.notion.refresh_secs;
        tokio::spawn(async move {
            poll_notion_tasks(
                notion_watch_rx,
                notion_client_clone,
                notion_poll_tx,
                refresh_secs,
            )
            .await;
        });
        state.log_info("Notion integration enabled".to_string());
    } else {
        state.log_debug("Notion not configured (set NOTION_TOKEN and database_id)".to_string());
    }

    let initial_clickup_tasks: Vec<(Uuid, String)> = state
        .agents
        .values()
        .filter_map(|a| {
            a.pm_task_status
                .as_clickup()
                .and_then(|s| s.id().map(|id| (a.id, id.to_string())))
        })
        .collect();
    let (clickup_watch_tx, clickup_watch_rx) = watch::channel(initial_clickup_tasks);

    if clickup_client.is_configured().await && matches!(pm_provider, ProjectMgmtProvider::Clickup) {
        let clickup_poll_tx = action_tx.clone();
        let clickup_client_clone = Arc::clone(&clickup_client);
        let refresh_secs = config.clickup.refresh_secs;
        tokio::spawn(async move {
            poll_clickup_tasks(
                clickup_watch_rx,
                clickup_client_clone,
                clickup_poll_tx,
                refresh_secs,
            )
            .await;
        });
        state.log_info("ClickUp integration enabled".to_string());
    } else {
        state.log_debug("ClickUp not configured (set CLICKUP_TOKEN and list_id)".to_string());
    }

    let initial_airtable_tasks: Vec<(Uuid, String)> = state
        .agents
        .values()
        .filter_map(|a| {
            a.pm_task_status
                .as_airtable()
                .and_then(|s| s.id().map(|id| (a.id, id.to_string())))
        })
        .collect();
    let (airtable_watch_tx, airtable_watch_rx) = watch::channel(initial_airtable_tasks);

    if airtable_client.is_configured().await && matches!(pm_provider, ProjectMgmtProvider::Airtable)
    {
        let airtable_poll_tx = action_tx.clone();
        let airtable_client_clone = Arc::clone(&airtable_client);
        let refresh_secs = config.airtable.refresh_secs;
        tokio::spawn(async move {
            poll_airtable_tasks(
                airtable_watch_rx,
                airtable_client_clone,
                airtable_poll_tx,
                refresh_secs,
            )
            .await;
        });
        state.log_info("Airtable integration enabled".to_string());
    } else {
        state.log_debug(
            "Airtable not configured (set AIRTABLE_TOKEN, base_id, and table_name)".to_string(),
        );
    }

    let initial_linear_tasks: Vec<(Uuid, String)> = state
        .agents
        .values()
        .filter_map(|a| {
            a.pm_task_status
                .as_linear()
                .and_then(|s| s.id().map(|id| (a.id, id.to_string())))
        })
        .collect();
    let (linear_watch_tx, linear_watch_rx) = watch::channel(initial_linear_tasks);

    if linear_client.is_configured().await && matches!(pm_provider, ProjectMgmtProvider::Linear) {
        let linear_poll_tx = action_tx.clone();
        let linear_client_clone = Arc::clone(&linear_client);
        let refresh_secs = config.linear.refresh_secs;
        tokio::spawn(async move {
            poll_linear_tasks(
                linear_watch_rx,
                linear_client_clone,
                linear_poll_tx,
                refresh_secs,
            )
            .await;
        });
        state.log_info("Linear integration enabled".to_string());
    } else {
        state.log_debug("Linear not configured (set LINEAR_TOKEN and team_id)".to_string());
    }

    // Main event loop
    let poll_timeout = Duration::from_millis(50);
    let tick_interval = Duration::from_millis(100);
    let mut last_tick = std::time::Instant::now();
    let mut pending_attach: Option<Uuid> = None;
    let mut pending_devserver_attach: Option<Uuid> = None;
    let mut pending_editor: Option<Uuid> = None;

    loop {
        // Handle pending dev server attach (outside of async context)
        if let Some(id) = pending_devserver_attach.take() {
            let session_name = devserver_manager
                .try_lock()
                .ok()
                .and_then(|m| m.get_tmux_session(id));

            if let Some(session_name) = session_name {
                state.log_info(format!(
                    "Attaching to dev server session '{}'",
                    session_name
                ));

                // Save session before attaching
                let agents: Vec<Agent> = state.agents.values().cloned().collect();
                let _ = save_session(&storage, &state.repo_path, &agents, state.selected_index);

                // Leave TUI mode
                disable_raw_mode()?;
                execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

                // Attach to tmux (blocks until detach)
                let tmux_session = grove::tmux::TmuxSession::new(&session_name);
                let attach_result = tmux_session.attach();

                // Restore TUI mode
                enable_raw_mode()?;
                execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
                terminal.clear()?;

                // Drain any stale input events
                while poll(Duration::from_millis(1))? {
                    let _ = event::read();
                }

                state.log_info("Returned from dev server session");

                if let Err(e) = attach_result {
                    state.log_error(format!("Attach error: {}", e));
                }
            }
            continue;
        }

        // Handle pending attach (outside of async context)
        if let Some(id) = pending_attach.take() {
            // Clone agent data we need before borrowing state mutably
            let agent_clone = state.agents.get(&id).cloned();
            if let Some(agent) = agent_clone {
                state.log_info(format!("Attaching to agent '{}'", agent.name));

                // Save session before attaching
                let agents: Vec<Agent> = state.agents.values().cloned().collect();
                let _ = save_session(&storage, &state.repo_path, &agents, state.selected_index);

                // Leave TUI mode
                disable_raw_mode()?;
                execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

                // Attach to tmux (blocks until detach)
                let ai_agent = state.config.global.ai_agent.clone();
                let attach_result = agent_manager.attach_to_agent(&agent, &ai_agent);

                // Restore TUI mode
                enable_raw_mode()?;
                execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
                terminal.clear()?;

                // Drain any stale input events
                while poll(Duration::from_millis(1))? {
                    let _ = event::read();
                }

                state.log_info("Returned from tmux session");

                if let Err(e) = attach_result {
                    state.log_error(format!("Attach error: {}", e));
                }
            }
            continue;
        }

        // Handle pending editor open (outside of async context)
        if let Some(id) = pending_editor.take() {
            let agent_clone = state.agents.get(&id).cloned();
            if let Some(agent) = agent_clone {
                let editor_cmd = state
                    .config
                    .global
                    .editor
                    .replace("{path}", &agent.worktree_path);

                state.log_info(format!("Opening editor for '{}'", agent.name));

                // Leave TUI mode
                disable_raw_mode()?;
                execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

                // Run editor (blocks until exit)
                let editor_result = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&editor_cmd)
                    .status();

                // Restore TUI mode
                enable_raw_mode()?;
                execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
                terminal.clear()?;

                // Drain any stale input events
                while poll(Duration::from_millis(1))? {
                    let _ = event::read();
                }

                state.log_info("Returned from editor");

                if let Err(e) = editor_result {
                    state.log_error(format!("Editor error: {}", e));
                }
            }
            continue;
        }

        // Render
        terminal.draw(|f| {
            let devserver_info = if let Some(agent) = state.selected_agent() {
                if let Ok(manager) = devserver_manager.try_lock() {
                    manager.get(agent.id).map(|server| DevServerRenderInfo {
                        status: server.status().clone(),
                        logs: server.logs().to_vec(),
                        agent_name: server.agent_name().to_string(),
                    })
                } else {
                    None
                }
            } else {
                None
            };

            let devserver_statuses = devserver_manager
                .try_lock()
                .map(|m| m.all_statuses())
                .unwrap_or_default();

            AppWidget::new(&state)
                .with_devserver(devserver_info)
                .with_devserver_statuses(devserver_statuses)
                .render(f);
        })?;

        // Poll for keyboard input (non-blocking with timeout)
        if poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                if let Some(action) = handle_key_event(key, &state) {
                    // Check if it's an attach action
                    match action {
                        Action::AttachToAgent { id } => {
                            pending_attach = Some(id);
                            continue;
                        }
                        Action::AttachToDevServer { agent_id } => {
                            pending_devserver_attach = Some(agent_id);
                            continue;
                        }
                        Action::OpenInEditor { id } => {
                            pending_editor = Some(id);
                            continue;
                        }
                        _ => action_tx.send(action)?,
                    }
                }
            }
        }

        // Send tick for animation updates
        if last_tick.elapsed() >= tick_interval {
            action_tx.send(Action::Tick)?;
            last_tick = std::time::Instant::now();
        }

        // Process any pending actions from background tasks
        while let Ok(action) = action_rx.try_recv() {
            match process_action(
                action,
                &mut state,
                &agent_manager,
                &gitlab_client,
                &github_client,
                &codeberg_client,
                &asana_client,
                &notion_client,
                &clickup_client,
                &airtable_client,
                &linear_client,
                &storage,
                &action_tx,
                &agent_watch_tx,
                &branch_watch_tx,
                &selected_watch_tx,
                &asana_watch_tx,
                &notion_watch_tx,
                &clickup_watch_tx,
                &airtable_watch_tx,
                &linear_watch_tx,
                &devserver_manager,
            )
            .await
            {
                Ok(should_quit) => {
                    if should_quit {
                        state.running = false;
                    }
                }
                Err(e) => {
                    state.log_error(format!("Action error: {}", e));
                }
            }
        }

        if !state.running {
            break;
        }
    }

    // Cleanup
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Save session on exit
    let agents: Vec<Agent> = state.agents.values().cloned().collect();
    save_session(&storage, &state.repo_path, &agents, state.selected_index)?;

    Ok(())
}

/// Convert key events to actions.
fn handle_key_event(key: crossterm::event::KeyEvent, state: &AppState) -> Option<Action> {
    // Handle settings mode first
    if state.settings.active {
        return handle_settings_key(key, state);
    }

    // Handle PM setup modal
    if state.pm_setup.active {
        let provider = state.settings.repo_config.project_mgmt.provider;
        return handle_pm_setup_key(key, &state.pm_setup, provider);
    }

    // Handle Git setup modal
    if state.git_setup.active {
        let provider = state.settings.repo_config.git.provider;
        return handle_git_setup_key(key, &state.git_setup, provider);
    }

    // Handle task reassignment warning modal
    if state.task_reassignment_warning.is_some() {
        return match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => Some(Action::ConfirmTaskReassignment),
            KeyCode::Char('n') | KeyCode::Esc => Some(Action::DismissTaskReassignmentWarning),
            _ => None,
        };
    }

    // Handle dev server warning modal
    if state.devserver_warning.is_some() {
        return match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => Some(Action::ConfirmStartDevServer),
            KeyCode::Char('n') | KeyCode::Esc => Some(Action::DismissDevServerWarning),
            _ => None,
        };
    }

    // Handle input mode
    if state.is_input_mode() {
        return handle_input_mode_key(key.code, state);
    }

    // Handle help overlay
    if state.show_help {
        return Some(Action::ToggleHelp);
    }

    // Handle status debug overlay
    if state.show_status_debug {
        let kb = &state.config.keybinds;
        if matches_keybind(key, &kb.debug_status) || key.code == KeyCode::Esc {
            return Some(Action::ToggleStatusDebug);
        }
    }

    // Handle global setup wizard
    if state.show_global_setup {
        if let Some(wizard) = &state.global_setup {
            return match key.code {
                KeyCode::Esc => {
                    if wizard.dropdown_open {
                        Some(Action::GlobalSetupToggleDropdown)
                    } else if matches!(wizard.step, grove::app::GlobalSetupStep::AgentSettings) {
                        Some(Action::GlobalSetupPrevStep)
                    } else {
                        None // Can't go back from first step
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if wizard.dropdown_open {
                        Some(Action::GlobalSetupDropdownPrev)
                    } else if matches!(wizard.step, grove::app::GlobalSetupStep::WorktreeLocation) {
                        Some(Action::GlobalSetupSelectPrev)
                    } else {
                        Some(Action::GlobalSetupNavigateUp)
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if wizard.dropdown_open {
                        Some(Action::GlobalSetupDropdownNext)
                    } else if matches!(wizard.step, grove::app::GlobalSetupStep::WorktreeLocation) {
                        Some(Action::GlobalSetupSelectNext)
                    } else {
                        Some(Action::GlobalSetupNavigateDown)
                    }
                }
                KeyCode::Enter => {
                    if wizard.dropdown_open {
                        Some(Action::GlobalSetupConfirmDropdown)
                    } else if matches!(wizard.step, grove::app::GlobalSetupStep::AgentSettings) {
                        Some(Action::GlobalSetupToggleDropdown)
                    } else {
                        Some(Action::GlobalSetupNextStep)
                    }
                }
                KeyCode::Char('c') => {
                    if matches!(wizard.step, grove::app::GlobalSetupStep::AgentSettings)
                        && !wizard.dropdown_open
                    {
                        Some(Action::GlobalSetupComplete)
                    } else {
                        None
                    }
                }
                _ => None,
            };
        }
    }

    // Handle project setup wizard
    if state.show_project_setup {
        if let Some(wizard) = &state.project_setup {
            return match key.code {
                KeyCode::Esc => {
                    if wizard.git_provider_dropdown_open || wizard.pm_provider_dropdown_open {
                        Some(Action::ProjectSetupToggleDropdown)
                    } else {
                        Some(Action::ProjectSetupSkip)
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if wizard.git_provider_dropdown_open {
                        Some(Action::ProjectSetupDropdownPrev)
                    } else if wizard.pm_provider_dropdown_open {
                        Some(Action::ProjectSetupPmDropdownPrev)
                    } else {
                        Some(Action::ProjectSetupNavigatePrev)
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if wizard.git_provider_dropdown_open {
                        Some(Action::ProjectSetupDropdownNext)
                    } else if wizard.pm_provider_dropdown_open {
                        Some(Action::ProjectSetupPmDropdownNext)
                    } else {
                        Some(Action::ProjectSetupNavigateNext)
                    }
                }
                KeyCode::Enter => {
                    if wizard.git_provider_dropdown_open {
                        Some(Action::ProjectSetupConfirmDropdown)
                    } else if wizard.pm_provider_dropdown_open {
                        Some(Action::ProjectSetupConfirmPmDropdown)
                    } else {
                        Some(Action::ProjectSetupSelect)
                    }
                }
                KeyCode::Char('c') => Some(Action::ProjectSetupComplete),
                KeyCode::Char('l') => {
                    if wizard.git_provider_dropdown_open {
                        Some(Action::ProjectSetupDropdownNext)
                    } else if wizard.pm_provider_dropdown_open {
                        Some(Action::ProjectSetupPmDropdownNext)
                    } else {
                        None
                    }
                }
                KeyCode::Char('h') => {
                    if wizard.git_provider_dropdown_open {
                        Some(Action::ProjectSetupDropdownPrev)
                    } else if wizard.pm_provider_dropdown_open {
                        Some(Action::ProjectSetupPmDropdownPrev)
                    } else {
                        None
                    }
                }
                _ => None,
            };
        }
    }

    // Check if selected agent is paused
    let is_paused = state
        .selected_agent()
        .map(|a| matches!(a.status, grove::agent::AgentStatus::Paused))
        .unwrap_or(false);

    let kb = &state.config.keybinds;

    // Quit (Ctrl+C always works)
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Action::Quit);
    }
    if matches_keybind(key, &kb.quit) {
        return Some(Action::Quit);
    }

    // Navigation
    if matches_keybind(key, &kb.nav_down) {
        return Some(Action::SelectNext);
    }
    if matches_keybind(key, &kb.nav_up) {
        return Some(Action::SelectPrevious);
    }
    if matches_keybind(key, &kb.nav_first) {
        return Some(Action::SelectFirst);
    }
    if matches_keybind(key, &kb.nav_last) {
        return Some(Action::SelectLast);
    }

    // Resume (only when paused)
    if is_paused && matches_keybind(key, &kb.resume) {
        return state
            .selected_agent_id()
            .map(|id| Action::ResumeAgent { id });
    }

    // Refresh selected agent status (only when not paused)
    if !is_paused && matches_keybind(key, &kb.resume) && state.selected_agent_id().is_some() {
        return Some(Action::RefreshSelected);
    }

    // Yank (copy) agent name to clipboard
    if matches_keybind(key, &kb.yank) {
        return state
            .selected_agent_id()
            .map(|id| Action::CopyAgentName { id });
    }

    // Notes
    if matches_keybind(key, &kb.set_note) {
        return Some(Action::EnterInputMode(InputMode::SetNote));
    }

    // New agent
    if matches_keybind(key, &kb.new_agent) {
        return Some(Action::EnterInputMode(InputMode::NewAgent));
    }

    // Delete agent
    if matches_keybind(key, &kb.delete_agent) {
        let has_task = state
            .selected_agent()
            .map(|a| a.pm_task_status.is_linked())
            .unwrap_or(false);
        return Some(if has_task {
            Action::EnterInputMode(InputMode::ConfirmDeleteTask)
        } else {
            Action::EnterInputMode(InputMode::ConfirmDelete)
        });
    }

    // Attach to agent
    if matches_keybind(key, &kb.attach) {
        return match state.preview_tab {
            PreviewTab::Preview => state
                .selected_agent_id()
                .map(|id| Action::AttachToAgent { id }),
            PreviewTab::DevServer => state
                .selected_agent_id()
                .map(|id| Action::AttachToDevServer { agent_id: id }),
        };
    }

    // Pause (only when not paused)
    if !is_paused && matches_keybind(key, &kb.pause) {
        return state
            .selected_agent_id()
            .map(|id| Action::PauseAgent { id });
    }

    // Merge (only when not paused)
    if !is_paused && matches_keybind(key, &kb.merge) && state.selected_agent_id().is_some() {
        return Some(Action::EnterInputMode(InputMode::ConfirmMerge));
    }

    // Push (only when not paused)
    if !is_paused && matches_keybind(key, &kb.push) && state.selected_agent_id().is_some() {
        return Some(Action::EnterInputMode(InputMode::ConfirmPush));
    }

    // Fetch (only when not paused)
    if !is_paused && matches_keybind(key, &kb.fetch) {
        return state
            .selected_agent_id()
            .map(|id| Action::FetchRemote { id });
    }

    // Summary (only when not paused)
    if !is_paused
        && matches_keybind(key, &kb.summary)
        && !key.modifiers.contains(KeyModifiers::CONTROL)
    {
        return state
            .selected_agent_id()
            .map(|id| Action::RequestSummary { id });
    }

    // Toggle diff
    if matches_keybind(key, &kb.toggle_diff) {
        return Some(Action::ToggleDiffView);
    }

    // Toggle logs
    if matches_keybind(key, &kb.toggle_logs) {
        return Some(Action::ToggleLogs);
    }

    // Toggle settings
    if matches_keybind(key, &kb.toggle_settings) && !key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Action::ToggleSettings);
    }

    // Open MR/PR
    if matches_keybind(key, &kb.open_mr) {
        let provider = state.settings.repo_config.git.provider;
        return match provider {
            grove::app::GitProvider::GitLab => state
                .selected_agent_id()
                .map(|id| Action::OpenMrInBrowser { id }),
            grove::app::GitProvider::GitHub => state
                .selected_agent_id()
                .map(|id| Action::OpenPrInBrowser { id }),
            grove::app::GitProvider::Codeberg => state
                .selected_agent_id()
                .map(|id| Action::OpenCodebergPrInBrowser { id }),
        };
    }

    // Open in editor
    if !is_paused && matches_keybind(key, &kb.open_editor) {
        return state
            .selected_agent_id()
            .map(|id| Action::OpenInEditor { id });
    }

    // Project management task assignment
    if matches_keybind(key, &kb.asana_assign) {
        return Some(Action::EnterInputMode(InputMode::AssignProjectTask));
    }

    // Open task in browser
    if matches_keybind(key, &kb.asana_open) {
        return state
            .selected_agent_id()
            .map(|id| Action::OpenProjectTaskInBrowser { id });
    }

    // Refresh all
    if matches_keybind(key, &kb.refresh_all) {
        return Some(Action::RefreshAll);
    }

    // Toggle help
    if matches_keybind(key, &kb.toggle_help) {
        return Some(Action::ToggleHelp);
    }

    // Task browsing
    if matches_keybind(key, &kb.show_tasks) {
        return Some(Action::EnterInputMode(InputMode::BrowseTasks));
    }

    // Status debug
    if matches_keybind(key, &kb.debug_status) {
        return Some(Action::ToggleStatusDebug);
    }

    match key.code {
        KeyCode::Char('T') => {
            let selected_id = state.selected_agent_id();
            selected_id
                .filter(|id| {
                    state
                        .agents
                        .get(id)
                        .map(|a| a.pm_task_status.is_linked())
                        .unwrap_or(false)
                })
                .map(|id| Action::OpenTaskStatusDropdown { id })
        }
        KeyCode::Esc => Some(Action::ClearError),

        // Preview tab navigation
        KeyCode::Tab => Some(Action::NextPreviewTab),
        KeyCode::BackTab => Some(Action::PrevPreviewTab),

        // Dev server controls
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::RequestStartDevServer)
        }
        KeyCode::Char('S') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::RestartDevServer)
        }
        KeyCode::Char('C') if state.preview_tab == PreviewTab::DevServer => {
            Some(Action::ClearDevServerLogs)
        }
        KeyCode::Char('O') if state.preview_tab == PreviewTab::DevServer => {
            Some(Action::OpenDevServerInBrowser)
        }

        _ => None,
    }
}

/// Handle key events in input mode.
fn handle_input_mode_key(key: KeyCode, state: &AppState) -> Option<Action> {
    if matches!(state.input_mode, Some(InputMode::ConfirmDeleteTask)) {
        return match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => state
                .selected_agent_id()
                .map(|id| Action::DeleteAgentAndCompleteTask { id }),
            KeyCode::Char('n') | KeyCode::Char('N') => state
                .selected_agent_id()
                .map(|id| Action::DeleteAgent { id }),
            KeyCode::Esc => Some(Action::ExitInputMode),
            _ => None,
        };
    }

    if matches!(state.input_mode, Some(InputMode::ConfirmDeleteAsana)) {
        return match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => state
                .selected_agent_id()
                .map(|id| Action::DeleteAgentAndCompleteAsana { id }),
            KeyCode::Char('n') | KeyCode::Char('N') => state
                .selected_agent_id()
                .map(|id| Action::DeleteAgent { id }),
            KeyCode::Esc => Some(Action::ExitInputMode),
            _ => None,
        };
    }

    if matches!(state.input_mode, Some(InputMode::BrowseTasks)) {
        return match key {
            KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectTaskNext),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectTaskPrev),
            KeyCode::Char('a') => Some(Action::AssignSelectedTaskToAgent),
            KeyCode::Char('s') => Some(Action::ToggleSubtaskStatus),
            KeyCode::Char('r') => Some(Action::RefreshTaskList),
            KeyCode::Enter => Some(Action::CreateAgentFromSelectedTask),
            KeyCode::Left | KeyCode::Right => Some(Action::ToggleTaskExpand),
            KeyCode::Esc => Some(Action::ExitInputMode),
            _ => None,
        };
    }

    if matches!(state.input_mode, Some(InputMode::SelectTaskStatus)) {
        return match key {
            KeyCode::Char('j') | KeyCode::Down => Some(Action::TaskStatusDropdownNext),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::TaskStatusDropdownPrev),
            KeyCode::Enter => Some(Action::TaskStatusDropdownSelect),
            KeyCode::Esc => Some(Action::ExitInputMode),
            _ => None,
        };
    }

    if matches!(state.input_mode, Some(InputMode::SelectSubtaskStatus)) {
        return match key {
            KeyCode::Char('j') | KeyCode::Down => Some(Action::SubtaskStatusDropdownNext),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::SubtaskStatusDropdownPrev),
            KeyCode::Enter => {
                if let Some(ref dropdown) = state.subtask_status_dropdown {
                    Some(Action::SubtaskStatusDropdownSelect {
                        completed: dropdown.selected_index == 1,
                    })
                } else {
                    Some(Action::ExitInputMode)
                }
            }
            KeyCode::Esc => Some(Action::ExitInputMode),
            _ => None,
        };
    }

    let is_confirm_mode = matches!(
        state.input_mode,
        Some(InputMode::ConfirmDelete)
            | Some(InputMode::ConfirmMerge)
            | Some(InputMode::ConfirmPush)
    );

    if is_confirm_mode {
        // Confirmation modes only respond to y/n/Esc
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => Some(Action::SubmitInput),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Some(Action::ExitInputMode),
            _ => None,
        }
    } else {
        // Text input modes
        match key {
            KeyCode::Enter => Some(Action::SubmitInput),
            KeyCode::Esc => Some(Action::ExitInputMode),
            KeyCode::Backspace => {
                let mut new_input = state.input_buffer.clone();
                new_input.pop();
                Some(Action::UpdateInput(new_input))
            }
            KeyCode::Char(c) => {
                let mut new_input = state.input_buffer.clone();
                new_input.push(c);
                Some(Action::UpdateInput(new_input))
            }
            _ => None,
        }
    }
}

/// Handle key events in settings mode.
fn handle_settings_key(key: crossterm::event::KeyEvent, state: &AppState) -> Option<Action> {
    use grove::app::DropdownState;

    // Handle reset confirmation mode
    if state.settings.reset_confirmation.is_some() {
        return match key.code {
            KeyCode::Esc => Some(Action::SettingsCancelReset),
            KeyCode::Enter => Some(Action::SettingsConfirmReset),
            _ => None,
        };
    }

    // Handle prompt editing mode (multi-line text editor)
    if state.settings.editing_prompt {
        return match key.code {
            KeyCode::Esc => Some(Action::SettingsCancelSelection),
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    Some(Action::SettingsInputChar('\n'))
                } else {
                    Some(Action::SettingsPromptSave)
                }
            }
            KeyCode::Backspace => Some(Action::SettingsBackspace),
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::SettingsConfirmSelection)
            }
            KeyCode::Char(c) => Some(Action::SettingsInputChar(c)),
            _ => None,
        };
    }

    // Handle text editing mode
    if state.settings.editing_text {
        return match key.code {
            KeyCode::Esc => Some(Action::SettingsCancelSelection),
            KeyCode::Enter => Some(Action::SettingsConfirmSelection),
            KeyCode::Backspace => Some(Action::SettingsBackspace),
            KeyCode::Char(c) => Some(Action::SettingsInputChar(c)),
            _ => None,
        };
    }

    // Handle dropdown mode
    if let DropdownState::Open { .. } = &state.settings.dropdown {
        return match key.code {
            KeyCode::Esc => Some(Action::SettingsCancelSelection),
            KeyCode::Enter => Some(Action::SettingsConfirmSelection),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::SettingsDropdownPrev),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::SettingsDropdownNext),
            _ => None,
        };
    }

    // Handle file browser mode
    if state.settings.file_browser.active {
        return match key.code {
            KeyCode::Esc => Some(Action::SettingsCloseFileBrowser),
            KeyCode::Enter => Some(Action::FileBrowserToggle),
            KeyCode::Char(' ') => Some(Action::FileBrowserToggle),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::FileBrowserSelectPrev),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::FileBrowserSelectNext),
            KeyCode::Right => Some(Action::FileBrowserEnterDir),
            KeyCode::Left => Some(Action::FileBrowserGoParent),
            _ => None,
        };
    }

    // Handle keybind capture mode
    if state.settings.capturing_keybind.is_some() {
        return match key.code {
            KeyCode::Esc => Some(Action::SettingsCancelKeybindCapture),
            KeyCode::Char(c) => {
                let mut modifiers = Vec::new();
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    modifiers.push("Control".to_string());
                }
                if key.modifiers.contains(KeyModifiers::ALT) {
                    modifiers.push("Alt".to_string());
                }
                let key_char =
                    if c.is_ascii_alphabetic() && key.modifiers.contains(KeyModifiers::SHIFT) {
                        modifiers.push("Shift".to_string());
                        c.to_ascii_lowercase().to_string()
                    } else {
                        c.to_string()
                    };
                Some(Action::SettingsCaptureKeybind {
                    key: key_char,
                    modifiers,
                })
            }
            _ => {
                let key_name = match key.code {
                    KeyCode::Enter => "Enter",
                    KeyCode::Backspace => "Backspace",
                    KeyCode::Tab => "Tab",
                    KeyCode::Delete => "Delete",
                    KeyCode::Home => "Home",
                    KeyCode::End => "End",
                    KeyCode::PageUp => "PageUp",
                    KeyCode::PageDown => "PageDown",
                    KeyCode::Up => "Up",
                    KeyCode::Down => "Down",
                    KeyCode::Left => "Left",
                    KeyCode::Right => "Right",
                    KeyCode::Esc => "Esc",
                    KeyCode::F(n) => {
                        return Some(Action::SettingsCaptureKeybind {
                            key: format!("F{}", n),
                            modifiers: vec![],
                        })
                    }
                    _ => return None,
                };
                let mut modifiers = Vec::new();
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    modifiers.push("Control".to_string());
                }
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    modifiers.push("Shift".to_string());
                }
                if key.modifiers.contains(KeyModifiers::ALT) {
                    modifiers.push("Alt".to_string());
                }
                Some(Action::SettingsCaptureKeybind {
                    key: key_name.to_string(),
                    modifiers,
                })
            }
        };
    }

    // Normal settings navigation
    match key.code {
        KeyCode::Esc => Some(Action::SettingsClose),
        KeyCode::Char('c') => Some(Action::SettingsSave),
        KeyCode::Tab => Some(Action::SettingsSwitchSection),
        KeyCode::BackTab => Some(Action::SettingsSwitchSectionBack),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::SettingsSelectPrev),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::SettingsSelectNext),
        KeyCode::Enter => {
            if let Some(btn) = state.settings.current_action_button() {
                use grove::app::ActionButtonType;
                match btn {
                    ActionButtonType::ResetTab => Some(Action::SettingsRequestReset {
                        reset_type: grove::app::ResetType::CurrentTab,
                    }),
                    ActionButtonType::ResetAll => Some(Action::SettingsRequestReset {
                        reset_type: grove::app::ResetType::AllSettings,
                    }),
                    ActionButtonType::SetupPm => Some(Action::SettingsSelectField),
                    ActionButtonType::SetupGit => Some(Action::OpenGitSetup),
                }
            } else {
                let field = state.settings.current_field();
                if field.is_keybind_field() {
                    Some(Action::SettingsStartKeybindCapture)
                } else {
                    Some(Action::SettingsSelectField)
                }
            }
        }
        _ => None,
    }
}

fn handle_pm_setup_key(
    key: crossterm::event::KeyEvent,
    pm_setup: &grove::app::state::PmSetupState,
    provider: grove::app::config::ProjectMgmtProvider,
) -> Option<Action> {
    use grove::app::state::PmSetupStep;
    let is_linear = matches!(provider, grove::app::config::ProjectMgmtProvider::Linear);

    if pm_setup.dropdown_open {
        return match key.code {
            KeyCode::Esc => Some(Action::PmSetupToggleDropdown),
            KeyCode::Enter => Some(Action::PmSetupConfirmDropdown),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::PmSetupDropdownPrev),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::PmSetupDropdownNext),
            _ => None,
        };
    }

    match pm_setup.step {
        PmSetupStep::Token => match key.code {
            KeyCode::Esc => Some(Action::ClosePmSetup),
            KeyCode::Enter => Some(Action::PmSetupNextStep),
            _ => None,
        },
        PmSetupStep::Workspace => {
            if pm_setup.field_index > 0 && pm_setup.advanced_expanded {
                match key.code {
                    KeyCode::Esc => Some(Action::PmSetupPrevStep),
                    KeyCode::Char('c') => {
                        if is_linear {
                            Some(Action::PmSetupComplete)
                        } else {
                            Some(Action::PmSetupNextStep)
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => Some(Action::PmSetupNavigatePrev),
                    KeyCode::Down | KeyCode::Char('j') => Some(Action::PmSetupNavigateNext),
                    KeyCode::Backspace => Some(Action::PmSetupBackspace),
                    KeyCode::Char(ch) if ch != 'c' && ch != 'j' && ch != 'k' => {
                        Some(Action::PmSetupInputChar(ch))
                    }
                    KeyCode::Left => Some(Action::PmSetupToggleAdvanced),
                    _ => None,
                }
            } else {
                match key.code {
                    KeyCode::Esc => Some(Action::PmSetupPrevStep),
                    KeyCode::Enter => {
                        if pm_setup.field_index == 0 && !pm_setup.teams.is_empty() {
                            Some(Action::PmSetupToggleDropdown)
                        } else if is_linear {
                            Some(Action::PmSetupComplete)
                        } else {
                            None
                        }
                    }
                    KeyCode::Char('c') => {
                        if is_linear {
                            Some(Action::PmSetupComplete)
                        } else {
                            Some(Action::PmSetupNextStep)
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => Some(Action::PmSetupNavigatePrev),
                    KeyCode::Down | KeyCode::Char('j') => Some(Action::PmSetupNavigateNext),
                    KeyCode::Right => Some(Action::PmSetupToggleAdvanced),
                    KeyCode::Left => Some(Action::PmSetupToggleAdvanced),
                    _ => None,
                }
            }
        }
        PmSetupStep::Project => {
            if is_linear {
                // Linear skips Project step, treat as completion
                match key.code {
                    KeyCode::Esc => Some(Action::PmSetupPrevStep),
                    KeyCode::Char('c') => Some(Action::PmSetupComplete),
                    _ => None,
                }
            } else if pm_setup.field_index > 0 && pm_setup.advanced_expanded {
                match key.code {
                    KeyCode::Esc => Some(Action::PmSetupPrevStep),
                    KeyCode::Char('c') => Some(Action::PmSetupComplete),
                    KeyCode::Up | KeyCode::Char('k') => Some(Action::PmSetupNavigatePrev),
                    KeyCode::Down | KeyCode::Char('j') => Some(Action::PmSetupNavigateNext),
                    KeyCode::Backspace => Some(Action::PmSetupBackspace),
                    KeyCode::Char(ch) if ch != 'c' && ch != 'j' && ch != 'k' => {
                        Some(Action::PmSetupInputChar(ch))
                    }
                    KeyCode::Left => Some(Action::PmSetupToggleAdvanced),
                    _ => None,
                }
            } else {
                match key.code {
                    KeyCode::Esc => Some(Action::PmSetupPrevStep),
                    KeyCode::Enter => {
                        if pm_setup.field_index == 0 && !pm_setup.teams.is_empty() {
                            Some(Action::PmSetupToggleDropdown)
                        } else {
                            None
                        }
                    }
                    KeyCode::Char('c') => Some(Action::PmSetupComplete),
                    KeyCode::Up | KeyCode::Char('k') => Some(Action::PmSetupNavigatePrev),
                    KeyCode::Down | KeyCode::Char('j') => Some(Action::PmSetupNavigateNext),
                    KeyCode::Right => Some(Action::PmSetupToggleAdvanced),
                    KeyCode::Left => Some(Action::PmSetupToggleAdvanced),
                    _ => None,
                }
            }
        }
        PmSetupStep::Advanced => {
            if is_linear {
                // Linear skips Advanced step, treat as completion
                match key.code {
                    KeyCode::Esc => Some(Action::PmSetupPrevStep),
                    KeyCode::Char('c') => Some(Action::PmSetupComplete),
                    _ => None,
                }
            } else {
                match key.code {
                    KeyCode::Esc => Some(Action::PmSetupPrevStep),
                    KeyCode::Char('c') => Some(Action::PmSetupComplete),
                    _ => None,
                }
            }
        }
    }
}

fn handle_git_setup_key(
    key: crossterm::event::KeyEvent,
    git_setup: &grove::app::state::GitSetupState,
    provider: grove::app::config::GitProvider,
) -> Option<Action> {
    use grove::app::state::GitSetupStep;

    if git_setup.loading {
        return None;
    }

    if git_setup.editing_text {
        return match key.code {
            KeyCode::Esc => Some(Action::GitSetupCancelEdit),
            KeyCode::Enter => Some(Action::GitSetupConfirmEdit),
            KeyCode::Backspace => Some(Action::GitSetupBackspace),
            KeyCode::Char(c) => Some(Action::GitSetupInputChar(c)),
            _ => None,
        };
    }

    match git_setup.step {
        GitSetupStep::Token => match key.code {
            KeyCode::Esc => Some(Action::CloseGitSetup),
            KeyCode::Enter => Some(Action::GitSetupNextStep),
            _ => None,
        },
        GitSetupStep::Repository => {
            // Calculate max field based on provider
            // GitLab: 0=Owner, 1=Repo, 2=ProjectID, 3=BaseURL(if advanced)
            // GitHub: 0=Owner, 1=Repo, 2=BaseURL(if advanced)
            // Codeberg: 0=Owner, 1=Repo, 2=CI Provider, 3=BaseURL(if advanced)
            let max_field = if git_setup.advanced_expanded {
                match provider {
                    grove::app::config::GitProvider::GitLab => 3,
                    grove::app::config::GitProvider::GitHub => 2,
                    grove::app::config::GitProvider::Codeberg => 3,
                }
            } else {
                match provider {
                    grove::app::config::GitProvider::GitLab => 2,
                    grove::app::config::GitProvider::GitHub => 1,
                    grove::app::config::GitProvider::Codeberg => 3,
                }
            };

            // Check if current field is the dropdown (field_index == 2 for Codeberg CI provider)
            let is_dropdown_field = matches!(provider, grove::app::config::GitProvider::Codeberg)
                && git_setup.field_index == 2
                && !git_setup.editing_text;

            match key.code {
                KeyCode::Esc => {
                    if git_setup.dropdown_open {
                        Some(Action::GitSetupCloseDropdown)
                    } else {
                        Some(Action::GitSetupPrevStep)
                    }
                }
                KeyCode::Enter => {
                    if git_setup.dropdown_open {
                        Some(Action::GitSetupConfirmDropdown)
                    } else if is_dropdown_field {
                        Some(Action::GitSetupToggleDropdown)
                    } else if git_setup.field_index < max_field {
                        Some(Action::GitSetupStartEdit)
                    } else {
                        Some(Action::GitSetupComplete)
                    }
                }
                KeyCode::Char('a') if !git_setup.advanced_expanded && !git_setup.dropdown_open => {
                    Some(Action::GitSetupToggleAdvanced)
                }
                KeyCode::Char('f')
                    if matches!(provider, grove::app::config::GitProvider::GitLab)
                        && git_setup.project_id.is_empty()
                        && !git_setup.owner.is_empty()
                        && !git_setup.repo.is_empty()
                        && grove::app::Config::gitlab_token().is_some() =>
                {
                    Some(Action::GitSetupFetchProjectId)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if git_setup.dropdown_open {
                        Some(Action::GitSetupDropdownPrev)
                    } else {
                        Some(Action::GitSetupNavigatePrev)
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if git_setup.dropdown_open {
                        Some(Action::GitSetupDropdownNext)
                    } else {
                        Some(Action::GitSetupNavigateNext)
                    }
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if git_setup.dropdown_open {
                        None
                    } else {
                        Some(Action::GitSetupComplete)
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    if git_setup.dropdown_open {
                        None
                    } else if git_setup.advanced_expanded {
                        Some(Action::GitSetupToggleAdvanced)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        GitSetupStep::Advanced => match key.code {
            KeyCode::Esc => Some(Action::GitSetupPrevStep),
            KeyCode::Enter => Some(Action::GitSetupComplete),
            KeyCode::Left | KeyCode::Char('h') => Some(Action::GitSetupPrevStep),
            _ => None,
        },
    }
}

/// Process an action and update state.
#[allow(clippy::too_many_arguments)]
async fn process_action(
    action: Action,
    state: &mut AppState,
    agent_manager: &Arc<AgentManager>,
    gitlab_client: &Arc<OptionalGitLabClient>,
    github_client: &Arc<OptionalGitHubClient>,
    codeberg_client: &Arc<OptionalCodebergClient>,
    asana_client: &Arc<OptionalAsanaClient>,
    notion_client: &Arc<OptionalNotionClient>,
    clickup_client: &Arc<OptionalClickUpClient>,
    airtable_client: &Arc<OptionalAirtableClient>,
    linear_client: &Arc<OptionalLinearClient>,
    _storage: &SessionStorage,
    action_tx: &mpsc::UnboundedSender<Action>,
    agent_watch_tx: &watch::Sender<HashSet<Uuid>>,
    branch_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
    selected_watch_tx: &watch::Sender<Option<Uuid>>,
    asana_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
    notion_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
    clickup_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
    airtable_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
    linear_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
    devserver_manager: &Arc<tokio::sync::Mutex<DevServerManager>>,
) -> Result<bool> {
    match action {
        Action::Quit => {
            let mut manager = devserver_manager.lock().await;
            let _ = manager.stop_all().await;
            state.running = false;
            return Ok(true);
        }

        // Navigation (clear any lingering messages)
        Action::SelectNext => {
            state.toast = None;
            state.select_next();
            let new_selected = state.selected_agent_id();
            tracing::info!("DEBUG SelectNext: new_selected={:?}", new_selected);
            match selected_watch_tx.send(new_selected) {
                Ok(_) => tracing::info!("DEBUG SelectNext: send succeeded"),
                Err(e) => tracing::error!("DEBUG SelectNext: send FAILED: {}", e),
            }
        }
        Action::SelectPrevious => {
            state.toast = None;
            state.select_previous();
            let new_selected = state.selected_agent_id();
            tracing::info!("DEBUG SelectPrevious: new_selected={:?}", new_selected);
            match selected_watch_tx.send(new_selected) {
                Ok(_) => tracing::info!("DEBUG SelectPrevious: send succeeded"),
                Err(e) => tracing::error!("DEBUG SelectPrevious: send FAILED: {}", e),
            }
        }
        Action::SelectFirst => {
            state.toast = None;
            state.select_first();
            let _ = selected_watch_tx.send(state.selected_agent_id());
        }
        Action::SelectLast => {
            state.toast = None;
            state.select_last();
            let _ = selected_watch_tx.send(state.selected_agent_id());
        }

        // Agent lifecycle
        Action::CreateAgent { name, branch, task } => {
            state.log_info(format!("Creating agent '{}' on branch '{}'", name, branch));
            let ai_agent = state.config.global.ai_agent.clone();
            let worktree_symlinks = state
                .settings
                .repo_config
                .dev_server
                .worktree_symlinks
                .clone();
            match agent_manager.create_agent(&name, &branch, &ai_agent, &worktree_symlinks) {
                Ok(mut agent) => {
                    state.log_info(format!("Agent '{}' created successfully", agent.name));

                    if let Some(ref task_item) = task {
                        let pm_status = match state.settings.repo_config.project_mgmt.provider {
                            ProjectMgmtProvider::Asana => {
                                ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted {
                                    gid: task_item.id.clone(),
                                    name: task_item.name.clone(),
                                    url: task_item.url.clone(),
                                    is_subtask: task_item.is_subtask(),
                                    status_name: task_item.status_name.clone(),
                                })
                            }
                            ProjectMgmtProvider::Notion => {
                                ProjectMgmtTaskStatus::Notion(NotionTaskStatus::Linked {
                                    page_id: task_item.id.clone(),
                                    name: task_item.name.clone(),
                                    url: task_item.url.clone(),
                                    status_option_id: String::new(),
                                    status_name: task_item.status_name.clone(),
                                })
                            }
                            ProjectMgmtProvider::Clickup => {
                                ProjectMgmtTaskStatus::ClickUp(ClickUpTaskStatus::NotStarted {
                                    id: task_item.id.clone(),
                                    name: task_item.name.clone(),
                                    url: task_item.url.clone(),
                                    status: task_item.status_name.clone(),
                                    is_subtask: task_item.is_subtask(),
                                })
                            }
                            ProjectMgmtProvider::Airtable => {
                                ProjectMgmtTaskStatus::Airtable(AirtableTaskStatus::NotStarted {
                                    id: task_item.id.clone(),
                                    name: task_item.name.clone(),
                                    url: task_item.url.clone(),
                                    is_subtask: task_item.is_subtask(),
                                })
                            }
                            ProjectMgmtProvider::Linear => {
                                let identifier = task_item
                                    .name
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or("")
                                    .to_string();
                                ProjectMgmtTaskStatus::Linear(LinearTaskStatus::NotStarted {
                                    id: task_item.id.clone(),
                                    identifier,
                                    name: task_item.name.clone(),
                                    status_name: task_item.status_name.clone(),
                                    url: task_item.url.clone(),
                                    is_subtask: task_item.is_subtask(),
                                })
                            }
                        };
                        agent.pm_task_status = pm_status;
                        state.log_info(format!("Linked task '{}' to agent", task_item.name));
                    }

                    state.add_agent(agent);
                    state.select_last();
                    state.toast = None;
                    // Notify polling tasks of new agent
                    let _ = agent_watch_tx.send(state.agents.keys().cloned().collect());
                    let _ = branch_watch_tx.send(
                        state
                            .agents
                            .values()
                            .map(|a| (a.id, a.branch.clone()))
                            .collect(),
                    );
                    let _ = selected_watch_tx.send(state.selected_agent_id());
                }
                Err(e) => {
                    state.log_error(format!("Failed to create agent: {}", e));
                    state.toast = Some(Toast::new(
                        format!("Failed to create agent: {}", e),
                        ToastLevel::Error,
                    ));
                }
            }
        }

        Action::DeleteAgent { id } => {
            // Clear input mode if triggered directly from ConfirmDeleteAsana (n key)
            if state.is_input_mode() {
                state.exit_input_mode();
            }
            let agent_info = state.agents.get(&id).map(|a| {
                (
                    a.name.clone(),
                    a.tmux_session.clone(),
                    a.worktree_path.clone(),
                )
            });

            if let Some((name, tmux_session, worktree_path)) = agent_info {
                state.log_info(format!("Deleting agent '{}'...", name));
                state.loading_message = Some(format!("Deleting '{}'...", name));

                let tx = action_tx.clone();
                let name_clone = name.clone();
                let repo_path = state.repo_path.clone();
                tokio::spawn(async move {
                    // Kill tmux session
                    let session = grove::tmux::TmuxSession::new(&tmux_session);
                    if session.exists() {
                        let _ = session.kill();
                    }

                    // Remove worktree
                    if std::path::Path::new(&worktree_path).exists() {
                        let _ = std::process::Command::new("git")
                            .args([
                                "-C",
                                &repo_path,
                                "worktree",
                                "remove",
                                "--force",
                                &worktree_path,
                            ])
                            .output();
                        let _ = std::process::Command::new("git")
                            .args(["-C", &repo_path, "worktree", "prune"])
                            .output();
                    }

                    let _ = tx.send(Action::DeleteAgentComplete {
                        id,
                        success: true,
                        message: format!("Deleted '{}'", name_clone),
                    });
                });
            }
        }

        Action::DeleteAgentAndCompleteAsana { id } => {
            state.exit_input_mode();

            // Complete the Asana task first (move to Done + mark complete)
            if let Some(agent) = state.agents.get(&id) {
                if let Some(task_gid) = agent.asana_task_status.gid() {
                    let gid = task_gid.to_string();
                    let client = Arc::clone(asana_client);
                    let done_gid = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .asana
                        .done_section_gid
                        .clone();
                    tokio::spawn(async move {
                        let _ = client.move_to_done(&gid, done_gid.as_deref()).await;
                        let _ = client.complete_task(&gid).await;
                    });
                    state.log_info("Moving Asana task to Done".to_string());
                }
            }

            // Then delete the agent (reuse existing logic)
            action_tx.send(Action::DeleteAgent { id })?;
        }

        Action::AttachToAgent { .. } => {
            // Handled in main loop for terminal access
        }

        Action::AttachToDevServer { .. } => {
            // Handled in main loop for terminal access
        }

        Action::DetachFromAgent => {
            // Handled in main loop
        }

        // Status updates
        Action::UpdateAgentStatus {
            id,
            status,
            status_reason,
        } => {
            if let Some(agent) = state.agents.get_mut(&id) {
                if matches!(agent.status, grove::agent::AgentStatus::Paused) {
                    return Ok(false);
                }

                let old_label = agent.status.label();
                let new_label = status.label();
                let name = agent.name.clone();
                let changed = old_label != new_label;

                agent.set_status(status);
                if let Some(reason) = status_reason {
                    agent.status_reason = Some(reason);
                }
                if changed {
                    state.log_debug(format!("Agent '{}': {} -> {}", name, old_label, new_label));
                }
            }
        }

        Action::UpdateAgentOutput { id, output } => {
            if let Some(agent) = state.agents.get_mut(&id) {
                agent.update_output(output, state.config.ui.output_buffer_lines);
            }
        }

        Action::SetAgentNote { id, note } => {
            if let Some(agent) = state.agents.get_mut(&id) {
                agent.custom_note = note;
            }
        }

        // Git operations
        Action::CheckoutBranch { id: _ } => {
            // Deprecated - use PauseAgent instead
        }

        Action::PauseAgent { id } => {
            // Get agent info before spawning background task
            let agent_info = state.agents.get(&id).map(|a| {
                (
                    a.name.clone(),
                    a.branch.clone(),
                    a.worktree_path.clone(),
                    a.tmux_session.clone(),
                )
            });

            if let Some((name, branch, worktree_path, _tmux_session)) = agent_info {
                state.log_info(format!("Pausing agent '{}'...", name));
                state.loading_message = Some(format!("Pausing '{}'...", name));

                // Spawn background task
                let tx = action_tx.clone();
                let name_clone = name.clone();
                let branch_clone = branch.clone();
                tokio::spawn(async move {
                    // 1. Commit any uncommitted changes
                    let commit_result = std::process::Command::new("git")
                        .args(["-C", &worktree_path, "add", "-A"])
                        .output();
                    if commit_result.is_ok() {
                        let _ = std::process::Command::new("git")
                            .args([
                                "-C",
                                &worktree_path,
                                "commit",
                                "-m",
                                &format!("[GROVE] {}", name_clone),
                            ])
                            .output();
                    }

                    // 2. DON'T kill tmux session - just leave it running (preserves Claude context)
                    // The tmux session stays alive but detached

                    // 3. DON'T remove worktree - keep it so agent stays functional
                    // The worktree stays intact so the agent can continue working

                    // 4. Get HEAD commit SHA for checkout command
                    let head_sha = std::process::Command::new("git")
                        .args(["-C", &worktree_path, "rev-parse", "HEAD"])
                        .output()
                        .ok()
                        .and_then(|output| {
                            if output.status.success() {
                                String::from_utf8(output.stdout).ok()
                            } else {
                                None
                            }
                        })
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|| branch_clone.clone());

                    // 5. Copy detach checkout command to clipboard
                    let checkout_cmd = format!("git checkout --detach {}", head_sha);
                    let clipboard_result =
                        Clipboard::new().and_then(|mut c| c.set_text(&checkout_cmd));
                    let message = if clipboard_result.is_ok() {
                        "Checkout command copied. Press 'r' to resume.".to_string()
                    } else {
                        format!("Paused '{}'. Press 'r' to resume.", name_clone)
                    };

                    // Send completion
                    let _ = tx.send(Action::PauseAgentComplete {
                        id,
                        success: true,
                        message,
                    });
                });
            }
        }

        Action::ResumeAgent { id } => {
            let agent_info = state.agents.get(&id).map(|a| {
                (
                    a.name.clone(),
                    a.branch.clone(),
                    a.worktree_path.clone(),
                    a.tmux_session.clone(),
                )
            });

            if let Some((name, branch, worktree_path, tmux_session)) = agent_info {
                state.log_info(format!("Resuming agent '{}'...", name));
                state.loading_message = Some(format!("Resuming '{}'...", name));

                let tx = action_tx.clone();
                let name_clone = name.clone();
                let ai_agent = state.config.global.ai_agent.clone();
                let repo_path = state.repo_path.clone();
                let worktree_symlinks = state
                    .settings
                    .repo_config
                    .dev_server
                    .worktree_symlinks
                    .clone();
                let worktree_base = state.worktree_base.clone();
                tokio::spawn(async move {
                    // Check if worktree already exists
                    let worktree_exists = std::path::Path::new(&worktree_path).exists();

                    if !worktree_exists {
                        // Recreate worktree
                        let worktree_result = std::process::Command::new("git")
                            .args(["worktree", "add", &worktree_path, &branch])
                            .output();

                        if let Err(e) = worktree_result {
                            let _ = tx.send(Action::ResumeAgentComplete {
                                id,
                                success: false,
                                message: format!("Failed to recreate worktree: {}", e),
                            });
                            return;
                        }

                        let worktree_output = worktree_result.unwrap();
                        if !worktree_output.status.success() {
                            let stderr = String::from_utf8_lossy(&worktree_output.stderr);
                            let message = if stderr.contains("already checked out") {
                                "Cannot resume: branch is checked out elsewhere. Switch branches first.".to_string()
                            } else {
                                format!("Failed to resume: {}", stderr)
                            };
                            let _ = tx.send(Action::ResumeAgentComplete {
                                id,
                                success: false,
                                message,
                            });
                            return;
                        }

                        // Create symlinks for newly created worktree
                        let worktree = grove::git::Worktree::new(&repo_path, worktree_base);
                        if let Err(e) = worktree.create_symlinks(&worktree_path, &worktree_symlinks)
                        {
                            // Log but don't fail - symlinks are optional
                            eprintln!("Warning: Failed to create symlinks: {}", e);
                        }
                    }

                    let session = grove::tmux::TmuxSession::new(&tmux_session);
                    if !session.exists() {
                        if let Err(e) = session.create(&worktree_path, ai_agent.command()) {
                            let _ = tx.send(Action::ResumeAgentComplete {
                                id,
                                success: false,
                                message: format!("Failed to create tmux session: {}", e),
                            });
                            return;
                        }
                    }
                    // If session exists, Claude context is preserved!

                    let _ = tx.send(Action::ResumeAgentComplete {
                        id,
                        success: true,
                        message: format!("Resumed '{}'", name_clone),
                    });
                });
            }
        }

        Action::MergeMain { id } => {
            let main_branch = state.settings.repo_config.git.main_branch.clone();
            let prompt = state
                .settings
                .repo_config
                .prompts
                .get_merge_prompt(&main_branch);
            let agent_info = state
                .agents
                .get(&id)
                .map(|a| (a.name.clone(), a.tmux_session.clone()));

            if let Some((name, tmux_session)) = agent_info {
                let session = grove::tmux::TmuxSession::new(&tmux_session);
                match session.send_keys(&prompt) {
                    Ok(()) => {
                        if let Some(agent) = state.agents.get_mut(&id) {
                            agent.custom_note = Some("merging main...".to_string());
                        }
                        state.log_info(format!("Sent merge request to agent '{}'", name));
                        state.show_success(format!("Sent merge {} request to Claude", main_branch));
                    }
                    Err(e) => {
                        state.log_error(format!("Failed to send merge request: {}", e));
                        state.show_error(format!("Failed to send merge request: {}", e));
                    }
                }
            }
        }

        Action::PushBranch { id } => {
            let agent_info = state
                .agents
                .get(&id)
                .map(|a| (a.name.clone(), a.tmux_session.clone()));

            if let Some((name, tmux_session)) = agent_info {
                let session = grove::tmux::TmuxSession::new(&tmux_session);
                let agent_type = state.config.global.ai_agent.clone();
                let push_cmd = agent_type.push_command();
                let push_prompt = state
                    .settings
                    .repo_config
                    .prompts
                    .get_push_prompt(&agent_type);

                let mut success = false;

                if let Some(cmd) = push_cmd {
                    match session.send_keys(cmd) {
                        Ok(()) => {
                            state.log_info(format!("Sent {} to agent '{}'", cmd, name));
                            success = true;
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to send {}: {}", cmd, e));
                            state.show_error(format!("Failed to send {}: {}", cmd, e));
                        }
                    }
                }

                if let Some(prompt) = push_prompt {
                    match session.send_keys(&prompt) {
                        Ok(()) => {
                            state.log_info(format!("Sent push prompt to agent '{}'", name));
                            success = true;
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to send push prompt: {}", e));
                            state.show_error(format!("Failed to send push prompt: {}", e));
                        }
                    }
                }

                if success {
                    if let Some(agent) = state.agents.get_mut(&id) {
                        agent.custom_note = Some("pushing...".to_string());
                    }
                    state.show_success(format!(
                        "Sent push command to {}",
                        agent_type.display_name()
                    ));
                }
            }
        }

        Action::FetchRemote { id } => {
            if let Some(agent) = state.agents.get(&id) {
                let git_sync = GitSync::new(&agent.worktree_path);
                if let Err(e) = git_sync.fetch() {
                    state.show_error(format!("Fetch failed: {}", e));
                }
            }
        }

        Action::RequestSummary { id } => {
            let prompt = state
                .settings
                .repo_config
                .prompts
                .get_summary_prompt()
                .to_string();
            let agent_info = state
                .agents
                .get(&id)
                .map(|a| (a.name.clone(), a.tmux_session.clone()));

            if let Some((name, tmux_session)) = agent_info {
                let session = grove::tmux::TmuxSession::new(&tmux_session);
                match session.send_keys(&prompt) {
                    Ok(()) => {
                        if let Some(agent) = state.agents.get_mut(&id) {
                            agent.summary_requested = true;
                            agent.custom_note = Some("summary...".to_string());
                        }
                        state.log_info(format!("Requested summary from agent '{}'", name));
                        state.show_success("Requested work summary from Claude");
                    }
                    Err(e) => {
                        state.log_error(format!("Failed to request summary: {}", e));
                        state.show_error(format!("Failed to request summary: {}", e));
                    }
                }
            }
        }

        Action::UpdateGitStatus { id, status } => {
            if let Some(agent) = state.agents.get_mut(&id) {
                agent.git_status = Some(status);
            }
        }

        // GitLab operations
        Action::UpdateMrStatus { id, status } => {
            // Check current state and extract needed data before mutable borrow
            let should_log = state.agents.get(&id).and_then(|agent| {
                let was_none = matches!(
                    agent.mr_status,
                    grove::core::git_providers::gitlab::MergeRequestStatus::None
                );
                let is_open = matches!(
                    &status,
                    grove::core::git_providers::gitlab::MergeRequestStatus::Open { .. }
                );
                if was_none && is_open {
                    if let grove::core::git_providers::gitlab::MergeRequestStatus::Open {
                        iid,
                        url,
                        ..
                    } = &status
                    {
                        Some((agent.name.clone(), *iid, url.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            // Auto-update note based on MR transitions
            let auto_note = state.agents.get(&id).and_then(|agent| {
                let was_none = matches!(
                    agent.mr_status,
                    grove::core::git_providers::gitlab::MergeRequestStatus::None
                );
                let was_pushing = agent.custom_note.as_deref() == Some("pushing...");
                let was_merging = agent.custom_note.as_deref() == Some("merging main...");

                match &status {
                    grove::core::git_providers::gitlab::MergeRequestStatus::Open { .. }
                        if was_none || was_pushing =>
                    {
                        Some("pushed".to_string())
                    }
                    grove::core::git_providers::gitlab::MergeRequestStatus::Merged { .. } => {
                        Some("merged".to_string())
                    }
                    _ if was_merging => {
                        // If we had "merging main..." and status updates, merge is done
                        Some("main merged".to_string())
                    }
                    _ => None,
                }
            });

            // Now do the mutable borrow to update
            if let Some(agent) = state.agents.get_mut(&id) {
                agent.mr_status = status;
                if let Some(note) = auto_note {
                    agent.custom_note = Some(note);
                }
            }

            // Log after mutation is done
            if let Some((name, iid, url)) = should_log {
                state.log_info(format!("MR !{} detected for '{}'", iid, name));
                state.show_success(format!("MR !{}: {}", iid, url));
            }
        }

        Action::OpenMrInBrowser { id } => {
            if let Some(agent) = state.agents.get(&id) {
                if let Some(url) = agent.mr_status.url() {
                    match open::that(url) {
                        Ok(_) => {
                            state.log_info("Opening MR in browser".to_string());
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to open browser: {}", e));
                            state.show_error(format!("Failed to open browser: {}", e));
                        }
                    }
                } else {
                    state.show_error("No MR available for this agent");
                }
            }
        }
        Action::OpenInEditor { .. } => {
            // Handled in main loop for terminal access
        }

        // GitHub operations
        Action::UpdatePrStatus { id, status } => {
            let should_log = state.agents.get(&id).and_then(|agent| {
                let was_none = matches!(
                    agent.pr_status,
                    grove::core::git_providers::github::PullRequestStatus::None
                );
                let is_open = matches!(
                    &status,
                    grove::core::git_providers::github::PullRequestStatus::Open { .. }
                );
                if was_none && is_open {
                    if let grove::core::git_providers::github::PullRequestStatus::Open {
                        number,
                        url,
                        ..
                    } = &status
                    {
                        Some((agent.name.clone(), *number, url.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            let auto_note = state.agents.get(&id).and_then(|agent| {
                let was_none = matches!(
                    agent.pr_status,
                    grove::core::git_providers::github::PullRequestStatus::None
                );
                let was_pushing = agent.custom_note.as_deref() == Some("pushing...");
                let was_merging = agent.custom_note.as_deref() == Some("merging main...");

                match &status {
                    grove::core::git_providers::github::PullRequestStatus::Open { .. }
                        if was_none || was_pushing =>
                    {
                        Some("pushed".to_string())
                    }
                    grove::core::git_providers::github::PullRequestStatus::Merged { .. } => {
                        Some("merged".to_string())
                    }
                    _ if was_merging => Some("main merged".to_string()),
                    _ => None,
                }
            });

            if let Some(agent) = state.agents.get_mut(&id) {
                agent.pr_status = status;
                if let Some(note) = auto_note {
                    agent.custom_note = Some(note);
                }
            }

            if let Some((name, number, url)) = should_log {
                state.log_info(format!("PR #{} detected for '{}'", number, name));
                state.show_success(format!("PR #{}: {}", number, url));
            }
        }

        Action::OpenPrInBrowser { id } => {
            if let Some(agent) = state.agents.get(&id) {
                if let Some(url) = agent.pr_status.url() {
                    match open::that(url) {
                        Ok(_) => {
                            state.log_info("Opening PR in browser".to_string());
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to open browser: {}", e));
                            state.show_error(format!("Failed to open browser: {}", e));
                        }
                    }
                } else {
                    state.show_error("No PR available for this agent");
                }
            }
        }

        // Codeberg operations
        Action::UpdateCodebergPrStatus { id, status } => {
            let should_log = state.agents.get(&id).and_then(|agent| {
                let was_none = matches!(
                    agent.codeberg_pr_status,
                    grove::core::git_providers::codeberg::PullRequestStatus::None
                );
                let is_open = matches!(
                    &status,
                    grove::core::git_providers::codeberg::PullRequestStatus::Open { .. }
                );
                if was_none && is_open {
                    if let grove::core::git_providers::codeberg::PullRequestStatus::Open {
                        number,
                        url,
                        ..
                    } = &status
                    {
                        Some((agent.name.clone(), *number, url.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            if let Some(agent) = state.agents.get_mut(&id) {
                agent.codeberg_pr_status = status;
            }

            if let Some((name, number, url)) = should_log {
                state.log_info(format!("Codeberg PR #{} detected for '{}'", number, name));
                state.show_success(format!("PR #{}: {}", number, url));
            }
        }

        Action::OpenCodebergPrInBrowser { id } => {
            if let Some(agent) = state.agents.get(&id) {
                if let Some(url) = agent.codeberg_pr_status.url() {
                    match open::that(url) {
                        Ok(_) => {
                            state.log_info("Opening Codeberg PR in browser".to_string());
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to open browser: {}", e));
                            state.show_error(format!("Failed to open browser: {}", e));
                        }
                    }
                } else {
                    state.show_error("No Codeberg PR available for this agent");
                }
            }
        }

        // Asana operations
        Action::AssignAsanaTask { id, url_or_gid } => {
            let gid = parse_asana_task_gid(&url_or_gid);
            let client = Arc::clone(asana_client);
            let tx = action_tx.clone();
            tokio::spawn(async move {
                match client.get_task(&gid).await {
                    Ok(task) => {
                        let url = task
                            .permalink_url
                            .unwrap_or_else(|| format!("https://app.asana.com/0/0/{}/f", task.gid));
                        let is_subtask = task.parent.is_some();
                        let status = if task.completed {
                            AsanaTaskStatus::Completed {
                                gid: task.gid,
                                name: task.name,
                                is_subtask,
                                status_name: "Complete".to_string(),
                            }
                        } else {
                            AsanaTaskStatus::NotStarted {
                                gid: task.gid,
                                name: task.name,
                                url,
                                is_subtask,
                                status_name: "Not Started".to_string(),
                            }
                        };
                        let _ = tx.send(Action::UpdateAsanaTaskStatus { id, status });
                    }
                    Err(e) => {
                        let status = AsanaTaskStatus::Error {
                            gid,
                            message: e.to_string(),
                        };
                        let _ = tx.send(Action::UpdateAsanaTaskStatus { id, status });
                    }
                }
            });
        }

        Action::UpdateAsanaTaskStatus { id, status } => {
            let log_msg = match &status {
                AsanaTaskStatus::NotStarted { name, .. } => {
                    Some(format!("Asana task '{}' linked", name))
                }
                AsanaTaskStatus::InProgress { name, .. } => {
                    Some(format!("Asana task '{}' in progress", name))
                }
                AsanaTaskStatus::Completed { name, .. } => {
                    Some(format!("Asana task '{}' completed", name))
                }
                AsanaTaskStatus::Error { message, .. } => Some(format!("Asana error: {}", message)),
                AsanaTaskStatus::None => None,
            };
            if let Some(agent) = state.agents.get_mut(&id) {
                agent.pm_task_status = ProjectMgmtTaskStatus::Asana(status);
            }
            if let Some(msg) = log_msg {
                state.log_info(&msg);
                state.show_info(msg);
            }
            let asana_tasks: Vec<(Uuid, String)> = state
                .agents
                .values()
                .filter_map(|a| {
                    a.pm_task_status
                        .as_asana()
                        .and_then(|s| s.gid().map(|gid| (a.id, gid.to_string())))
                })
                .collect();
            let _ = asana_watch_tx.send(asana_tasks);
        }

        Action::OpenAsanaInBrowser { id } => {
            if let Some(agent) = state.agents.get(&id) {
                if let Some(url) = agent.pm_task_status.url() {
                    match open::that(url) {
                        Ok(_) => {
                            state.log_info("Opening Asana task in browser".to_string());
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to open browser: {}", e));
                            state.show_error(format!("Failed to open browser: {}", e));
                        }
                    }
                } else {
                    state.show_error("No Asana task linked to this agent");
                }
            }
        }

        Action::AssignProjectTask { id, url_or_id } => {
            let provider = state.settings.repo_config.project_mgmt.provider;
            match provider {
                ProjectMgmtProvider::Asana => {
                    let gid = parse_asana_task_gid(&url_or_id);
                    let client = Arc::clone(asana_client);
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        match client.get_task(&gid).await {
                            Ok(task) => {
                                let url = task.permalink_url.unwrap_or_else(|| {
                                    format!("https://app.asana.com/0/0/{}/f", task.gid)
                                });
                                let is_subtask = task.parent.is_some();
                                let status = if task.completed {
                                    AsanaTaskStatus::Completed {
                                        gid: task.gid,
                                        name: task.name,
                                        is_subtask,
                                        status_name: "Complete".to_string(),
                                    }
                                } else {
                                    AsanaTaskStatus::NotStarted {
                                        gid: task.gid,
                                        name: task.name,
                                        url,
                                        is_subtask,
                                        status_name: "Not Started".to_string(),
                                    }
                                };
                                let _ = tx.send(Action::UpdateProjectTaskStatus {
                                    id,
                                    status: ProjectMgmtTaskStatus::Asana(status),
                                });
                            }
                            Err(e) => {
                                let status = ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::Error {
                                    gid,
                                    message: e.to_string(),
                                });
                                let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                            }
                        }
                    });
                }
                ProjectMgmtProvider::Notion => {
                    let page_id = parse_notion_page_id(&url_or_id);
                    let client = Arc::clone(notion_client);
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        match client.get_page(&page_id).await {
                            Ok(page) => {
                                let status = NotionTaskStatus::Linked {
                                    page_id: page.id,
                                    name: page.name,
                                    url: page.url,
                                    status_option_id: page.status_id.unwrap_or_default(),
                                    status_name: page.status_name.unwrap_or_default(),
                                };
                                let _ = tx.send(Action::UpdateProjectTaskStatus {
                                    id,
                                    status: ProjectMgmtTaskStatus::Notion(status),
                                });
                            }
                            Err(e) => {
                                let status =
                                    ProjectMgmtTaskStatus::Notion(NotionTaskStatus::Error {
                                        page_id,
                                        message: e.to_string(),
                                    });
                                let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                            }
                        }
                    });
                }
                ProjectMgmtProvider::Clickup => {
                    let task_id = parse_clickup_task_id(&url_or_id);
                    let client = Arc::clone(clickup_client);
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        match client.get_task(&task_id).await {
                            Ok(task) => {
                                let url = task.url.clone().unwrap_or_default();
                                let is_subtask = task.parent.is_some();
                                let status = if task.status.status_type == "closed" {
                                    ClickUpTaskStatus::Completed {
                                        id: task.id,
                                        name: task.name,
                                        is_subtask,
                                    }
                                } else {
                                    ClickUpTaskStatus::NotStarted {
                                        id: task.id,
                                        name: task.name,
                                        url,
                                        status: task.status.status,
                                        is_subtask,
                                    }
                                };
                                let _ = tx.send(Action::UpdateProjectTaskStatus {
                                    id,
                                    status: ProjectMgmtTaskStatus::ClickUp(status),
                                });
                            }
                            Err(e) => {
                                let status =
                                    ProjectMgmtTaskStatus::ClickUp(ClickUpTaskStatus::Error {
                                        id: task_id,
                                        message: e.to_string(),
                                    });
                                let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                            }
                        }
                    });
                }
                ProjectMgmtProvider::Airtable => {
                    let record_id = parse_airtable_record_id(&url_or_id);
                    let client = Arc::clone(airtable_client);
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        match client.get_record(&record_id).await {
                            Ok(record) => {
                                let status_name = record.status.clone().unwrap_or_default();
                                let is_subtask = record.parent_id.is_some();
                                let is_completed = status_name.to_lowercase().contains("done")
                                    || status_name.to_lowercase().contains("complete");
                                let status = if is_completed {
                                    AirtableTaskStatus::Completed {
                                        id: record.id,
                                        name: record.name,
                                        is_subtask,
                                    }
                                } else if status_name.to_lowercase().contains("progress") {
                                    AirtableTaskStatus::InProgress {
                                        id: record.id,
                                        name: record.name,
                                        url: record.url,
                                        is_subtask,
                                    }
                                } else {
                                    AirtableTaskStatus::NotStarted {
                                        id: record.id,
                                        name: record.name,
                                        url: record.url,
                                        is_subtask,
                                    }
                                };
                                let _ = tx.send(Action::UpdateProjectTaskStatus {
                                    id,
                                    status: ProjectMgmtTaskStatus::Airtable(status),
                                });
                            }
                            Err(e) => {
                                let status =
                                    ProjectMgmtTaskStatus::Airtable(AirtableTaskStatus::Error {
                                        id: record_id,
                                        message: e.to_string(),
                                    });
                                let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                            }
                        }
                    });
                }
                ProjectMgmtProvider::Linear => {
                    let issue_id = parse_linear_issue_id(&url_or_id);
                    let client = Arc::clone(linear_client);
                    let tx = action_tx.clone();
                    let configured_team_id = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .linear
                        .team_id
                        .clone();
                    tokio::spawn(async move {
                        match client.get_issue(&issue_id).await {
                            Ok(issue) => {
                                if let Some(ref team_id) = configured_team_id {
                                    if &issue.team_id != team_id {
                                        let status =
                                            ProjectMgmtTaskStatus::Linear(LinearTaskStatus::Error {
                                                id: issue_id,
                                                message: format!(
                                                    "Issue belongs to team '{}', but this project is configured for team '{}'",
                                                    issue.team_id, team_id
                                                ),
                                            });
                                        let _ =
                                            tx.send(Action::UpdateProjectTaskStatus { id, status });
                                        return;
                                    }
                                }
                                let is_subtask = issue.parent_id.is_some();
                                let status = match issue.state_type.as_str() {
                                    "completed" | "cancelled" => LinearTaskStatus::Completed {
                                        id: issue.id,
                                        identifier: issue.identifier,
                                        name: issue.title,
                                        status_name: issue.state_name,
                                        is_subtask,
                                    },
                                    "started" => LinearTaskStatus::InProgress {
                                        id: issue.id,
                                        identifier: issue.identifier,
                                        name: issue.title,
                                        status_name: issue.state_name,
                                        url: issue.url,
                                        is_subtask,
                                    },
                                    _ => LinearTaskStatus::NotStarted {
                                        id: issue.id,
                                        identifier: issue.identifier,
                                        name: issue.title,
                                        status_name: issue.state_name,
                                        url: issue.url,
                                        is_subtask,
                                    },
                                };
                                let _ = tx.send(Action::UpdateProjectTaskStatus {
                                    id,
                                    status: ProjectMgmtTaskStatus::Linear(status),
                                });
                            }
                            Err(e) => {
                                let status =
                                    ProjectMgmtTaskStatus::Linear(LinearTaskStatus::Error {
                                        id: issue_id,
                                        message: e.to_string(),
                                    });
                                let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                            }
                        }
                    });
                }
            }
        }

        Action::UpdateProjectTaskStatus { id, status } => {
            let log_msg = match &status {
                ProjectMgmtTaskStatus::Asana(s) => match s {
                    AsanaTaskStatus::NotStarted { name, .. } => {
                        Some(format!("Asana task '{}' linked", name))
                    }
                    AsanaTaskStatus::InProgress { name, .. } => {
                        Some(format!("Asana task '{}' in progress", name))
                    }
                    AsanaTaskStatus::Completed { name, .. } => {
                        Some(format!("Asana task '{}' completed", name))
                    }
                    AsanaTaskStatus::Error { message, .. } => {
                        Some(format!("Asana error: {}", message))
                    }
                    AsanaTaskStatus::None => None,
                },
                ProjectMgmtTaskStatus::Notion(s) => match s {
                    NotionTaskStatus::Linked {
                        name, status_name, ..
                    } => Some(format!("Notion task '{}' [{}]", name, status_name)),
                    NotionTaskStatus::Error { message, .. } => {
                        Some(format!("Notion error: {}", message))
                    }
                    NotionTaskStatus::None => None,
                },
                ProjectMgmtTaskStatus::ClickUp(s) => match s {
                    ClickUpTaskStatus::NotStarted { name, .. } => {
                        Some(format!("ClickUp task '{}' linked", name))
                    }
                    ClickUpTaskStatus::InProgress { name, .. } => {
                        Some(format!("ClickUp task '{}' in progress", name))
                    }
                    ClickUpTaskStatus::Completed { name, .. } => {
                        Some(format!("ClickUp task '{}' completed", name))
                    }
                    ClickUpTaskStatus::Error { message, .. } => {
                        Some(format!("ClickUp error: {}", message))
                    }
                    ClickUpTaskStatus::None => None,
                },
                ProjectMgmtTaskStatus::Airtable(s) => match s {
                    AirtableTaskStatus::NotStarted { name, .. } => {
                        Some(format!("Airtable task '{}' linked", name))
                    }
                    AirtableTaskStatus::InProgress { name, .. } => {
                        Some(format!("Airtable task '{}' in progress", name))
                    }
                    AirtableTaskStatus::Completed { name, .. } => {
                        Some(format!("Airtable task '{}' completed", name))
                    }
                    AirtableTaskStatus::Error { message, .. } => {
                        Some(format!("Airtable error: {}", message))
                    }
                    AirtableTaskStatus::None => None,
                },
                ProjectMgmtTaskStatus::Linear(s) => match s {
                    LinearTaskStatus::NotStarted { identifier, .. } => {
                        Some(format!("Linear task '{}' linked", identifier))
                    }
                    LinearTaskStatus::InProgress { identifier, .. } => {
                        Some(format!("Linear task '{}' in progress", identifier))
                    }
                    LinearTaskStatus::Completed { identifier, .. } => {
                        Some(format!("Linear task '{}' completed", identifier))
                    }
                    LinearTaskStatus::Error { message, .. } => {
                        Some(format!("Linear error: {}", message))
                    }
                    LinearTaskStatus::None => None,
                },
                ProjectMgmtTaskStatus::None => None,
            };
            if let Some(agent) = state.agents.get_mut(&id) {
                agent.pm_task_status = status;
            }
            if let Some(msg) = log_msg {
                state.log_info(&msg);
                state.show_info(msg);
            }
            let asana_tasks: Vec<(Uuid, String)> = state
                .agents
                .values()
                .filter_map(|a| {
                    a.pm_task_status
                        .as_asana()
                        .and_then(|s| s.gid().map(|gid| (a.id, gid.to_string())))
                })
                .collect();
            let _ = asana_watch_tx.send(asana_tasks);
            let notion_tasks: Vec<(Uuid, String)> = state
                .agents
                .values()
                .filter_map(|a| {
                    a.pm_task_status
                        .as_notion()
                        .and_then(|s| s.page_id().map(|id| (a.id, id.to_string())))
                })
                .collect();
            let _ = notion_watch_tx.send(notion_tasks);
            let clickup_tasks: Vec<(Uuid, String)> = state
                .agents
                .values()
                .filter_map(|a| {
                    a.pm_task_status
                        .as_clickup()
                        .and_then(|s| s.id().map(|id| (a.id, id.to_string())))
                })
                .collect();
            let _ = clickup_watch_tx.send(clickup_tasks);
            let airtable_tasks: Vec<(Uuid, String)> = state
                .agents
                .values()
                .filter_map(|a| {
                    a.pm_task_status
                        .as_airtable()
                        .and_then(|s| s.id().map(|id| (a.id, id.to_string())))
                })
                .collect();
            let _ = airtable_watch_tx.send(airtable_tasks);
            let linear_tasks: Vec<(Uuid, String)> = state
                .agents
                .values()
                .filter_map(|a| {
                    a.pm_task_status
                        .as_linear()
                        .and_then(|s| s.id().map(|id| (a.id, id.to_string())))
                })
                .collect();
            let _ = linear_watch_tx.send(linear_tasks);
        }

        Action::CycleTaskStatus { id } => {
            if let Some(agent) = state.agents.get(&id) {
                let current_status = agent.pm_task_status.clone();
                match &current_status {
                    ProjectMgmtTaskStatus::Asana(asana_status) => match asana_status {
                        AsanaTaskStatus::NotStarted {
                            gid,
                            name,
                            url,
                            is_subtask,
                            status_name: _,
                        } => {
                            let gid = gid.clone();
                            let name = name.clone();
                            let url = url.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::InProgress {
                                        gid: gid.clone(),
                                        name: name.clone(),
                                        url: url.clone(),
                                        is_subtask,
                                        status_name: "In Progress".to_string(),
                                    });
                            }
                            let client = Arc::clone(asana_client);
                            let override_gid = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .asana
                                .in_progress_section_gid
                                .clone();
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client
                                    .move_to_in_progress(&gid, override_gid.as_deref())
                                    .await
                                {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::Asana(
                                            AsanaTaskStatus::Error {
                                                gid,
                                                message: format!(
                                                    "Failed to move to In Progress: {}",
                                                    e
                                                ),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("Asana task '{}' → In Progress", name));
                        }
                        AsanaTaskStatus::InProgress {
                            gid,
                            name,
                            is_subtask,
                            ..
                        } => {
                            let gid = gid.clone();
                            let name = name.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::Completed {
                                        gid: gid.clone(),
                                        name: name.clone(),
                                        is_subtask,
                                        status_name: "Complete".to_string(),
                                    });
                            }
                            let client = Arc::clone(asana_client);
                            let done_gid = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .asana
                                .done_section_gid
                                .clone();
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client.complete_task(&gid).await {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::Asana(
                                            AsanaTaskStatus::Error {
                                                gid,
                                                message: format!("Failed to complete task: {}", e),
                                            },
                                        ),
                                    });
                                } else {
                                    let _ = client.move_to_done(&gid, done_gid.as_deref()).await;
                                }
                            });
                            state.log_info(format!("Asana task '{}' → Done", name));
                        }
                        AsanaTaskStatus::Completed {
                            name, is_subtask, ..
                        } => {
                            let gid = match asana_status.gid() {
                                Some(g) => g.to_string(),
                                None => return Ok(false),
                            };
                            let name = name.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted {
                                        gid: gid.clone(),
                                        name: name.clone(),
                                        url: String::new(),
                                        is_subtask,
                                        status_name: "Not Started".to_string(),
                                    });
                            }
                            let client = Arc::clone(asana_client);
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client.uncomplete_task(&gid).await {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::Asana(
                                            AsanaTaskStatus::Error {
                                                gid,
                                                message: format!(
                                                    "Failed to uncomplete task: {}",
                                                    e
                                                ),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("Asana task '{}' → Not Started", name));
                        }
                        AsanaTaskStatus::Error { .. } | AsanaTaskStatus::None => {}
                    },
                    ProjectMgmtTaskStatus::ClickUp(clickup_status) => match clickup_status {
                        ClickUpTaskStatus::NotStarted {
                            id: task_id,
                            name,
                            url,
                            status,
                            is_subtask,
                        } => {
                            let task_id = task_id.clone();
                            let name = name.clone();
                            let url = url.clone();
                            let status = status.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::ClickUp(ClickUpTaskStatus::InProgress {
                                        id: task_id.clone(),
                                        name: name.clone(),
                                        url: url.clone(),
                                        status: status.clone(),
                                        is_subtask,
                                    });
                            }
                            let client = Arc::clone(clickup_client);
                            let override_status = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .clickup
                                .in_progress_status
                                .clone();
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client
                                    .move_to_in_progress(&task_id, override_status.as_deref())
                                    .await
                                {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::ClickUp(
                                            ClickUpTaskStatus::Error {
                                                id: task_id,
                                                message: format!(
                                                    "Failed to move to In Progress: {}",
                                                    e
                                                ),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("ClickUp task '{}' → In Progress", name));
                        }
                        ClickUpTaskStatus::InProgress {
                            id: task_id,
                            name,
                            is_subtask,
                            ..
                        } => {
                            let task_id = task_id.clone();
                            let name = name.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::ClickUp(ClickUpTaskStatus::Completed {
                                        id: task_id.clone(),
                                        name: name.clone(),
                                        is_subtask,
                                    });
                            }
                            let client = Arc::clone(clickup_client);
                            let done_status = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .clickup
                                .done_status
                                .clone();
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) =
                                    client.move_to_done(&task_id, done_status.as_deref()).await
                                {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::ClickUp(
                                            ClickUpTaskStatus::Error {
                                                id: task_id,
                                                message: format!("Failed to complete task: {}", e),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("ClickUp task '{}' → Done", name));
                        }
                        ClickUpTaskStatus::Completed {
                            id: task_id,
                            name,
                            is_subtask,
                        } => {
                            let task_id = task_id.clone();
                            let name = name.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::ClickUp(ClickUpTaskStatus::NotStarted {
                                        id: task_id.clone(),
                                        name: name.clone(),
                                        url: String::new(),
                                        status: String::new(),
                                        is_subtask,
                                    });
                            }
                            let client = Arc::clone(clickup_client);
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client.move_to_not_started(&task_id, None).await {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::ClickUp(
                                            ClickUpTaskStatus::Error {
                                                id: task_id,
                                                message: format!(
                                                    "Failed to uncomplete task: {}",
                                                    e
                                                ),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("ClickUp task '{}' → Not Started", name));
                        }
                        ClickUpTaskStatus::Error { .. } | ClickUpTaskStatus::None => {}
                    },
                    ProjectMgmtTaskStatus::Airtable(airtable_status) => match airtable_status {
                        AirtableTaskStatus::NotStarted {
                            id: record_id,
                            name,
                            url,
                            is_subtask,
                        } => {
                            let record_id = record_id.clone();
                            let name = name.clone();
                            let url = url.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status = ProjectMgmtTaskStatus::Airtable(
                                    AirtableTaskStatus::InProgress {
                                        id: record_id.clone(),
                                        name: name.clone(),
                                        url: url.clone(),
                                        is_subtask,
                                    },
                                );
                            }
                            let client = Arc::clone(airtable_client);
                            let override_value = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .in_progress_option
                                .clone();
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client
                                    .move_to_in_progress(&record_id, override_value.as_deref())
                                    .await
                                {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::Airtable(
                                            AirtableTaskStatus::Error {
                                                id: record_id,
                                                message: format!(
                                                    "Failed to move to In Progress: {}",
                                                    e
                                                ),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("Airtable task '{}' → In Progress", name));
                        }
                        AirtableTaskStatus::InProgress {
                            id: record_id,
                            name,
                            is_subtask,
                            ..
                        } => {
                            let record_id = record_id.clone();
                            let name = name.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status = ProjectMgmtTaskStatus::Airtable(
                                    AirtableTaskStatus::Completed {
                                        id: record_id.clone(),
                                        name: name.clone(),
                                        is_subtask,
                                    },
                                );
                            }
                            let client = Arc::clone(airtable_client);
                            let override_value = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .done_option
                                .clone();
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client
                                    .move_to_done(&record_id, override_value.as_deref())
                                    .await
                                {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::Airtable(
                                            AirtableTaskStatus::Error {
                                                id: record_id,
                                                message: format!("Failed to complete task: {}", e),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("Airtable task '{}' → Done", name));
                        }
                        AirtableTaskStatus::Completed {
                            name, is_subtask, ..
                        } => {
                            let record_id = match airtable_status.id() {
                                Some(r) => r.to_string(),
                                None => return Ok(false),
                            };
                            let name = name.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status = ProjectMgmtTaskStatus::Airtable(
                                    AirtableTaskStatus::NotStarted {
                                        id: record_id.clone(),
                                        name: name.clone(),
                                        url: String::new(),
                                        is_subtask,
                                    },
                                );
                            }
                            let client = Arc::clone(airtable_client);
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client.move_to_not_started(&record_id, None).await {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::Airtable(
                                            AirtableTaskStatus::Error {
                                                id: record_id,
                                                message: format!(
                                                    "Failed to uncomplete task: {}",
                                                    e
                                                ),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("Airtable task '{}' → Not Started", name));
                        }
                        AirtableTaskStatus::Error { .. } | AirtableTaskStatus::None => {}
                    },
                    ProjectMgmtTaskStatus::Linear(linear_status) => match linear_status {
                        LinearTaskStatus::NotStarted {
                            id: issue_id,
                            identifier,
                            name,
                            url,
                            is_subtask,
                            ..
                        } => {
                            let issue_id = issue_id.clone();
                            let identifier = identifier.clone();
                            let name = name.clone();
                            let url = url.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::Linear(LinearTaskStatus::InProgress {
                                        id: issue_id.clone(),
                                        identifier: identifier.clone(),
                                        name: name.clone(),
                                        status_name: "In Progress".to_string(),
                                        url: url.clone(),
                                        is_subtask,
                                    });
                            }
                            let client = Arc::clone(linear_client);
                            let override_state = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .linear
                                .in_progress_state
                                .clone();
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client
                                    .move_to_in_progress(&issue_id, override_state.as_deref())
                                    .await
                                {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::Linear(
                                            LinearTaskStatus::Error {
                                                id: issue_id,
                                                message: format!(
                                                    "Failed to move to In Progress: {}",
                                                    e
                                                ),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("Linear task '{}' → In Progress", identifier));
                        }
                        LinearTaskStatus::InProgress {
                            id: issue_id,
                            identifier,
                            name,
                            is_subtask,
                            ..
                        } => {
                            let issue_id = issue_id.clone();
                            let identifier = identifier.clone();
                            let name = name.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::Linear(LinearTaskStatus::Completed {
                                        id: issue_id.clone(),
                                        identifier: identifier.clone(),
                                        name: name.clone(),
                                        status_name: "Done".to_string(),
                                        is_subtask,
                                    });
                            }
                            let client = Arc::clone(linear_client);
                            let override_state = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .linear
                                .done_state
                                .clone();
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client
                                    .move_to_done(&issue_id, override_state.as_deref())
                                    .await
                                {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::Linear(
                                            LinearTaskStatus::Error {
                                                id: issue_id,
                                                message: format!("Failed to complete task: {}", e),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("Linear task '{}' → Done", identifier));
                        }
                        LinearTaskStatus::Completed {
                            id: issue_id,
                            identifier,
                            name,
                            is_subtask,
                            ..
                        } => {
                            let issue_id = issue_id.clone();
                            let identifier = identifier.clone();
                            let name = name.clone();
                            let is_subtask = *is_subtask;
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::Linear(LinearTaskStatus::NotStarted {
                                        id: issue_id.clone(),
                                        identifier: identifier.clone(),
                                        name: name.clone(),
                                        status_name: "Todo".to_string(),
                                        url: String::new(),
                                        is_subtask,
                                    });
                            }
                            let client = Arc::clone(linear_client);
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client.move_to_not_started(&issue_id, None).await {
                                    let _ = tx.send(Action::UpdateProjectTaskStatus {
                                        id: agent_id,
                                        status: ProjectMgmtTaskStatus::Linear(
                                            LinearTaskStatus::Error {
                                                id: issue_id,
                                                message: format!(
                                                    "Failed to uncomplete task: {}",
                                                    e
                                                ),
                                            },
                                        ),
                                    });
                                }
                            });
                            state.log_info(format!("Linear task '{}' → Not Started", identifier));
                        }
                        LinearTaskStatus::Error { .. } | LinearTaskStatus::None => {}
                    },
                    ProjectMgmtTaskStatus::Notion(_) => {}
                    ProjectMgmtTaskStatus::None => {}
                }
            }
        }

        Action::OpenTaskStatusDropdown { id } => {
            tracing::info!("OpenTaskStatusDropdown called for agent {}", id);
            if let Some(agent) = state.agents.get(&id) {
                tracing::info!("Agent found, pm_task_status: {:?}", agent.pm_task_status);
                match &agent.pm_task_status {
                    ProjectMgmtTaskStatus::Notion(notion_status) => {
                        if !notion_client.is_configured().await {
                            state.show_error(
                                "Notion not configured. Set NOTION_TOKEN and database_id.",
                            );
                            return Ok(false);
                        }
                        let page_id = notion_status.page_id();
                        if page_id.is_none() || page_id.map(|p| p.is_empty()).unwrap_or(true) {
                            state.show_error("No Notion page linked to this task");
                            return Ok(false);
                        }
                        let agent_id = id;
                        let client = Arc::clone(notion_client);
                        let tx = action_tx.clone();
                        state.loading_message = Some("Loading status options...".to_string());
                        tokio::spawn(async move {
                            tracing::info!("Fetching Notion status options...");
                            match client.get_status_options().await {
                                Ok(opts) => {
                                    tracing::info!("Got {} status options", opts.all_options.len());
                                    let options: Vec<StatusOption> = opts
                                        .all_options
                                        .into_iter()
                                        .map(|o| StatusOption {
                                            id: o.id,
                                            name: o.name,
                                        })
                                        .collect();
                                    let _ = tx.send(Action::TaskStatusOptionsLoaded {
                                        id: agent_id,
                                        options,
                                    });
                                }
                                Err(e) => {
                                    tracing::error!("Failed to load status options: {}", e);
                                    let _ = tx.send(Action::SetLoading(None));
                                    let _ = tx.send(Action::ShowError(format!(
                                        "Failed to load status options: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    ProjectMgmtTaskStatus::Asana(asana_status) => {
                        if !asana_client.is_configured().await {
                            state.show_error(
                                "Asana not configured. Set ASANA_TOKEN and project_gid.",
                            );
                            return Ok(false);
                        }
                        if asana_status.gid().is_none() {
                            state.show_error("No Asana task linked to this agent");
                            return Ok(false);
                        }

                        if asana_status.is_subtask() {
                            let gid = asana_status.gid().unwrap().to_string();
                            let name = asana_status.name().unwrap_or("Task").to_string();
                            let is_completed =
                                matches!(asana_status, AsanaTaskStatus::Completed { .. });
                            state.subtask_status_dropdown = Some(SubtaskStatusDropdownState {
                                task_id: gid,
                                task_name: name,
                                current_completed: is_completed,
                                selected_index: if is_completed { 1 } else { 0 },
                            });
                            state.input_mode = Some(InputMode::SelectSubtaskStatus);
                        } else {
                            let agent_id = id;
                            let client = Arc::clone(asana_client);
                            let tx = action_tx.clone();
                            state.loading_message = Some("Loading sections...".to_string());
                            tokio::spawn(async move {
                                match client.get_sections().await {
                                    Ok(sections) => {
                                        let options: Vec<StatusOption> = sections
                                            .into_iter()
                                            .map(|s| StatusOption {
                                                id: s.gid,
                                                name: s.name,
                                            })
                                            .collect();
                                        let _ = tx.send(Action::TaskStatusOptionsLoaded {
                                            id: agent_id,
                                            options,
                                        });
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to load Asana sections: {}", e);
                                        let _ = tx.send(Action::SetLoading(None));
                                        let _ = tx.send(Action::ShowError(format!(
                                            "Failed to load sections: {}",
                                            e
                                        )));
                                    }
                                }
                            });
                        }
                    }
                    ProjectMgmtTaskStatus::ClickUp(clickup_status) => {
                        if !clickup_client.is_configured().await {
                            state.show_error(
                                "ClickUp not configured. Set CLICKUP_TOKEN and list_id.",
                            );
                            return Ok(false);
                        }
                        if clickup_status.id().is_none() {
                            state.show_error("No ClickUp task linked to this agent");
                            return Ok(false);
                        }

                        // ClickUp subtasks use the same statuses as parent tasks
                        let agent_id = id;
                        let client = Arc::clone(clickup_client);
                        let tx = action_tx.clone();
                        state.loading_message = Some("Loading statuses...".to_string());
                        tokio::spawn(async move {
                            match client.get_statuses().await {
                                Ok(statuses) => {
                                    let options: Vec<StatusOption> = statuses
                                        .into_iter()
                                        .map(|s| StatusOption {
                                            id: s.status.clone(),
                                            name: s.status,
                                        })
                                        .collect();
                                    let _ = tx.send(Action::TaskStatusOptionsLoaded {
                                        id: agent_id,
                                        options,
                                    });
                                }
                                Err(e) => {
                                    tracing::error!("Failed to load ClickUp statuses: {}", e);
                                    let _ = tx.send(Action::SetLoading(None));
                                    let _ = tx.send(Action::ShowError(format!(
                                        "Failed to load statuses: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    ProjectMgmtTaskStatus::Airtable(airtable_status) => {
                        if !airtable_client.is_configured().await {
                            state.show_error(
                                "Airtable not configured. Set AIRTABLE_TOKEN, base_id, and table_name.",
                            );
                            return Ok(false);
                        }
                        if airtable_status.id().is_none() {
                            state.show_error("No Airtable record linked to this agent");
                            return Ok(false);
                        }

                        let agent_id = id;
                        let client = Arc::clone(airtable_client);
                        let tx = action_tx.clone();
                        state.loading_message = Some("Loading statuses...".to_string());
                        tokio::spawn(async move {
                            match client.get_status_options().await {
                                Ok(options) => {
                                    let status_options: Vec<StatusOption> = options
                                        .into_iter()
                                        .map(|o| StatusOption {
                                            id: o.name.clone(),
                                            name: o.name,
                                        })
                                        .collect();
                                    let _ = tx.send(Action::TaskStatusOptionsLoaded {
                                        id: agent_id,
                                        options: status_options,
                                    });
                                }
                                Err(e) => {
                                    tracing::error!("Failed to load Airtable statuses: {}", e);
                                    let _ = tx.send(Action::SetLoading(None));
                                    let _ = tx.send(Action::ShowError(format!(
                                        "Failed to load statuses: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    ProjectMgmtTaskStatus::Linear(linear_status) => {
                        if !linear_client.is_configured().await {
                            state
                                .show_error("Linear not configured. Set LINEAR_TOKEN and team_id.");
                            return Ok(false);
                        }
                        if linear_status.id().is_none() {
                            state.show_error("No Linear issue linked to this agent");
                            return Ok(false);
                        }

                        let agent_id = id;
                        let client = Arc::clone(linear_client);
                        let tx = action_tx.clone();
                        state.loading_message = Some("Loading statuses...".to_string());
                        tokio::spawn(async move {
                            match client.get_workflow_states().await {
                                Ok(states) => {
                                    let options: Vec<StatusOption> = states
                                        .into_iter()
                                        .map(|s| StatusOption {
                                            id: s.id,
                                            name: s.name,
                                        })
                                        .collect();
                                    let _ = tx.send(Action::TaskStatusOptionsLoaded {
                                        id: agent_id,
                                        options,
                                    });
                                }
                                Err(e) => {
                                    tracing::error!("Failed to load Linear statuses: {}", e);
                                    let _ = tx.send(Action::SetLoading(None));
                                    let _ = tx.send(Action::ShowError(format!(
                                        "Failed to load statuses: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    ProjectMgmtTaskStatus::None => {}
                }
            }
        }

        Action::TaskStatusOptionsLoaded { id, options } => {
            tracing::info!(
                "TaskStatusOptionsLoaded: {} options for agent {}",
                options.len(),
                id
            );
            state.loading_message = None;
            if !options.is_empty() {
                state.task_status_dropdown = Some(TaskStatusDropdownState {
                    agent_id: id,
                    status_options: options,
                    selected_index: 0,
                });
                state.input_mode = Some(InputMode::SelectTaskStatus);
                tracing::info!("Dropdown opened with input_mode = SelectTaskStatus");
            } else {
                state.show_warning("No status options found");
            }
        }

        Action::TaskStatusDropdownNext => {
            if let Some(ref mut dropdown) = state.task_status_dropdown {
                if dropdown.selected_index < dropdown.status_options.len().saturating_sub(1) {
                    dropdown.selected_index += 1;
                }
            }
        }

        Action::TaskStatusDropdownPrev => {
            if let Some(ref mut dropdown) = state.task_status_dropdown {
                if dropdown.selected_index > 0 {
                    dropdown.selected_index -= 1;
                }
            }
        }

        Action::SubtaskStatusDropdownNext => {
            if let Some(ref mut dropdown) = state.subtask_status_dropdown {
                if dropdown.selected_index < 1 {
                    dropdown.selected_index += 1;
                }
            }
        }

        Action::SubtaskStatusDropdownPrev => {
            if let Some(ref mut dropdown) = state.subtask_status_dropdown {
                if dropdown.selected_index > 0 {
                    dropdown.selected_index -= 1;
                }
            }
        }

        Action::TaskStatusDropdownSelect => {
            tracing::info!("TaskStatusDropdownSelect triggered");
            let dropdown = state.task_status_dropdown.take();
            let subtask_dropdown = state.subtask_status_dropdown.take();
            state.exit_input_mode();
            if let Some(dropdown) = dropdown {
                let agent_id = dropdown.agent_id;
                if let Some(selected_option) = dropdown.status_options.get(dropdown.selected_index)
                {
                    let option_id = selected_option.id.clone();
                    let option_name = selected_option.name.clone();

                    // Check if this is a task from the task list (nil UUID)
                    if agent_id == Uuid::nil() {
                        if let Some(subtask) = subtask_dropdown {
                            let task_id = subtask.task_id.clone();
                            let option_id = option_id.clone();
                            let status_name = option_name.clone();
                            let provider = state.settings.repo_config.project_mgmt.provider;

                            if matches!(provider, ProjectMgmtProvider::Notion) {
                                let client = Arc::clone(notion_client);
                                let tx = action_tx.clone();
                                let status_prop_name = state
                                    .settings
                                    .repo_config
                                    .project_mgmt
                                    .notion
                                    .status_property_name
                                    .clone();
                                state.loading_message = Some("Updating task status...".to_string());

                                let task_id_for_spawn = task_id.clone();
                                let status_name_for_spawn = status_name.clone();
                                tokio::spawn(async move {
                                    let prop_name =
                                        status_prop_name.unwrap_or_else(|| "Status".to_string());
                                    if let Err(e) = client
                                        .update_page_status(
                                            &task_id_for_spawn,
                                            &prop_name,
                                            &option_id,
                                        )
                                        .await
                                    {
                                        tracing::error!("Failed to update Notion status: {}", e);
                                        let _ = tx.send(Action::SetLoading(None));
                                        let _ = tx.send(Action::ShowError(format!(
                                            "Failed to update status: {}",
                                            e
                                        )));
                                    } else {
                                        let _ = tx.send(Action::SetLoading(None));
                                        let _ = tx.send(Action::ShowToast {
                                            message: format!("Task → {}", status_name_for_spawn),
                                            level: ToastLevel::Success,
                                        });
                                        let _ = tx.send(Action::RefreshTaskList);
                                    }
                                });

                                if let Some(task) =
                                    state.task_list.iter_mut().find(|t| t.id == task_id)
                                {
                                    let lower = status_name.to_lowercase();
                                    task.status_name = status_name.clone();
                                    task.completed =
                                        lower.contains("done") || lower.contains("complete");
                                    task.status = if task.completed {
                                        TaskItemStatus::Completed
                                    } else if lower.contains("progress") {
                                        TaskItemStatus::InProgress
                                    } else {
                                        TaskItemStatus::NotStarted
                                    };
                                }
                                state.show_success(format!("Task → {}", status_name));
                            } else if matches!(provider, ProjectMgmtProvider::Linear) {
                                let client = Arc::clone(linear_client);
                                let tx = action_tx.clone();
                                state.loading_message = Some("Updating task status...".to_string());

                                let task_id_for_spawn = task_id.clone();
                                let status_name_for_spawn = status_name.clone();
                                let state_id_for_spawn = option_id.clone();
                                tokio::spawn(async move {
                                    match client
                                        .update_issue_status(
                                            &task_id_for_spawn,
                                            &state_id_for_spawn,
                                        )
                                        .await
                                    {
                                        Ok(()) => {
                                            let _ = tx.send(Action::SetLoading(None));
                                            let _ = tx.send(Action::ShowToast {
                                                message: format!(
                                                    "Task → {}",
                                                    status_name_for_spawn
                                                ),
                                                level: ToastLevel::Success,
                                            });
                                            let _ = tx.send(Action::RefreshTaskList);
                                        }
                                        Err(e) => {
                                            let _ = tx.send(Action::SetLoading(None));
                                            let _ = tx.send(Action::ShowError(format!(
                                                "Failed to update task: {}",
                                                e
                                            )));
                                        }
                                    }
                                });

                                if let Some(task) =
                                    state.task_list.iter_mut().find(|t| t.id == task_id)
                                {
                                    let lower = status_name.to_lowercase();
                                    task.status_name = status_name.clone();
                                    task.status = if lower.contains("done")
                                        || lower.contains("complete")
                                        || lower.contains("cancelled")
                                    {
                                        task.completed = true;
                                        TaskItemStatus::Completed
                                    } else if lower.contains("progress")
                                        || lower.contains("started")
                                    {
                                        task.completed = false;
                                        TaskItemStatus::InProgress
                                    } else {
                                        task.completed = false;
                                        TaskItemStatus::NotStarted
                                    };
                                }
                                state.show_success(format!("Task → {}", status_name));
                            } else {
                                let client = Arc::clone(clickup_client);
                                let tx = action_tx.clone();
                                state.loading_message =
                                    Some("Updating subtask status...".to_string());

                                let task_id_for_spawn = task_id.clone();
                                let status_name_for_spawn = status_name.clone();
                                tokio::spawn(async move {
                                    match client
                                        .update_task_status(
                                            &task_id_for_spawn,
                                            &status_name_for_spawn,
                                        )
                                        .await
                                    {
                                        Ok(()) => {
                                            let _ = tx.send(Action::SetLoading(None));
                                            let _ = tx.send(Action::ShowToast {
                                                message: format!(
                                                    "Subtask → {}",
                                                    status_name_for_spawn
                                                ),
                                                level: ToastLevel::Success,
                                            });
                                            let _ = tx.send(Action::RefreshTaskList);
                                        }
                                        Err(e) => {
                                            let _ = tx.send(Action::SetLoading(None));
                                            let _ = tx.send(Action::ShowError(format!(
                                                "Failed to update subtask: {}",
                                                e
                                            )));
                                        }
                                    }
                                });

                                if let Some(task) =
                                    state.task_list.iter_mut().find(|t| t.id == task_id)
                                {
                                    let lower = status_name.to_lowercase();
                                    task.status_name = status_name.clone();
                                    task.status = if lower.contains("done")
                                        || lower.contains("complete")
                                        || lower.contains("closed")
                                    {
                                        task.completed = true;
                                        TaskItemStatus::Completed
                                    } else if lower.contains("progress") {
                                        task.completed = false;
                                        TaskItemStatus::InProgress
                                    } else {
                                        task.completed = false;
                                        TaskItemStatus::NotStarted
                                    };
                                }
                                state.show_success(format!("Subtask → {}", status_name));
                            }
                        }
                        return Ok(false);
                    }

                    if let Some(agent) = state.agents.get(&agent_id) {
                        match &agent.pm_task_status {
                            ProjectMgmtTaskStatus::Notion(notion_status) => {
                                if let Some(page_id) = notion_status.page_id() {
                                    if page_id.is_empty() {
                                        state.show_error("No Notion page linked to this task");
                                        return Ok(false);
                                    }
                                    let page_id = page_id.to_string();
                                    let task_name = match notion_status {
                                        NotionTaskStatus::Linked { name, .. } => name.clone(),
                                        NotionTaskStatus::Error { .. } => "Task".to_string(),
                                        NotionTaskStatus::None => "Task".to_string(),
                                    };
                                    let client = Arc::clone(notion_client);
                                    let status_prop_name = state
                                        .settings
                                        .repo_config
                                        .project_mgmt
                                        .notion
                                        .status_property_name
                                        .clone();
                                    let tx = action_tx.clone();

                                    let new_status =
                                        ProjectMgmtTaskStatus::Notion(NotionTaskStatus::Linked {
                                            page_id: page_id.clone(),
                                            name: task_name.clone(),
                                            url: String::new(),
                                            status_option_id: option_id.clone(),
                                            status_name: option_name.clone(),
                                        });

                                    if let Some(agent) = state.agents.get_mut(&agent_id) {
                                        agent.pm_task_status = new_status;
                                    }

                                    tracing::info!(
                                        "Updating Notion page {} status to '{}'",
                                        page_id,
                                        option_name
                                    );
                                    tokio::spawn(async move {
                                        let prop_name = status_prop_name
                                            .unwrap_or_else(|| "Status".to_string());
                                        if let Err(e) = client
                                            .update_page_status(&page_id, &prop_name, &option_id)
                                            .await
                                        {
                                            tracing::error!(
                                                "Failed to update Notion status: {}",
                                                e
                                            );
                                            let _ = tx.send(Action::ShowError(format!(
                                                "Failed to update status: {}",
                                                e
                                            )));
                                        }
                                    });
                                    state.show_success(format!("Task → {}", option_name));
                                } else {
                                    state.show_error("No Notion page linked to this task");
                                }
                            }
                            ProjectMgmtTaskStatus::Asana(asana_status) => {
                                if let Some(gid_str) = asana_status.gid() {
                                    let task_gid = gid_str.to_string();
                                    let task_name = asana_status.format_short();
                                    let is_subtask = asana_status.is_subtask();
                                    let client = Arc::clone(asana_client);
                                    let agent_id_clone = agent_id;
                                    let section_gid = option_id.clone();
                                    let section_name_lower = option_name.to_lowercase();

                                    let is_done = section_name_lower.contains("done")
                                        || section_name_lower.contains("complete");
                                    let is_in_progress = section_name_lower.contains("progress");

                                    let new_status = if is_done {
                                        ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::Completed {
                                            gid: task_gid.clone(),
                                            name: task_name.clone(),
                                            is_subtask,
                                            status_name: option_name.clone(),
                                        })
                                    } else if is_in_progress {
                                        ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::InProgress {
                                            gid: task_gid.clone(),
                                            name: task_name.clone(),
                                            url: String::new(),
                                            is_subtask,
                                            status_name: option_name.clone(),
                                        })
                                    } else {
                                        ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted {
                                            gid: task_gid.clone(),
                                            name: task_name.clone(),
                                            url: String::new(),
                                            is_subtask,
                                            status_name: option_name.clone(),
                                        })
                                    };

                                    tokio::spawn(async move {
                                        if is_done {
                                            let _ = client.complete_task(&task_gid).await;
                                        } else {
                                            let _ = client.uncomplete_task(&task_gid).await;
                                        }
                                        let _ = client
                                            .move_task_to_section(&task_gid, &section_gid)
                                            .await;
                                    });

                                    if let Some(agent) = state.agents.get_mut(&agent_id_clone) {
                                        agent.pm_task_status = new_status;
                                    }
                                    state.show_success(format!("Task → {}", option_name));
                                }
                            }
                            ProjectMgmtTaskStatus::ClickUp(clickup_status) => {
                                if let Some(task_id_str) = clickup_status.id() {
                                    let task_id = task_id_str.to_string();
                                    let task_name = match clickup_status {
                                        ClickUpTaskStatus::NotStarted { name, .. } => name.clone(),
                                        ClickUpTaskStatus::InProgress { name, .. } => name.clone(),
                                        ClickUpTaskStatus::Completed { name, .. } => name.clone(),
                                        ClickUpTaskStatus::Error { .. } => "Task".to_string(),
                                        ClickUpTaskStatus::None => "Task".to_string(),
                                    };
                                    let is_subtask = clickup_status.is_subtask();
                                    let client = Arc::clone(clickup_client);
                                    let agent_id_clone = agent_id;
                                    let new_status_name = option_name.clone();

                                    let is_done = new_status_name.to_lowercase().contains("done")
                                        || new_status_name.to_lowercase().contains("complete")
                                        || new_status_name.to_lowercase().contains("closed");
                                    let is_in_progress =
                                        new_status_name.to_lowercase().contains("progress");

                                    let new_status = if is_done {
                                        ProjectMgmtTaskStatus::ClickUp(
                                            ClickUpTaskStatus::Completed {
                                                id: task_id.clone(),
                                                name: task_name.clone(),
                                                is_subtask,
                                            },
                                        )
                                    } else if is_in_progress {
                                        ProjectMgmtTaskStatus::ClickUp(
                                            ClickUpTaskStatus::InProgress {
                                                id: task_id.clone(),
                                                name: task_name.clone(),
                                                url: String::new(),
                                                status: new_status_name.clone(),
                                                is_subtask,
                                            },
                                        )
                                    } else {
                                        ProjectMgmtTaskStatus::ClickUp(
                                            ClickUpTaskStatus::NotStarted {
                                                id: task_id.clone(),
                                                name: task_name.clone(),
                                                url: String::new(),
                                                status: new_status_name.clone(),
                                                is_subtask,
                                            },
                                        )
                                    };

                                    tokio::spawn(async move {
                                        let _ = client
                                            .update_task_status(&task_id, &new_status_name)
                                            .await;
                                    });

                                    if let Some(agent) = state.agents.get_mut(&agent_id_clone) {
                                        agent.pm_task_status = new_status;
                                    }
                                    state.show_success(format!("Task → {}", option_name));
                                }
                            }
                            ProjectMgmtTaskStatus::Airtable(airtable_status) => {
                                if let Some(record_id_str) = airtable_status.id() {
                                    let record_id = record_id_str.to_string();
                                    let task_name = match airtable_status {
                                        AirtableTaskStatus::NotStarted { name, .. } => name.clone(),
                                        AirtableTaskStatus::InProgress { name, .. } => name.clone(),
                                        AirtableTaskStatus::Completed { name, .. } => name.clone(),
                                        AirtableTaskStatus::Error { .. } => "Task".to_string(),
                                        AirtableTaskStatus::None => "Task".to_string(),
                                    };
                                    let is_subtask = airtable_status.is_subtask();
                                    let client = Arc::clone(airtable_client);
                                    let agent_id_clone = agent_id;
                                    let new_status_name = option_name.clone();

                                    let is_done = new_status_name.to_lowercase().contains("done")
                                        || new_status_name.to_lowercase().contains("complete");
                                    let is_in_progress =
                                        new_status_name.to_lowercase().contains("progress");

                                    let new_status = if is_done {
                                        ProjectMgmtTaskStatus::Airtable(
                                            AirtableTaskStatus::Completed {
                                                id: record_id.clone(),
                                                name: task_name.clone(),
                                                is_subtask,
                                            },
                                        )
                                    } else if is_in_progress {
                                        ProjectMgmtTaskStatus::Airtable(
                                            AirtableTaskStatus::InProgress {
                                                id: record_id.clone(),
                                                name: task_name.clone(),
                                                url: String::new(),
                                                is_subtask,
                                            },
                                        )
                                    } else {
                                        ProjectMgmtTaskStatus::Airtable(
                                            AirtableTaskStatus::NotStarted {
                                                id: record_id.clone(),
                                                name: task_name.clone(),
                                                url: String::new(),
                                                is_subtask,
                                            },
                                        )
                                    };

                                    tokio::spawn(async move {
                                        let _ = client
                                            .update_record_status(&record_id, &new_status_name)
                                            .await;
                                    });

                                    if let Some(agent) = state.agents.get_mut(&agent_id_clone) {
                                        agent.pm_task_status = new_status;
                                    }
                                    state.show_success(format!("Task → {}", option_name));
                                }
                            }
                            ProjectMgmtTaskStatus::Linear(linear_status) => {
                                if let Some(issue_id) = linear_status.id() {
                                    let issue_id = issue_id.to_string();
                                    let identifier =
                                        linear_status.identifier().unwrap_or("Task").to_string();
                                    let name = linear_status.name().unwrap_or("").to_string();
                                    let url = linear_status.url().unwrap_or("").to_string();
                                    let is_subtask = linear_status.is_subtask();
                                    let client = Arc::clone(linear_client);
                                    let agent_id_clone = agent_id;
                                    let state_id = option_id.clone();

                                    let new_status = if option_name.to_lowercase().contains("done")
                                        || option_name.to_lowercase().contains("complete")
                                        || option_name.to_lowercase().contains("cancelled")
                                    {
                                        ProjectMgmtTaskStatus::Linear(LinearTaskStatus::Completed {
                                            id: issue_id.clone(),
                                            identifier: identifier.clone(),
                                            name: name.clone(),
                                            status_name: option_name.clone(),
                                            is_subtask,
                                        })
                                    } else if option_name.to_lowercase().contains("progress") {
                                        ProjectMgmtTaskStatus::Linear(
                                            LinearTaskStatus::InProgress {
                                                id: issue_id.clone(),
                                                identifier: identifier.clone(),
                                                name: name.clone(),
                                                status_name: option_name.clone(),
                                                url: url.clone(),
                                                is_subtask,
                                            },
                                        )
                                    } else {
                                        ProjectMgmtTaskStatus::Linear(
                                            LinearTaskStatus::NotStarted {
                                                id: issue_id.clone(),
                                                identifier: identifier.clone(),
                                                name: name.clone(),
                                                status_name: option_name.clone(),
                                                url: url.clone(),
                                                is_subtask,
                                            },
                                        )
                                    };

                                    tokio::spawn(async move {
                                        let _ =
                                            client.update_issue_status(&issue_id, &state_id).await;
                                    });

                                    if let Some(agent) = state.agents.get_mut(&agent_id_clone) {
                                        agent.pm_task_status = new_status;
                                    }
                                    state.show_success(format!("Task → {}", option_name));
                                }
                            }
                            ProjectMgmtTaskStatus::None => {}
                        }
                    }
                }
            }
        }

        Action::OpenProjectTaskInBrowser { id } => {
            if let Some(agent) = state.agents.get(&id) {
                if let Some(url) = agent.pm_task_status.url() {
                    match open::that(url) {
                        Ok(_) => {
                            state.log_info("Opening task in browser".to_string());
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to open browser: {}", e));
                            state.show_error(format!("Failed to open browser: {}", e));
                        }
                    }
                } else {
                    state.show_error("No task linked to this agent");
                }
            }
        }

        Action::DeleteAgentAndCompleteTask { id } => {
            state.exit_input_mode();

            if let Some(agent) = state.agents.get(&id) {
                match &agent.pm_task_status {
                    ProjectMgmtTaskStatus::Asana(asana_status) => {
                        if let Some(task_gid) = asana_status.gid() {
                            let gid = task_gid.to_string();
                            let client = Arc::clone(asana_client);
                            let done_gid = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .asana
                                .done_section_gid
                                .clone();
                            tokio::spawn(async move {
                                let _ = client.move_to_done(&gid, done_gid.as_deref()).await;
                                let _ = client.complete_task(&gid).await;
                            });
                            state.log_info("Moving Asana task to Done".to_string());
                        }
                    }
                    ProjectMgmtTaskStatus::Notion(notion_status) => {
                        if let Some(page_id) = notion_status.page_id() {
                            let pid = page_id.to_string();
                            let client = Arc::clone(notion_client);
                            let status_prop_name = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .notion
                                .status_property_name
                                .clone();
                            tokio::spawn(async move {
                                if let Ok(opts) = client.get_status_options().await {
                                    if let Some(done_id) = opts.done_id {
                                        let prop_name = status_prop_name
                                            .unwrap_or_else(|| "Status".to_string());
                                        let _ = client
                                            .update_page_status(&pid, &prop_name, &done_id)
                                            .await;
                                    }
                                }
                            });
                            state.log_info("Moving Notion task to Done".to_string());
                        }
                    }
                    ProjectMgmtTaskStatus::ClickUp(clickup_status) => {
                        if let Some(task_id) = clickup_status.id() {
                            let tid = task_id.to_string();
                            let client = Arc::clone(clickup_client);
                            let done_status = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .clickup
                                .done_status
                                .clone();
                            tokio::spawn(async move {
                                let _ = client.move_to_done(&tid, done_status.as_deref()).await;
                            });
                            state.log_info("Moving ClickUp task to Done".to_string());
                        }
                    }
                    ProjectMgmtTaskStatus::Airtable(airtable_status) => {
                        if let Some(record_id) = airtable_status.id() {
                            let rid = record_id.to_string();
                            let client = Arc::clone(airtable_client);
                            let done_option = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .done_option
                                .clone();
                            tokio::spawn(async move {
                                let _ = client.move_to_done(&rid, done_option.as_deref()).await;
                            });
                            state.log_info("Moving Airtable task to Done".to_string());
                        }
                    }
                    ProjectMgmtTaskStatus::Linear(linear_status) => {
                        if let Some(issue_id) = linear_status.id() {
                            let iid = issue_id.to_string();
                            let client = Arc::clone(linear_client);
                            let done_state = state
                                .settings
                                .repo_config
                                .project_mgmt
                                .linear
                                .done_state
                                .clone();
                            tokio::spawn(async move {
                                let _ = client.move_to_done(&iid, done_state.as_deref()).await;
                            });
                            state.log_info("Moving Linear task to Done".to_string());
                        }
                    }
                    ProjectMgmtTaskStatus::None => {}
                }
            }

            action_tx.send(Action::DeleteAgent { id })?;
        }

        Action::RefreshTaskList => {
            let provider = state.settings.repo_config.project_mgmt.provider;
            match provider {
                ProjectMgmtProvider::Asana => asana_client.invalidate_cache().await,
                ProjectMgmtProvider::Notion => notion_client.invalidate_cache().await,
                ProjectMgmtProvider::Clickup => clickup_client.invalidate_cache().await,
                ProjectMgmtProvider::Airtable => airtable_client.invalidate_cache().await,
                ProjectMgmtProvider::Linear => linear_client.invalidate_cache().await,
            }
            state.task_list_loading = true;
            let _ = action_tx.send(Action::FetchTaskList);
        }

        Action::FetchTaskList => {
            let provider = state.settings.repo_config.project_mgmt.provider;
            let asana_client = Arc::clone(asana_client);
            let notion_client = Arc::clone(notion_client);
            let clickup_client = Arc::clone(clickup_client);
            let airtable_client = Arc::clone(airtable_client);
            let linear_client = Arc::clone(linear_client);
            let tx = action_tx.clone();
            tokio::spawn(async move {
                let result = match provider {
                    ProjectMgmtProvider::Asana => {
                        match asana_client.get_project_tasks_with_subtasks().await {
                            Ok(tasks) => {
                                let mut items: Vec<TaskListItem> = tasks
                                    .into_iter()
                                    .filter(|t| {
                                        if t.parent_gid.is_some() {
                                            true
                                        } else {
                                            !t.completed
                                        }
                                    })
                                    .map(|t| {
                                        let (status, status_name) = if t.completed {
                                            (TaskItemStatus::Completed, "Completed".to_string())
                                        } else {
                                            (TaskItemStatus::NotStarted, "Not Started".to_string())
                                        };
                                        TaskListItem {
                                            id: t.gid,
                                            identifier: None,
                                            name: t.name,
                                            status,
                                            status_name,
                                            url: t.permalink_url.unwrap_or_default(),
                                            parent_id: t.parent_gid,
                                            has_children: t.num_subtasks > 0,
                                            completed: t.completed,
                                        }
                                    })
                                    .collect();
                                sort_tasks_by_parent(&mut items);
                                Ok(items)
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    ProjectMgmtProvider::Notion => {
                        match notion_client.query_database_with_children(true).await {
                            Ok(pages) => {
                                let parent_ids: std::collections::HashSet<String> = pages
                                    .iter()
                                    .filter_map(|p| p.parent_page_id.as_ref())
                                    .cloned()
                                    .collect();

                                let mut items: Vec<TaskListItem> = pages
                                    .into_iter()
                                    .map(|p| {
                                        let status_name = p
                                            .status_name
                                            .clone()
                                            .unwrap_or_else(|| "Unknown".to_string());
                                        let status =
                                            if status_name.to_lowercase().contains("progress") {
                                                TaskItemStatus::InProgress
                                            } else if status_name.to_lowercase().contains("done")
                                                || status_name.to_lowercase().contains("complete")
                                            {
                                                TaskItemStatus::Completed
                                            } else {
                                                TaskItemStatus::NotStarted
                                            };
                                        let completed = matches!(status, TaskItemStatus::Completed);
                                        let has_children = parent_ids.contains(&p.id);
                                        TaskListItem {
                                            id: p.id,
                                            identifier: None,
                                            name: p.name,
                                            status,
                                            status_name,
                                            url: p.url,
                                            parent_id: p.parent_page_id,
                                            has_children,
                                            completed,
                                        }
                                    })
                                    .collect();
                                sort_tasks_by_parent(&mut items);
                                Ok(items)
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    ProjectMgmtProvider::Clickup => {
                        match clickup_client.get_list_tasks_with_subtasks().await {
                            Ok(tasks) => {
                                let parent_ids: std::collections::HashSet<String> = tasks
                                    .iter()
                                    .filter_map(|t| t.parent_id.as_ref())
                                    .cloned()
                                    .collect();

                                let mut items: Vec<TaskListItem> = tasks
                                    .into_iter()
                                    .filter(|t| {
                                        if t.parent_id.is_some() {
                                            true
                                        } else {
                                            t.status_type != "closed"
                                        }
                                    })
                                    .map(|t| {
                                        let status_name = t.status.clone();
                                        let status =
                                            if status_name.to_lowercase().contains("progress")
                                                || status_name.to_lowercase().contains("review")
                                            {
                                                TaskItemStatus::InProgress
                                            } else if status_name.to_lowercase().contains("done")
                                                || status_name.to_lowercase().contains("complete")
                                                || status_name.to_lowercase().contains("closed")
                                            {
                                                TaskItemStatus::Completed
                                            } else {
                                                TaskItemStatus::NotStarted
                                            };
                                        let completed =
                                            t.status_type == "closed" || t.status_type == "done";
                                        let has_children = parent_ids.contains(&t.id);
                                        TaskListItem {
                                            id: t.id,
                                            identifier: None,
                                            name: t.name,
                                            status,
                                            status_name,
                                            url: t.url.unwrap_or_default(),
                                            parent_id: t.parent_id,
                                            has_children,
                                            completed,
                                        }
                                    })
                                    .collect();
                                sort_tasks_by_parent(&mut items);
                                Ok(items)
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    ProjectMgmtProvider::Airtable => {
                        match airtable_client.list_records_with_children().await {
                            Ok(tasks) => {
                                let parent_ids: std::collections::HashSet<String> = tasks
                                    .iter()
                                    .filter_map(|t| t.parent_id.as_ref())
                                    .cloned()
                                    .collect();

                                let mut items: Vec<TaskListItem> = tasks
                                    .into_iter()
                                    .map(|t| {
                                        let status_name = t
                                            .status
                                            .clone()
                                            .unwrap_or_else(|| "Unknown".to_string());
                                        let status =
                                            if status_name.to_lowercase().contains("progress") {
                                                TaskItemStatus::InProgress
                                            } else if status_name.to_lowercase().contains("done")
                                                || status_name.to_lowercase().contains("complete")
                                            {
                                                TaskItemStatus::Completed
                                            } else {
                                                TaskItemStatus::NotStarted
                                            };
                                        let completed = matches!(status, TaskItemStatus::Completed);
                                        let has_children = parent_ids.contains(&t.id);
                                        TaskListItem {
                                            id: t.id,
                                            identifier: None,
                                            name: t.name,
                                            status,
                                            status_name,
                                            url: t.url,
                                            parent_id: t.parent_id,
                                            has_children,
                                            completed,
                                        }
                                    })
                                    .collect();
                                sort_tasks_by_parent(&mut items);
                                Ok(items)
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    ProjectMgmtProvider::Linear => {
                        match linear_client.get_team_issues_with_children().await {
                            Ok(issues) => {
                                tracing::debug!("Linear issues before building task list:");
                                for i in &issues {
                                    tracing::debug!(
                                        "  {} - state_name: {}, state_type: {}, parent_id: {:?}",
                                        i.identifier,
                                        i.state_name,
                                        i.state_type,
                                        i.parent_id
                                    );
                                }

                                let parent_ids: std::collections::HashSet<String> = issues
                                    .iter()
                                    .filter_map(|i| i.parent_id.as_ref())
                                    .cloned()
                                    .collect();

                                let mut items: Vec<TaskListItem> = issues
                                    .into_iter()
                                    .filter(|i| {
                                        i.state_type != "completed" && i.state_type != "cancelled"
                                    })
                                    .map(|i| {
                                        let status = match i.state_type.as_str() {
                                            "started" => TaskItemStatus::InProgress,
                                            _ => TaskItemStatus::NotStarted,
                                        };
                                        let completed = i.state_type == "completed";
                                        let has_children = parent_ids.contains(&i.id);
                                        TaskListItem {
                                            id: i.id,
                                            identifier: Some(i.identifier),
                                            name: i.title,
                                            status,
                                            status_name: i.state_name,
                                            url: i.url,
                                            parent_id: i.parent_id,
                                            has_children,
                                            completed,
                                        }
                                    })
                                    .collect();
                                sort_tasks_by_parent(&mut items);
                                Ok(items)
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                };
                match result {
                    Ok(tasks) => {
                        let _ = tx.send(Action::TaskListFetched { tasks });
                    }
                    Err(msg) => {
                        let _ = tx.send(Action::TaskListFetchError { message: msg });
                    }
                }
            });
        }

        Action::TaskListFetched { tasks } => {
            state.task_list_loading = false;
            state.task_list = tasks.clone();
            state.task_list_selected = 0;
            state.task_list_expanded_ids = tasks
                .iter()
                .filter(|t| t.has_children)
                .map(|t| t.id.clone())
                .collect();
        }

        Action::TaskListFetchError { message } => {
            state.task_list_loading = false;
            state.show_error(format!("Failed to fetch tasks: {}", message));
            state.exit_input_mode();
        }

        Action::SelectTaskNext => {
            let visible_indices =
                compute_visible_task_indices(&state.task_list, &state.task_list_expanded_ids);
            if !visible_indices.is_empty() {
                let visible_pos = visible_indices
                    .iter()
                    .position(|&i| i == state.task_list_selected)
                    .unwrap_or(0);
                let next_pos = (visible_pos + 1) % visible_indices.len();
                state.task_list_selected = visible_indices[next_pos];
            }
        }

        Action::SelectTaskPrev => {
            let visible_indices =
                compute_visible_task_indices(&state.task_list, &state.task_list_expanded_ids);
            if !visible_indices.is_empty() {
                let visible_pos = visible_indices
                    .iter()
                    .position(|&i| i == state.task_list_selected)
                    .unwrap_or(0);
                let prev_pos = if visible_pos == 0 {
                    visible_indices.len() - 1
                } else {
                    visible_pos - 1
                };
                state.task_list_selected = visible_indices[prev_pos];
            }
        }

        Action::ToggleTaskExpand => {
            if let Some(task) = state.task_list.get(state.task_list_selected) {
                if task.has_children {
                    if state.task_list_expanded_ids.contains(&task.id) {
                        state.task_list_expanded_ids.remove(&task.id);
                    } else {
                        state.task_list_expanded_ids.insert(task.id.clone());
                    }
                }
            }
        }

        Action::ToggleSubtaskStatus => {
            if let Some(task) = state.task_list.get(state.task_list_selected).cloned() {
                let provider = state.settings.repo_config.project_mgmt.provider;

                // Notion: All tasks use full status dropdown
                if matches!(provider, ProjectMgmtProvider::Notion) {
                    if !notion_client.is_configured().await {
                        state
                            .show_error("Notion not configured. Set NOTION_TOKEN and database_id.");
                        return Ok(false);
                    }

                    let task_id_for_dropdown = task.id.clone();
                    let client = Arc::clone(notion_client);
                    let tx = action_tx.clone();
                    state.loading_message = Some("Loading status options...".to_string());

                    tokio::spawn(async move {
                        match client.get_status_options().await {
                            Ok(opts) => {
                                let options: Vec<StatusOption> = opts
                                    .all_options
                                    .into_iter()
                                    .map(|o| StatusOption {
                                        id: o.id,
                                        name: o.name,
                                    })
                                    .collect();
                                let _ = tx.send(Action::SubtaskStatusOptionsLoaded {
                                    task_id: task_id_for_dropdown,
                                    task_name: task.name.clone(),
                                    options,
                                });
                            }
                            Err(e) => {
                                tracing::error!("Failed to load Notion status options: {}", e);
                                let _ = tx.send(Action::SetLoading(None));
                                let _ = tx.send(Action::ShowError(format!(
                                    "Failed to load status options: {}",
                                    e
                                )));
                            }
                        }
                    });
                } else if matches!(provider, ProjectMgmtProvider::Linear) {
                    // Linear: All tasks (parent and subtask) use full status dropdown
                    if !linear_client.is_configured().await {
                        state.show_error("Linear not configured. Set LINEAR_TOKEN and team_id.");
                        return Ok(false);
                    }

                    let task_id_for_dropdown = task.id.clone();
                    let task_name_for_dropdown = task.name.clone();
                    let client = Arc::clone(linear_client);
                    let tx = action_tx.clone();
                    state.loading_message = Some("Loading workflow states...".to_string());

                    tokio::spawn(async move {
                        match client.get_workflow_states().await {
                            Ok(states) => {
                                let options: Vec<StatusOption> = states
                                    .into_iter()
                                    .map(|s| StatusOption {
                                        id: s.id,
                                        name: s.name,
                                    })
                                    .collect();
                                let _ = tx.send(Action::SubtaskStatusOptionsLoaded {
                                    task_id: task_id_for_dropdown,
                                    task_name: task_name_for_dropdown,
                                    options,
                                });
                            }
                            Err(e) => {
                                tracing::error!("Failed to load Linear workflow states: {}", e);
                                let _ = tx.send(Action::SetLoading(None));
                                let _ = tx.send(Action::ShowError(format!(
                                    "Failed to load workflow states: {}",
                                    e
                                )));
                            }
                        }
                    });
                } else if task.is_subtask() {
                    // ClickUp subtasks use the same statuses as parent tasks
                    if matches!(provider, ProjectMgmtProvider::Clickup) {
                        if !clickup_client.is_configured().await {
                            state.show_error(
                                "ClickUp not configured. Set CLICKUP_TOKEN and list_id.",
                            );
                            return Ok(false);
                        }

                        let task_id_for_dropdown = task.id.clone();
                        let client = Arc::clone(clickup_client);
                        let tx = action_tx.clone();
                        state.loading_message = Some("Loading statuses...".to_string());

                        tokio::spawn(async move {
                            match client.get_statuses().await {
                                Ok(statuses) => {
                                    let options: Vec<StatusOption> = statuses
                                        .into_iter()
                                        .map(|s| StatusOption {
                                            id: s.status.clone(),
                                            name: s.status,
                                        })
                                        .collect();
                                    let _ = tx.send(Action::SubtaskStatusOptionsLoaded {
                                        task_id: task_id_for_dropdown,
                                        task_name: task.name.clone(),
                                        options,
                                    });
                                }
                                Err(e) => {
                                    tracing::error!("Failed to load ClickUp statuses: {}", e);
                                    let _ = tx.send(Action::SetLoading(None));
                                    let _ = tx.send(Action::ShowError(format!(
                                        "Failed to load statuses: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    } else {
                        // Asana subtasks use simple complete/incomplete toggle
                        state.subtask_status_dropdown = Some(SubtaskStatusDropdownState {
                            task_id: task.id.clone(),
                            task_name: task.name.clone(),
                            current_completed: task.completed,
                            selected_index: if task.completed { 1 } else { 0 },
                        });
                        state.input_mode = Some(InputMode::SelectSubtaskStatus);
                    }
                } else if matches!(provider, ProjectMgmtProvider::Clickup) {
                    // ClickUp parent tasks also support status dropdown
                    if !clickup_client.is_configured().await {
                        state.show_error("ClickUp not configured. Set CLICKUP_TOKEN and list_id.");
                        return Ok(false);
                    }

                    let task_id_for_dropdown = task.id.clone();
                    let client = Arc::clone(clickup_client);
                    let tx = action_tx.clone();
                    state.loading_message = Some("Loading statuses...".to_string());

                    tokio::spawn(async move {
                        match client.get_statuses().await {
                            Ok(statuses) => {
                                let options: Vec<StatusOption> = statuses
                                    .into_iter()
                                    .map(|s| StatusOption {
                                        id: s.status.clone(),
                                        name: s.status,
                                    })
                                    .collect();
                                let _ = tx.send(Action::SubtaskStatusOptionsLoaded {
                                    task_id: task_id_for_dropdown,
                                    task_name: task.name.clone(),
                                    options,
                                });
                            }
                            Err(e) => {
                                tracing::error!("Failed to load ClickUp statuses: {}", e);
                                let _ = tx.send(Action::SetLoading(None));
                                let _ = tx.send(Action::ShowError(format!(
                                    "Failed to load statuses: {}",
                                    e
                                )));
                            }
                        }
                    });
                } else {
                    state.show_warning("Status toggle only available for subtasks");
                }
            }
        }

        Action::SubtaskStatusOptionsLoaded {
            task_id,
            task_name,
            options,
        } => {
            state.loading_message = None;
            if !options.is_empty() {
                state.task_status_dropdown = Some(TaskStatusDropdownState {
                    agent_id: Uuid::nil(), // Using nil UUID to indicate this is for a subtask
                    status_options: options,
                    selected_index: 0,
                });
                // Store task info for later use
                state.subtask_status_dropdown = Some(SubtaskStatusDropdownState {
                    task_id,
                    task_name,
                    current_completed: false, // Not used for ClickUp
                    selected_index: 0,
                });
                state.input_mode = Some(InputMode::SelectTaskStatus);
            } else {
                state.show_warning("No status options found");
            }
        }

        Action::SubtaskStatusOptionSelected {
            task_id,
            status_name,
        } => {
            let client = Arc::clone(clickup_client);
            let tx = action_tx.clone();
            let status_name_for_spawn = status_name.clone();
            let task_id_for_spawn = task_id.clone();
            state.loading_message = Some("Updating subtask status...".to_string());

            tokio::spawn(async move {
                match client
                    .update_task_status(&task_id_for_spawn, &status_name_for_spawn)
                    .await
                {
                    Ok(()) => {
                        let _ = tx.send(Action::SetLoading(None));
                        let _ = tx.send(Action::ShowToast {
                            message: format!("Subtask → {}", status_name_for_spawn),
                            level: ToastLevel::Success,
                        });
                        // Trigger a refresh of the task list
                        let _ = tx.send(Action::RefreshTaskList);
                    }
                    Err(e) => {
                        let _ = tx.send(Action::SetLoading(None));
                        let _ = tx.send(Action::ShowError(format!(
                            "Failed to update subtask: {}",
                            e
                        )));
                    }
                }
            });

            // Update local state
            if let Some(task) = state.task_list.iter_mut().find(|t| t.id == task_id) {
                let lower = status_name.to_lowercase();
                task.status_name = status_name.clone();
                task.status = if lower.contains("done")
                    || lower.contains("complete")
                    || lower.contains("closed")
                {
                    task.completed = true;
                    TaskItemStatus::Completed
                } else if lower.contains("progress") {
                    task.completed = false;
                    TaskItemStatus::InProgress
                } else {
                    task.completed = false;
                    TaskItemStatus::NotStarted
                };
            }
        }

        Action::SubtaskStatusDropdownSelect { completed } => {
            let dropdown = state.subtask_status_dropdown.take();
            state.exit_input_mode();
            if let Some(dropdown) = dropdown {
                let task_id = dropdown.task_id.clone();
                let task_name = dropdown.task_name.clone();
                let current_completed = dropdown.current_completed;

                if completed == current_completed {
                    state.show_info("No change needed");
                    return Ok(false);
                }

                let client = Arc::clone(asana_client);
                let tx = action_tx.clone();
                state.loading_message = Some("Updating subtask status...".to_string());

                tokio::spawn(async move {
                    let result = if completed {
                        client.complete_task(&task_id).await
                    } else {
                        client.uncomplete_task(&task_id).await
                    };

                    match result {
                        Ok(()) => {
                            let _ = tx.send(Action::SubtaskStatusUpdated { task_id, completed });
                            let _ = tx.send(Action::SetLoading(None));
                            let _ = tx.send(Action::ShowToast {
                                message: format!(
                                    "{} → {}",
                                    task_name,
                                    if completed {
                                        "Completed"
                                    } else {
                                        "Not Complete"
                                    }
                                ),
                                level: ToastLevel::Success,
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(Action::SetLoading(None));
                            let _ = tx.send(Action::ShowError(format!(
                                "Failed to update subtask: {}",
                                e
                            )));
                        }
                    }
                });
            }
        }

        Action::SubtaskStatusUpdated { task_id, completed } => {
            if let Some(task) = state.task_list.iter_mut().find(|t| t.id == task_id) {
                task.completed = completed;
                task.status = if completed {
                    TaskItemStatus::Completed
                } else {
                    TaskItemStatus::NotStarted
                };
                task.status_name = if completed {
                    "Completed".to_string()
                } else {
                    "Not Started".to_string()
                };
            }

            for agent in state.agents.values_mut() {
                if let Some(gid) = agent.pm_task_status.id() {
                    if gid == task_id {
                        let name = agent.pm_task_status.name().unwrap_or("").to_string();
                        let url = agent.pm_task_status.url().unwrap_or("").to_string();
                        let is_subtask = agent
                            .pm_task_status
                            .as_asana()
                            .map(|s| s.is_subtask())
                            .unwrap_or(false);
                        agent.pm_task_status = if completed {
                            ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::Completed {
                                gid: task_id.clone(),
                                name,
                                is_subtask,
                                status_name: "Complete".to_string(),
                            })
                        } else {
                            ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted {
                                gid: task_id.clone(),
                                name,
                                url,
                                is_subtask,
                                status_name: "Not Started".to_string(),
                            })
                        };
                        break;
                    }
                }
            }
        }

        Action::CreateAgentFromSelectedTask => {
            if let Some(task) = state.task_list.get(state.task_list_selected).cloned() {
                let provider = state.settings.repo_config.project_mgmt.provider;

                let (branch, name) = if provider == ProjectMgmtProvider::Linear {
                    let username = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .linear
                        .username
                        .clone();
                    match username {
                        Some(ref uname) if !uname.is_empty() => {
                            let identifier = task.identifier.clone().unwrap_or_default();
                            if identifier.is_empty() {
                                state.show_error("Linear task has no identifier");
                                return Ok(false);
                            }
                            let title_without_id = task
                                .name
                                .strip_prefix(&identifier)
                                .unwrap_or(&task.name)
                                .trim()
                                .trim_start_matches(['-', ' '])
                                .to_string();
                            let title = if title_without_id.is_empty() {
                                &task.name
                            } else {
                                &title_without_id
                            };
                            let branch = grove::core::common::sanitize_linear_branch_name(
                                uname,
                                &identifier,
                                title,
                            );
                            let name = format!("{} {}", identifier, title);
                            (branch, name)
                        }
                        _ => {
                            state.show_error(
                                "Linear username not configured. Please re-run Linear setup.",
                            );
                            return Ok(false);
                        }
                    }
                } else {
                    let branch = grove::core::common::sanitize_branch_name(&task.name);
                    if branch.is_empty() {
                        state.show_error("Invalid task name for branch");
                        return Ok(false);
                    }
                    (branch, task.name.clone())
                };

                state.log_info(format!("Creating agent '{}' on branch '{}'", name, branch));
                let ai_agent = state.config.global.ai_agent.clone();
                tracing::debug!(
                    "CreateAgentFromSelectedTask - name: {:?}, branch: {:?}, ai_agent: {:?}",
                    name,
                    branch,
                    ai_agent
                );
                let worktree_symlinks = state
                    .settings
                    .repo_config
                    .dev_server
                    .worktree_symlinks
                    .clone();
                match agent_manager.create_agent(&name, &branch, &ai_agent, &worktree_symlinks) {
                    Ok(mut agent) => {
                        state.log_info(format!("Agent '{}' created successfully", agent.name));

                        let pm_status = match provider {
                            ProjectMgmtProvider::Asana => {
                                ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted {
                                    gid: task.id.clone(),
                                    name: task.name.clone(),
                                    url: task.url.clone(),
                                    is_subtask: task.is_subtask(),
                                    status_name: task.status_name.clone(),
                                })
                            }
                            ProjectMgmtProvider::Notion => {
                                ProjectMgmtTaskStatus::Notion(NotionTaskStatus::Linked {
                                    page_id: task.id.clone(),
                                    name: task.name.clone(),
                                    url: task.url.clone(),
                                    status_option_id: String::new(),
                                    status_name: task.status_name.clone(),
                                })
                            }
                            ProjectMgmtProvider::Clickup => {
                                ProjectMgmtTaskStatus::ClickUp(ClickUpTaskStatus::NotStarted {
                                    id: task.id.clone(),
                                    name: task.name.clone(),
                                    url: task.url.clone(),
                                    status: task.status_name.clone(),
                                    is_subtask: task.is_subtask(),
                                })
                            }
                            ProjectMgmtProvider::Airtable => {
                                ProjectMgmtTaskStatus::Airtable(AirtableTaskStatus::NotStarted {
                                    id: task.id.clone(),
                                    name: task.name.clone(),
                                    url: task.url.clone(),
                                    is_subtask: task.is_subtask(),
                                })
                            }
                            ProjectMgmtProvider::Linear => {
                                let identifier = task.identifier.clone().unwrap_or_default();
                                ProjectMgmtTaskStatus::Linear(LinearTaskStatus::NotStarted {
                                    id: task.id.clone(),
                                    identifier,
                                    name: task.name.clone(),
                                    status_name: task.status_name.clone(),
                                    url: task.url.clone(),
                                    is_subtask: task.is_subtask(),
                                })
                            }
                        };
                        agent.pm_task_status = pm_status;
                        state.log_info(format!("Linked task '{}' to agent", task.name));

                        state.add_agent(agent);
                        state.select_last();
                        state.toast = None;
                        state.exit_input_mode();

                        let _ = agent_watch_tx.send(state.agents.keys().cloned().collect());
                        let _ = branch_watch_tx.send(
                            state
                                .agents
                                .values()
                                .map(|a| (a.id, a.branch.clone()))
                                .collect(),
                        );
                        let _ = selected_watch_tx.send(state.selected_agent_id());
                    }
                    Err(e) => {
                        state.log_error(format!("Failed to create agent: {}", e));
                        state.show_error(format!("Failed to create agent: {}", e));
                    }
                }
            }
        }

        Action::AssignSelectedTaskToAgent => {
            if let Some(task) = state.task_list.get(state.task_list_selected).cloned() {
                if let Some(agent_id) = state.selected_agent_id() {
                    let agent_current_task = state.agents.get(&agent_id).and_then(|a| {
                        if a.pm_task_status.is_linked() {
                            Some((
                                a.pm_task_status.id().unwrap_or_default().to_string(),
                                a.pm_task_status.name().unwrap_or_default().to_string(),
                            ))
                        } else {
                            None
                        }
                    });

                    let task_id_normalized = task.id.replace('-', "").to_lowercase();
                    let task_current_agent = state.agents.values().find_map(|a| {
                        let agent_task_id = a
                            .pm_task_status
                            .id()
                            .map(|id| id.replace('-', "").to_lowercase());
                        if agent_task_id.as_deref() == Some(&task_id_normalized) {
                            Some((a.id, a.name.clone()))
                        } else {
                            None
                        }
                    });

                    if agent_current_task.is_some() || task_current_agent.is_some() {
                        state.task_reassignment_warning =
                            Some(grove::app::TaskReassignmentWarning {
                                target_agent_id: agent_id,
                                task_id: task.id.clone(),
                                task_name: task.name.clone(),
                                agent_current_task,
                                task_current_agent,
                            });
                    } else {
                        state.exit_input_mode();
                        action_tx.send(Action::AssignProjectTask {
                            id: agent_id,
                            url_or_id: task.id.clone(),
                        })?;
                    }
                } else {
                    state.show_warning("No agent selected");
                }
            }
        }

        Action::ConfirmTaskReassignment => {
            if let Some(warning) = state.task_reassignment_warning.take() {
                if let Some((old_agent_id, old_agent_name)) = warning.task_current_agent {
                    if let Some(old_agent) = state.agents.get_mut(&old_agent_id) {
                        old_agent.pm_task_status = ProjectMgmtTaskStatus::None;
                    }
                    state.log_info(format!("Removed task from agent '{}'", old_agent_name));
                }

                state.exit_input_mode();
                action_tx.send(Action::AssignProjectTask {
                    id: warning.target_agent_id,
                    url_or_id: warning.task_id,
                })?;
            }
        }

        Action::DismissTaskReassignmentWarning => {
            state.task_reassignment_warning = None;
        }

        // UI state
        Action::ToggleDiffView => {
            // Diff view removed for simplicity
        }

        Action::ToggleHelp => {
            state.show_help = !state.show_help;
        }

        Action::ToggleLogs => {
            state.show_logs = !state.show_logs;
        }

        Action::ToggleStatusDebug => {
            if state.config.global.debug_mode {
                state.show_status_debug = !state.show_status_debug;
            } else {
                state.show_info("Debug mode is disabled. Enable it in Settings > General.");
            }
        }

        Action::ShowError(msg) => {
            state.toast = Some(Toast::new(msg, ToastLevel::Error));
        }

        Action::ShowToast { message, level } => {
            state.toast = Some(Toast::new(message, level));
        }

        Action::ClearError => {
            state.toast = None;
        }

        Action::EnterInputMode(mode) => {
            state.enter_input_mode(mode.clone());
            if mode == InputMode::BrowseTasks {
                state.task_list_loading = true;
                state.task_list.clear();
                state.task_list_selected = 0;
                let _ = action_tx.send(Action::FetchTaskList);
            }
        }

        Action::ExitInputMode => {
            state.exit_input_mode();
        }

        Action::UpdateInput(input) => {
            state.input_buffer = input;
        }

        Action::SubmitInput => {
            if let Some(mode) = state.input_mode.clone() {
                let input = state.input_buffer.clone();
                state.exit_input_mode();

                match mode {
                    InputMode::NewAgent => {
                        if !input.is_empty() {
                            let branch = grove::core::common::sanitize_branch_name(&input);
                            if branch.is_empty() {
                                action_tx.send(Action::ShowError(
                                    "Invalid name: name cannot be only spaces".to_string(),
                                ))?;
                            } else {
                                action_tx.send(Action::CreateAgent {
                                    name: input.trim().to_string(),
                                    branch,
                                    task: None,
                                })?;
                            }
                        }
                    }
                    InputMode::SetNote => {
                        if let Some(id) = state.selected_agent_id() {
                            let note = if input.is_empty() { None } else { Some(input) };
                            action_tx.send(Action::SetAgentNote { id, note })?;
                        }
                    }
                    InputMode::ConfirmDelete => {
                        // Confirmation already validated by key handler (y pressed)
                        if let Some(id) = state.selected_agent_id() {
                            action_tx.send(Action::DeleteAgent { id })?;
                        }
                    }
                    InputMode::ConfirmMerge => {
                        // Send merge main prompt to the agent
                        if let Some(id) = state.selected_agent_id() {
                            action_tx.send(Action::MergeMain { id })?;
                        }
                    }
                    InputMode::ConfirmPush => {
                        // Send /push command to the agent
                        if let Some(id) = state.selected_agent_id() {
                            action_tx.send(Action::PushBranch { id })?;
                        }
                    }
                    InputMode::AssignAsana => {
                        if !input.is_empty() {
                            if let Some(id) = state.selected_agent_id() {
                                action_tx.send(Action::AssignAsanaTask {
                                    id,
                                    url_or_gid: input,
                                })?;
                            }
                        }
                    }
                    InputMode::AssignProjectTask => {
                        if !input.is_empty() {
                            if let Some(agent_id) = state.selected_agent_id() {
                                let agent_current_task =
                                    state.agents.get(&agent_id).and_then(|a| {
                                        if a.pm_task_status.is_linked() {
                                            Some((
                                                a.pm_task_status
                                                    .id()
                                                    .unwrap_or_default()
                                                    .to_string(),
                                                a.pm_task_status
                                                    .name()
                                                    .unwrap_or_default()
                                                    .to_string(),
                                            ))
                                        } else {
                                            None
                                        }
                                    });

                                let input_normalized = input.replace('-', "").to_lowercase();
                                let parts: Vec<&str> = input_normalized.split('/').collect();
                                let task_id_part = parts.last().unwrap_or(&"").to_string();

                                let task_current_agent = state.agents.values().find_map(|a| {
                                    let agent_task_id = a
                                        .pm_task_status
                                        .id()
                                        .map(|id| id.replace('-', "").to_lowercase());
                                    if agent_task_id.as_deref() == Some(&task_id_part) {
                                        Some((a.id, a.name.clone()))
                                    } else {
                                        None
                                    }
                                });

                                if agent_current_task.is_some() || task_current_agent.is_some() {
                                    state.task_reassignment_warning =
                                        Some(grove::app::TaskReassignmentWarning {
                                            target_agent_id: agent_id,
                                            task_id: input.clone(),
                                            task_name: input.clone(),
                                            agent_current_task,
                                            task_current_agent,
                                        });
                                } else {
                                    state.exit_input_mode();
                                    action_tx.send(Action::AssignProjectTask {
                                        id: agent_id,
                                        url_or_id: input,
                                    })?;
                                }
                            }
                        }
                    }
                    InputMode::ConfirmDeleteAsana => {
                        // Handled directly by key handler (y/n/Esc), not through SubmitInput
                    }
                    InputMode::ConfirmDeleteTask => {
                        // Handled directly by key handler (y/n/Esc), not through SubmitInput
                    }
                    InputMode::BrowseTasks => {
                        // Handled by SelectTaskNext/Prev and CreateAgentFromSelectedTask
                    }
                    InputMode::SelectTaskStatus => {
                        // Handled by TaskStatusDropdownNext/Prev/Select
                    }
                    InputMode::SelectSubtaskStatus => {
                        // Handled by SubtaskStatusDropdownNext/Prev/Select
                    }
                }
            }
        }

        // Clipboard
        Action::CopyAgentName { id } => {
            if let Some(agent) = state.agents.get(&id) {
                let name = agent.name.clone();
                match Clipboard::new().and_then(|mut c| c.set_text(&name)) {
                    Ok(()) => {
                        state.show_success(format!("Copied '{}'", name));
                    }
                    Err(e) => {
                        state.show_error(format!("Copy failed: {}", e));
                    }
                }
            }
        }

        // Application
        Action::RefreshAll => {
            state.show_info("Refreshing...");

            if let Some(agent) = state.selected_agent() {
                let git_sync = GitSync::new(&agent.worktree_path);
                if let Ok(status) = git_sync.get_status(&state.settings.repo_config.git.main_branch)
                {
                    let id = agent.id;
                    action_tx.send(Action::UpdateGitStatus { id, status })?;
                }
            }
        }

        Action::RefreshSelected => {
            state.show_info("Refreshing...");

            if let Some(agent) = state.selected_agent() {
                let id = agent.id;
                let branch = agent.branch.clone();
                let branch_for_gitlab = branch.clone();
                let branch_for_github = branch.clone();
                let branch_for_codeberg = branch.clone();
                let worktree_path = agent.worktree_path.clone();
                let main_branch = state.settings.repo_config.git.main_branch.clone();

                // Refresh git status
                let git_sync = GitSync::new(&worktree_path);
                if let Ok(status) = git_sync.get_status(&main_branch) {
                    action_tx.send(Action::UpdateGitStatus { id, status })?;
                }

                // Refresh GitLab MR status
                let gitlab_client_clone = Arc::clone(gitlab_client);
                let tx_clone = action_tx.clone();
                tokio::spawn(async move {
                    let status = gitlab_client_clone
                        .get_mr_for_branch(&branch_for_gitlab)
                        .await;
                    if !matches!(
                        status,
                        grove::core::git_providers::gitlab::MergeRequestStatus::None
                    ) {
                        let _ = tx_clone.send(Action::UpdateMrStatus { id, status });
                    }
                });

                // Refresh GitHub PR status
                let github_client_clone = Arc::clone(github_client);
                let tx_clone = action_tx.clone();
                tokio::spawn(async move {
                    let status = github_client_clone
                        .get_pr_for_branch(&branch_for_github)
                        .await;
                    if !matches!(
                        status,
                        grove::core::git_providers::github::PullRequestStatus::None
                    ) {
                        let _ = tx_clone.send(Action::UpdatePrStatus { id, status });
                    }
                });

                // Refresh Codeberg PR status
                let codeberg_client_clone = Arc::clone(codeberg_client);
                let tx_clone = action_tx.clone();
                tokio::spawn(async move {
                    let status = codeberg_client_clone
                        .get_pr_for_branch(&branch_for_codeberg)
                        .await;
                    if !matches!(
                        status,
                        grove::core::git_providers::codeberg::PullRequestStatus::None
                    ) {
                        let _ = tx_clone.send(Action::UpdateCodebergPrStatus { id, status });
                    }
                });

                match &agent.pm_task_status {
                    ProjectMgmtTaskStatus::Asana(asana_status) => {
                        if let Some(task_gid) = asana_status.gid() {
                            let asana_client_clone = Arc::clone(asana_client);
                            let tx_clone = action_tx.clone();
                            let gid = task_gid.to_string();
                            tokio::spawn(async move {
                                if let Ok(task) = asana_client_clone.get_task(&gid).await {
                                    let url = task.permalink_url.unwrap_or_else(|| {
                                        format!("https://app.asana.com/0/0/{}/f", task.gid)
                                    });
                                    let is_subtask = task.parent.is_some();
                                    let status = if task.completed {
                                        grove::core::projects::asana::AsanaTaskStatus::Completed {
                                            gid: task.gid,
                                            name: task.name,
                                            is_subtask,
                                            status_name: "Complete".to_string(),
                                        }
                                    } else {
                                        grove::core::projects::asana::AsanaTaskStatus::InProgress {
                                            gid: task.gid,
                                            name: task.name,
                                            url,
                                            is_subtask,
                                            status_name: "In Progress".to_string(),
                                        }
                                    };
                                    let _ = tx_clone.send(Action::UpdateProjectTaskStatus {
                                        id,
                                        status: ProjectMgmtTaskStatus::Asana(status),
                                    });
                                }
                            });
                        }
                    }
                    ProjectMgmtTaskStatus::Notion(notion_status) => {
                        if let Some(page_id) = notion_status.page_id() {
                            let notion_client_clone = Arc::clone(notion_client);
                            let tx_clone = action_tx.clone();
                            let pid = page_id.to_string();
                            tokio::spawn(async move {
                                if let Ok(page) = notion_client_clone.get_page(&pid).await {
                                    let status = NotionTaskStatus::Linked {
                                        page_id: page.id,
                                        name: page.name,
                                        url: page.url,
                                        status_option_id: page.status_id.unwrap_or_default(),
                                        status_name: page.status_name.unwrap_or_default(),
                                    };
                                    let _ = tx_clone.send(Action::UpdateProjectTaskStatus {
                                        id,
                                        status: ProjectMgmtTaskStatus::Notion(status),
                                    });
                                }
                            });
                        }
                    }
                    ProjectMgmtTaskStatus::ClickUp(clickup_status) => {
                        if let Some(task_id) = clickup_status.id() {
                            let clickup_client_clone = Arc::clone(clickup_client);
                            let tx_clone = action_tx.clone();
                            let tid = task_id.to_string();
                            tokio::spawn(async move {
                                if let Ok(task) = clickup_client_clone.get_task(&tid).await {
                                    let url = task.url.clone().unwrap_or_default();
                                    let is_subtask = task.parent.is_some();
                                    let status = if task.status.status_type == "closed" {
                                        ClickUpTaskStatus::Completed {
                                            id: task.id,
                                            name: task.name,
                                            is_subtask,
                                        }
                                    } else {
                                        ClickUpTaskStatus::InProgress {
                                            id: task.id,
                                            name: task.name,
                                            url,
                                            status: task.status.status,
                                            is_subtask,
                                        }
                                    };
                                    let _ = tx_clone.send(Action::UpdateProjectTaskStatus {
                                        id,
                                        status: ProjectMgmtTaskStatus::ClickUp(status),
                                    });
                                }
                            });
                        }
                    }
                    ProjectMgmtTaskStatus::Airtable(airtable_status) => {
                        if let Some(record_id) = airtable_status.id() {
                            let airtable_client_clone = Arc::clone(airtable_client);
                            let tx_clone = action_tx.clone();
                            let rid = record_id.to_string();
                            tokio::spawn(async move {
                                if let Ok(record) = airtable_client_clone.get_record(&rid).await {
                                    let status_name = record.status.clone().unwrap_or_default();
                                    let is_subtask = record.parent_id.is_some();
                                    let is_completed = status_name.to_lowercase().contains("done")
                                        || status_name.to_lowercase().contains("complete");
                                    let status = if is_completed {
                                        AirtableTaskStatus::Completed {
                                            id: record.id,
                                            name: record.name,
                                            is_subtask,
                                        }
                                    } else if status_name.to_lowercase().contains("progress") {
                                        AirtableTaskStatus::InProgress {
                                            id: record.id,
                                            name: record.name,
                                            url: record.url,
                                            is_subtask,
                                        }
                                    } else {
                                        AirtableTaskStatus::NotStarted {
                                            id: record.id,
                                            name: record.name,
                                            url: record.url,
                                            is_subtask,
                                        }
                                    };
                                    let _ = tx_clone.send(Action::UpdateProjectTaskStatus {
                                        id,
                                        status: ProjectMgmtTaskStatus::Airtable(status),
                                    });
                                }
                            });
                        }
                    }
                    ProjectMgmtTaskStatus::Linear(linear_status) => {
                        if let Some(issue_id) = linear_status.id() {
                            let linear_client_clone = Arc::clone(linear_client);
                            let tx_clone = action_tx.clone();
                            let iid = issue_id.to_string();
                            tokio::spawn(async move {
                                if let Ok(issue) = linear_client_clone.get_issue(&iid).await {
                                    let is_subtask = issue.parent_id.is_some();
                                    let status = match issue.state_type.as_str() {
                                        "completed" | "cancelled" => LinearTaskStatus::Completed {
                                            id: issue.id,
                                            identifier: issue.identifier,
                                            name: issue.title,
                                            status_name: issue.state_name,
                                            is_subtask,
                                        },
                                        "started" => LinearTaskStatus::InProgress {
                                            id: issue.id,
                                            identifier: issue.identifier,
                                            name: issue.title,
                                            status_name: issue.state_name,
                                            url: issue.url,
                                            is_subtask,
                                        },
                                        _ => LinearTaskStatus::NotStarted {
                                            id: issue.id,
                                            identifier: issue.identifier,
                                            name: issue.title,
                                            status_name: issue.state_name,
                                            url: issue.url,
                                            is_subtask,
                                        },
                                    };
                                    let _ = tx_clone.send(Action::UpdateProjectTaskStatus {
                                        id,
                                        status: ProjectMgmtTaskStatus::Linear(status),
                                    });
                                }
                            });
                        }
                    }
                    ProjectMgmtTaskStatus::None => {}
                }
            }
        }

        Action::Tick => {
            state.advance_animation();
            if let Some(ref toast) = state.toast {
                if toast.is_expired() {
                    state.toast = None;
                }
            }
        }

        Action::RecordActivity { id, had_activity } => {
            if let Some(agent) = state.agents.get_mut(&id) {
                agent.record_activity(had_activity);
            }
        }

        Action::UpdateChecklistProgress { id, progress } => {
            if let Some(agent) = state.agents.get_mut(&id) {
                agent.checklist_progress = progress;
            }
        }

        Action::UpdateGlobalSystemMetrics {
            cpu_percent,
            memory_used,
            memory_total,
        } => {
            state.record_system_metrics(cpu_percent, memory_used, memory_total);
        }

        Action::SetLoading(message) => {
            state.loading_message = message;
        }

        Action::UpdatePreviewContent(content) => {
            state.preview_content = content;
        }

        Action::DeleteAgentComplete {
            id,
            success,
            message,
        } => {
            state.loading_message = None;
            if success {
                state.remove_agent(id);
                state.log_info(&message);
                let _ = agent_watch_tx.send(state.agents.keys().cloned().collect());
                let _ = branch_watch_tx.send(
                    state
                        .agents
                        .values()
                        .map(|a| (a.id, a.branch.clone()))
                        .collect(),
                );
                let _ = selected_watch_tx.send(state.selected_agent_id());
            } else {
                state.log_error(&message);
            }
            state.show_info(message);
        }

        Action::PauseAgentComplete {
            id,
            success,
            message,
        } => {
            state.loading_message = None;
            if success {
                if let Some(agent) = state.agents.get_mut(&id) {
                    agent.status = grove::agent::AgentStatus::Paused;
                }
                state.log_info(&message);
                state.show_success(message);
            } else {
                state.log_error(&message);
                state.show_error(message);
            }
        }

        Action::ResumeAgentComplete {
            id,
            success,
            message,
        } => {
            state.loading_message = None;
            if success {
                if let Some(agent) = state.agents.get_mut(&id) {
                    agent.status = grove::agent::AgentStatus::Running;
                }
                state.log_info(&message);
                state.show_success(message);
            } else {
                state.log_error(&message);
                state.show_error(message);
            }
        }

        // Settings actions
        Action::ToggleSettings => {
            if state.settings.active {
                state.settings.active = false;
            } else {
                state.settings.active = true;
                state.settings.tab = grove::app::SettingsTab::General;
                state.settings.field_index = 0;
                state.settings.scroll_offset = 0;
                state.settings.dropdown = grove::app::DropdownState::Closed;
                state.settings.editing_text = false;
                state.settings.pending_ai_agent = state.config.global.ai_agent.clone();
                state.settings.pending_editor = state.config.global.editor.clone();
                state.settings.pending_log_level = state.config.global.log_level;
                state.settings.pending_worktree_location = state.config.global.worktree_location;
                state.settings.pending_debug_mode = state.config.global.debug_mode;
                state.settings.pending_ui = state.config.ui.clone();
            }
        }

        Action::SettingsSwitchSection => {
            state.settings.tab = state.settings.next_tab();
            state.settings.field_index = 0;
            state.settings.scroll_offset = 0;
            state.settings.dropdown = grove::app::DropdownState::Closed;
            state.settings.editing_text = false;
        }

        Action::SettingsSwitchSectionBack => {
            state.settings.tab = state.settings.prev_tab();
            state.settings.field_index = 0;
            state.settings.scroll_offset = 0;
            state.settings.dropdown = grove::app::DropdownState::Closed;
            state.settings.editing_text = false;
        }

        Action::SettingsSelectNext => {
            if state.settings.editing_text {
            } else {
                let total = state.settings.total_fields();
                state.settings.field_index = (state.settings.field_index + 1) % total;
            }
        }

        Action::SettingsSelectPrev => {
            if state.settings.editing_text {
            } else {
                let total = state.settings.total_fields();
                state.settings.field_index = if state.settings.field_index == 0 {
                    total.saturating_sub(1)
                } else {
                    state.settings.field_index - 1
                };
            }
        }

        Action::SettingsDropdownPrev => {
            if let grove::app::DropdownState::Open { selected_index } = &state.settings.dropdown {
                state.settings.dropdown = grove::app::DropdownState::Open {
                    selected_index: selected_index.saturating_sub(1),
                };
            }
        }

        Action::SettingsDropdownNext => {
            let field = state.settings.current_field();
            if let grove::app::DropdownState::Open { selected_index } = &state.settings.dropdown {
                let max = match field {
                    grove::app::SettingsField::AiAgent => grove::app::AiAgent::all().len(),
                    grove::app::SettingsField::GitProvider => grove::app::GitProvider::all().len(),
                    grove::app::SettingsField::LogLevel => grove::app::ConfigLogLevel::all().len(),
                    grove::app::SettingsField::WorktreeLocation => {
                        grove::app::WorktreeLocation::all().len()
                    }
                    grove::app::SettingsField::CodebergCiProvider => {
                        grove::app::CodebergCiProvider::all().len()
                    }
                    grove::app::SettingsField::ProjectMgmtProvider => {
                        grove::app::ProjectMgmtProvider::all().len()
                    }
                    _ => 0,
                };
                state.settings.dropdown = grove::app::DropdownState::Open {
                    selected_index: (*selected_index + 1).min(max.saturating_sub(1)),
                };
            }
        }

        Action::SettingsSelectField => {
            let field = state.settings.current_field();
            match field {
                grove::app::SettingsField::AiAgent => {
                    let current = &state.settings.pending_ai_agent;
                    let idx = grove::app::AiAgent::all()
                        .iter()
                        .position(|a| a == current)
                        .unwrap_or(0);
                    state.settings.dropdown = grove::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                grove::app::SettingsField::Editor => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state.settings.pending_editor.clone();
                }
                grove::app::SettingsField::GitProvider => {
                    let current = &state.settings.repo_config.git.provider;
                    let idx = grove::app::GitProvider::all()
                        .iter()
                        .position(|g| g == current)
                        .unwrap_or(0);
                    state.settings.dropdown = grove::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                grove::app::SettingsField::LogLevel => {
                    let current = &state.settings.pending_log_level;
                    let idx = grove::app::ConfigLogLevel::all()
                        .iter()
                        .position(|l| l == current)
                        .unwrap_or(0);
                    state.settings.dropdown = grove::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                grove::app::SettingsField::WorktreeLocation => {
                    let current = &state.settings.pending_worktree_location;
                    let idx = grove::app::WorktreeLocation::all()
                        .iter()
                        .position(|w| w == current)
                        .unwrap_or(0);
                    state.settings.dropdown = grove::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                grove::app::SettingsField::CodebergCiProvider => {
                    let current = &state.settings.repo_config.git.codeberg.ci_provider;
                    let idx = grove::app::CodebergCiProvider::all()
                        .iter()
                        .position(|c| c == current)
                        .unwrap_or(0);
                    state.settings.dropdown = grove::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                grove::app::SettingsField::BranchPrefix => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.git.branch_prefix.clone();
                }
                grove::app::SettingsField::MainBranch => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state.settings.repo_config.git.main_branch.clone();
                }
                grove::app::SettingsField::WorktreeSymlinks => {
                    state.settings.init_file_browser(&state.repo_path);
                }
                grove::app::SettingsField::GitLabProjectId => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .git
                        .gitlab
                        .project_id
                        .map(|id| id.to_string())
                        .unwrap_or_default();
                }
                grove::app::SettingsField::GitLabBaseUrl => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.git.gitlab.base_url.clone();
                }
                grove::app::SettingsField::GitHubOwner => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .git
                        .github
                        .owner
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::GitHubRepo => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .git
                        .github
                        .repo
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::CodebergOwner => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .git
                        .codeberg
                        .owner
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::CodebergRepo => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .git
                        .codeberg
                        .repo
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::CodebergBaseUrl => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.git.codeberg.base_url.clone();
                }
                grove::app::SettingsField::AsanaProjectGid => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .asana
                        .project_gid
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::AsanaInProgressGid => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .asana
                        .in_progress_section_gid
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::AsanaDoneGid => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .asana
                        .done_section_gid
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::SummaryPrompt => {
                    state.settings.editing_prompt = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .prompts
                        .summary_prompt
                        .clone()
                        .unwrap_or_else(|| {
                            state
                                .settings
                                .repo_config
                                .prompts
                                .get_summary_prompt()
                                .to_string()
                        });
                }
                grove::app::SettingsField::MergePrompt => {
                    state.settings.editing_prompt = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .prompts
                        .merge_prompt
                        .clone()
                        .unwrap_or_else(|| {
                            state
                                .settings
                                .repo_config
                                .prompts
                                .get_merge_prompt(&state.settings.repo_config.git.main_branch)
                        });
                }
                grove::app::SettingsField::PushPrompt => {
                    let agent = &state.settings.pending_ai_agent;
                    if agent.push_command().is_some() {
                        state.show_warning(format!(
                            "{} uses /push command, no prompt to configure",
                            agent.display_name()
                        ));
                        return Ok(false);
                    }
                    let default_prompt = agent.push_prompt().unwrap_or("");
                    state.settings.editing_prompt = true;
                    let current = match agent {
                        grove::app::AiAgent::Opencode => {
                            &state.settings.repo_config.prompts.push_prompt_opencode
                        }
                        grove::app::AiAgent::Codex => {
                            &state.settings.repo_config.prompts.push_prompt_codex
                        }
                        grove::app::AiAgent::Gemini => {
                            &state.settings.repo_config.prompts.push_prompt_gemini
                        }
                        grove::app::AiAgent::ClaudeCode => &None,
                    };
                    state.settings.text_buffer = current
                        .clone()
                        .unwrap_or_else(|| default_prompt.to_string());
                }
                grove::app::SettingsField::ShowPreview => {
                    state.settings.pending_ui.show_preview =
                        !state.settings.pending_ui.show_preview;
                    state.config.ui.show_preview = state.settings.pending_ui.show_preview;
                    state.show_logs = state.config.ui.show_logs;
                }
                grove::app::SettingsField::ShowMetrics => {
                    state.settings.pending_ui.show_metrics =
                        !state.settings.pending_ui.show_metrics;
                    state.config.ui.show_metrics = state.settings.pending_ui.show_metrics;
                }
                grove::app::SettingsField::ShowLogs => {
                    state.settings.pending_ui.show_logs = !state.settings.pending_ui.show_logs;
                    state.config.ui.show_logs = state.settings.pending_ui.show_logs;
                    state.show_logs = state.config.ui.show_logs;
                }
                grove::app::SettingsField::ShowBanner => {
                    state.settings.pending_ui.show_banner = !state.settings.pending_ui.show_banner;
                    state.config.ui.show_banner = state.settings.pending_ui.show_banner;
                }
                grove::app::SettingsField::DebugMode => {
                    state.settings.pending_debug_mode = !state.settings.pending_debug_mode;
                    state.config.global.debug_mode = state.settings.pending_debug_mode;
                }
                grove::app::SettingsField::ProjectMgmtProvider => {
                    let current = state.settings.repo_config.project_mgmt.provider;
                    let idx = grove::app::ProjectMgmtProvider::all()
                        .iter()
                        .position(|p| *p == current)
                        .unwrap_or(0);
                    state.settings.dropdown = grove::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                grove::app::SettingsField::NotionDatabaseId => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .notion
                        .database_id
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::NotionStatusProperty => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .notion
                        .status_property_name
                        .clone()
                        .unwrap_or_else(|| "Status".to_string());
                }
                grove::app::SettingsField::NotionInProgressOption => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .notion
                        .in_progress_option
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::NotionDoneOption => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .notion
                        .done_option
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::ClickUpListId => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .clickup
                        .list_id
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::ClickUpInProgressStatus => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .clickup
                        .in_progress_status
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::ClickUpDoneStatus => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .clickup
                        .done_status
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::AirtableBaseId => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .airtable
                        .base_id
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::AirtableTableName => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .airtable
                        .table_name
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::AirtableStatusField => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .airtable
                        .status_field_name
                        .clone()
                        .unwrap_or_else(|| "Status".to_string());
                }
                grove::app::SettingsField::AirtableInProgressOption => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .airtable
                        .in_progress_option
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::AirtableDoneOption => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .airtable
                        .done_option
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::SetupPm => {
                    state.settings.active = false;
                    state.pm_setup.active = true;
                    state.pm_setup.step = grove::app::state::PmSetupStep::Token;
                    state.pm_setup.teams.clear();
                    state.pm_setup.error = None;
                    state.pm_setup.selected_team_index = 0;
                    state.pm_setup.field_index = 0;
                    state.pm_setup.advanced_expanded = false;
                    state.pm_setup.manual_team_id.clear();
                    state.pm_setup.in_progress_state.clear();
                    state.pm_setup.done_state.clear();
                }
                grove::app::SettingsField::LinearTeamId => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .linear
                        .team_id
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::LinearInProgressState => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .linear
                        .in_progress_state
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::LinearDoneState => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .project_mgmt
                        .linear
                        .done_state
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::DevServerCommand => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .dev_server
                        .command
                        .clone()
                        .unwrap_or_default();
                }
                grove::app::SettingsField::DevServerRunBefore => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.dev_server.run_before.join(", ");
                }
                grove::app::SettingsField::DevServerWorkingDir => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.dev_server.working_dir.clone();
                }
                grove::app::SettingsField::DevServerPort => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .dev_server
                        .port
                        .map(|p| p.to_string())
                        .unwrap_or_default();
                }
                grove::app::SettingsField::DevServerAutoStart => {
                    state.settings.repo_config.dev_server.auto_start =
                        !state.settings.repo_config.dev_server.auto_start;
                }
                _ => {
                    // Keybind fields are handled by SettingsStartKeybindCapture
                }
            }
        }

        Action::SettingsConfirmSelection => {
            if state.settings.editing_text || state.settings.editing_prompt {
                let field = state.settings.current_field();
                match field {
                    grove::app::SettingsField::BranchPrefix => {
                        state.settings.repo_config.git.branch_prefix =
                            state.settings.text_buffer.clone();
                    }
                    grove::app::SettingsField::MainBranch => {
                        state.settings.repo_config.git.main_branch =
                            state.settings.text_buffer.clone();
                    }
                    grove::app::SettingsField::WorktreeSymlinks => {
                        state.settings.repo_config.dev_server.worktree_symlinks = state
                            .settings
                            .text_buffer
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    grove::app::SettingsField::GitLabProjectId => {
                        state.settings.repo_config.git.gitlab.project_id =
                            state.settings.text_buffer.parse().ok();
                        gitlab_client.reconfigure(
                            &state.settings.repo_config.git.gitlab.base_url,
                            state.settings.repo_config.git.gitlab.project_id,
                            grove::app::Config::gitlab_token().as_deref(),
                        );
                    }
                    grove::app::SettingsField::GitLabBaseUrl => {
                        state.settings.repo_config.git.gitlab.base_url =
                            state.settings.text_buffer.clone();
                        gitlab_client.reconfigure(
                            &state.settings.repo_config.git.gitlab.base_url,
                            state.settings.repo_config.git.gitlab.project_id,
                            grove::app::Config::gitlab_token().as_deref(),
                        );
                    }
                    grove::app::SettingsField::GitHubOwner => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.git.github.owner =
                            if val.is_empty() { None } else { Some(val) };
                        github_client.reconfigure(
                            state.settings.repo_config.git.github.owner.as_deref(),
                            state.settings.repo_config.git.github.repo.as_deref(),
                            grove::app::Config::github_token().as_deref(),
                        );
                    }
                    grove::app::SettingsField::GitHubRepo => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.git.github.repo =
                            if val.is_empty() { None } else { Some(val) };
                        github_client.reconfigure(
                            state.settings.repo_config.git.github.owner.as_deref(),
                            state.settings.repo_config.git.github.repo.as_deref(),
                            grove::app::Config::github_token().as_deref(),
                        );
                    }
                    grove::app::SettingsField::CodebergOwner => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.git.codeberg.owner =
                            if val.is_empty() { None } else { Some(val) };
                        codeberg_client.reconfigure(
                            state.settings.repo_config.git.codeberg.owner.as_deref(),
                            state.settings.repo_config.git.codeberg.repo.as_deref(),
                            Some(&state.settings.repo_config.git.codeberg.base_url),
                            grove::app::Config::codeberg_token().as_deref(),
                            state.settings.repo_config.git.codeberg.ci_provider,
                            grove::app::Config::woodpecker_token().as_deref(),
                            state.settings.repo_config.git.codeberg.woodpecker_repo_id,
                        );
                    }
                    grove::app::SettingsField::CodebergRepo => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.git.codeberg.repo =
                            if val.is_empty() { None } else { Some(val) };
                        codeberg_client.reconfigure(
                            state.settings.repo_config.git.codeberg.owner.as_deref(),
                            state.settings.repo_config.git.codeberg.repo.as_deref(),
                            Some(&state.settings.repo_config.git.codeberg.base_url),
                            grove::app::Config::codeberg_token().as_deref(),
                            state.settings.repo_config.git.codeberg.ci_provider,
                            grove::app::Config::woodpecker_token().as_deref(),
                            state.settings.repo_config.git.codeberg.woodpecker_repo_id,
                        );
                    }
                    grove::app::SettingsField::CodebergBaseUrl => {
                        state.settings.repo_config.git.codeberg.base_url =
                            state.settings.text_buffer.clone();
                        codeberg_client.reconfigure(
                            state.settings.repo_config.git.codeberg.owner.as_deref(),
                            state.settings.repo_config.git.codeberg.repo.as_deref(),
                            Some(&state.settings.repo_config.git.codeberg.base_url),
                            grove::app::Config::codeberg_token().as_deref(),
                            state.settings.repo_config.git.codeberg.ci_provider,
                            grove::app::Config::woodpecker_token().as_deref(),
                            state.settings.repo_config.git.codeberg.woodpecker_repo_id,
                        );
                    }
                    grove::app::SettingsField::AsanaProjectGid => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.asana.project_gid =
                            if val.is_empty() { None } else { Some(val) };
                        asana_client.reconfigure(
                            grove::app::Config::asana_token().as_deref(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .asana
                                .project_gid
                                .clone(),
                        );
                    }
                    grove::app::SettingsField::AsanaInProgressGid => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .asana
                            .in_progress_section_gid =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::AsanaDoneGid => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .asana
                            .done_section_gid = if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::DevServerCommand => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.dev_server.command =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::DevServerRunBefore => {
                        state.settings.repo_config.dev_server.run_before = state
                            .settings
                            .text_buffer
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    grove::app::SettingsField::DevServerWorkingDir => {
                        state.settings.repo_config.dev_server.working_dir =
                            state.settings.text_buffer.clone();
                    }
                    grove::app::SettingsField::DevServerPort => {
                        state.settings.repo_config.dev_server.port =
                            state.settings.text_buffer.parse().ok();
                    }
                    grove::app::SettingsField::SummaryPrompt => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.prompts.summary_prompt =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::MergePrompt => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.prompts.merge_prompt =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::PushPrompt => {
                        let val = state.settings.text_buffer.clone();
                        match state.settings.pending_ai_agent {
                            grove::app::AiAgent::Opencode => {
                                state.settings.repo_config.prompts.push_prompt_opencode =
                                    if val.is_empty() { None } else { Some(val) };
                            }
                            grove::app::AiAgent::Codex => {
                                state.settings.repo_config.prompts.push_prompt_codex =
                                    if val.is_empty() { None } else { Some(val) };
                            }
                            grove::app::AiAgent::Gemini => {
                                state.settings.repo_config.prompts.push_prompt_gemini =
                                    if val.is_empty() { None } else { Some(val) };
                            }
                            grove::app::AiAgent::ClaudeCode => {}
                        }
                    }
                    grove::app::SettingsField::NotionDatabaseId => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.notion.database_id =
                            if val.is_empty() { None } else { Some(val) };
                        notion_client.reconfigure(
                            grove::app::Config::notion_token().as_deref(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .notion
                                .database_id
                                .clone(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .notion
                                .status_property_name
                                .clone(),
                        );
                    }
                    grove::app::SettingsField::NotionStatusProperty => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .notion
                            .status_property_name = if val.is_empty() { None } else { Some(val) };
                        notion_client.reconfigure(
                            grove::app::Config::notion_token().as_deref(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .notion
                                .database_id
                                .clone(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .notion
                                .status_property_name
                                .clone(),
                        );
                    }
                    grove::app::SettingsField::NotionInProgressOption => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .notion
                            .in_progress_option = if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::NotionDoneOption => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.notion.done_option =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::ClickUpListId => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.clickup.list_id =
                            if val.is_empty() { None } else { Some(val) };
                        clickup_client.reconfigure(
                            grove::app::Config::clickup_token().as_deref(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .clickup
                                .list_id
                                .clone(),
                        );
                    }
                    grove::app::SettingsField::ClickUpInProgressStatus => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .clickup
                            .in_progress_status = if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::ClickUpDoneStatus => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.clickup.done_status =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::AirtableBaseId => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.airtable.base_id =
                            if val.is_empty() { None } else { Some(val) };
                        airtable_client.reconfigure(
                            grove::app::Config::airtable_token().as_deref(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .base_id
                                .clone(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .table_name
                                .clone(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .status_field_name
                                .clone(),
                        );
                    }
                    grove::app::SettingsField::AirtableTableName => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.airtable.table_name =
                            if val.is_empty() { None } else { Some(val) };
                        airtable_client.reconfigure(
                            grove::app::Config::airtable_token().as_deref(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .base_id
                                .clone(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .table_name
                                .clone(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .status_field_name
                                .clone(),
                        );
                    }
                    grove::app::SettingsField::AirtableStatusField => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .airtable
                            .status_field_name = if val.is_empty() { None } else { Some(val) };
                        airtable_client.reconfigure(
                            grove::app::Config::airtable_token().as_deref(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .base_id
                                .clone(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .table_name
                                .clone(),
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .status_field_name
                                .clone(),
                        );
                    }
                    grove::app::SettingsField::AirtableInProgressOption => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .airtable
                            .in_progress_option = if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::AirtableDoneOption => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.airtable.done_option =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::LinearTeamId => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.linear.team_id =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::LinearInProgressState => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .linear
                            .in_progress_state = if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::LinearDoneState => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.linear.done_state =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    grove::app::SettingsField::Editor => {
                        state.settings.pending_editor = state.settings.text_buffer.clone();
                    }
                    _ => {}
                }
                state.settings.editing_text = false;
                state.settings.editing_prompt = false;
                state.settings.text_buffer.clear();
            } else if let grove::app::DropdownState::Open { selected_index } =
                state.settings.dropdown
            {
                let field = state.settings.current_field();
                match field {
                    grove::app::SettingsField::AiAgent => {
                        if let Some(agent) = grove::app::AiAgent::all().get(selected_index) {
                            state.settings.pending_ai_agent = agent.clone();
                            state.config.global.ai_agent = agent.clone();
                        }
                    }
                    grove::app::SettingsField::GitProvider => {
                        if let Some(provider) = grove::app::GitProvider::all().get(selected_index) {
                            state.settings.repo_config.git.provider = *provider;
                        }
                    }
                    grove::app::SettingsField::LogLevel => {
                        if let Some(level) = grove::app::ConfigLogLevel::all().get(selected_index) {
                            state.settings.pending_log_level = *level;
                            state.config.global.log_level = *level;
                        }
                    }
                    grove::app::SettingsField::WorktreeLocation => {
                        if let Some(loc) = grove::app::WorktreeLocation::all().get(selected_index) {
                            state.settings.pending_worktree_location = *loc;
                            state.config.global.worktree_location = *loc;
                            state.worktree_base = state.config.worktree_base_path(&state.repo_path);
                        }
                    }
                    grove::app::SettingsField::CodebergCiProvider => {
                        if let Some(provider) =
                            grove::app::CodebergCiProvider::all().get(selected_index)
                        {
                            state.settings.repo_config.git.codeberg.ci_provider = *provider;
                        }
                    }
                    grove::app::SettingsField::ProjectMgmtProvider => {
                        if let Some(provider) =
                            grove::app::ProjectMgmtProvider::all().get(selected_index)
                        {
                            state.settings.repo_config.project_mgmt.provider = *provider;
                        }
                    }
                    _ => {}
                }
                state.settings.dropdown = grove::app::DropdownState::Closed;
            }
        }

        Action::SettingsCancelSelection => {
            state.settings.dropdown = grove::app::DropdownState::Closed;
            state.settings.editing_text = false;
            state.settings.editing_prompt = false;
            state.settings.text_buffer.clear();
        }

        Action::SettingsPromptSave => {
            let field = state.settings.current_field();
            match field {
                grove::app::SettingsField::SummaryPrompt => {
                    let val = state.settings.text_buffer.clone();
                    state.settings.repo_config.prompts.summary_prompt =
                        if val.is_empty() { None } else { Some(val) };
                }
                grove::app::SettingsField::MergePrompt => {
                    let val = state.settings.text_buffer.clone();
                    state.settings.repo_config.prompts.merge_prompt =
                        if val.is_empty() { None } else { Some(val) };
                }
                grove::app::SettingsField::PushPrompt => {
                    let val = state.settings.text_buffer.clone();
                    match state.settings.pending_ai_agent {
                        grove::app::AiAgent::Opencode => {
                            state.settings.repo_config.prompts.push_prompt_opencode =
                                if val.is_empty() { None } else { Some(val) };
                        }
                        grove::app::AiAgent::Codex => {
                            state.settings.repo_config.prompts.push_prompt_codex =
                                if val.is_empty() { None } else { Some(val) };
                        }
                        grove::app::AiAgent::Gemini => {
                            state.settings.repo_config.prompts.push_prompt_gemini =
                                if val.is_empty() { None } else { Some(val) };
                        }
                        grove::app::AiAgent::ClaudeCode => {}
                    }
                }
                _ => {}
            }
            state.show_success("Saved");
        }

        Action::SettingsInputChar(c) => {
            state.settings.text_buffer.push(c);
        }

        Action::SettingsBackspace => {
            state.settings.text_buffer.pop();
        }

        Action::SettingsClose => {
            if let Err(e) = state.config.save() {
                state.log_error(format!("Failed to save config: {}", e));
            }
            if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                state.log_error(format!("Failed to save repo config: {}", e));
            }
            state.settings.active = false;
        }

        Action::SettingsSave => {
            let old_provider = state.settings.repo_config.project_mgmt.provider;

            state.config.global.ai_agent = state.settings.pending_ai_agent.clone();
            state.config.global.editor = state.settings.pending_editor.clone();
            state.config.global.log_level = state.settings.pending_log_level;
            state.config.global.worktree_location = state.settings.pending_worktree_location;
            state.config.global.debug_mode = state.settings.pending_debug_mode;
            state.config.ui = state.settings.pending_ui.clone();
            state.config.keybinds = state.settings.pending_keybinds.clone();

            if let Err(e) = state.config.save() {
                state.log_error(format!("Failed to save config: {}", e));
            }

            if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                state.log_error(format!("Failed to save repo config: {}", e));
            }

            let new_provider = state.settings.repo_config.project_mgmt.provider;
            if old_provider != new_provider {
                state.task_list.clear();
                state.task_list_loading = true;
                let _ = action_tx.send(Action::FetchTaskList);
            }

            state.show_logs = state.config.ui.show_logs;
            state.worktree_base = state.config.worktree_base_path(&state.repo_path);
            state.settings.active = false;
            state.log_info("Settings saved".to_string());
        }

        Action::SettingsStartKeybindCapture => {
            let field = state.settings.current_field();
            if field.is_keybind_field() {
                state.settings.capturing_keybind = Some(field);
            }
        }

        Action::SettingsCaptureKeybind { key, modifiers } => {
            if let Some(field) = state.settings.capturing_keybind {
                use grove::app::config::Keybind;
                let keybind = Keybind::with_modifiers(key, modifiers);
                state.settings.set_keybind(field, keybind);
                state.settings.capturing_keybind = None;
            }
        }

        Action::SettingsCancelKeybindCapture => {
            state.settings.capturing_keybind = None;
        }

        Action::SettingsRequestReset { reset_type } => {
            state.settings.reset_confirmation = Some(reset_type);
        }

        Action::SettingsConfirmReset => {
            if let Some(reset_type) = state.settings.reset_confirmation {
                match reset_type {
                    grove::app::ResetType::CurrentTab => {
                        state.settings.reset_current_tab();
                    }
                    grove::app::ResetType::AllSettings => {
                        state.settings.reset_all();
                    }
                }
                state.settings.reset_confirmation = None;

                let old_provider = state.settings.repo_config.project_mgmt.provider;

                state.config.global.ai_agent = state.settings.pending_ai_agent.clone();
                state.config.global.editor = state.settings.pending_editor.clone();
                state.config.global.log_level = state.settings.pending_log_level;
                state.config.global.worktree_location = state.settings.pending_worktree_location;
                state.config.ui = state.settings.pending_ui.clone();
                state.config.keybinds = state.settings.pending_keybinds.clone();

                if let Err(e) = state.config.save() {
                    state.log_error(format!("Failed to save config: {}", e));
                }

                if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                    state.log_error(format!("Failed to save repo config: {}", e));
                }

                let new_provider = state.settings.repo_config.project_mgmt.provider;
                if old_provider != new_provider {
                    state.task_list.clear();
                    state.task_list_loading = true;
                    let _ = action_tx.send(Action::FetchTaskList);
                }

                state.show_logs = state.config.ui.show_logs;
                state.worktree_base = state.config.worktree_base_path(&state.repo_path);
                state.settings.active = false;

                match reset_type {
                    grove::app::ResetType::CurrentTab => {
                        state.log_info(format!(
                            "{} settings reset to defaults",
                            state.settings.tab.display_name()
                        ));
                    }
                    grove::app::ResetType::AllSettings => {
                        state.log_info("All settings reset to defaults");
                    }
                }
            }
        }

        Action::SettingsCancelReset => {
            state.settings.reset_confirmation = None;
        }

        // File Browser Actions
        Action::SettingsCloseFileBrowser => {
            let repo_path = std::path::PathBuf::from(&state.repo_path);
            let selected: Vec<String> = state
                .settings
                .file_browser
                .selected_files
                .iter()
                .filter_map(|p| {
                    p.strip_prefix(&repo_path)
                        .ok()
                        .map(|s| s.to_string_lossy().to_string())
                })
                .collect();

            state.settings.repo_config.dev_server.worktree_symlinks = selected;

            if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                state.log_error(format!("Failed to save repo config: {}", e));
            }

            let symlinks = state
                .settings
                .repo_config
                .dev_server
                .worktree_symlinks
                .clone();
            let worktree = Worktree::new(&state.repo_path, state.worktree_base.clone());

            let agent_worktrees: Vec<(String, String)> = state
                .agents
                .values()
                .map(|a| (a.name.clone(), a.worktree_path.clone()))
                .collect();

            let mut refreshed_count = 0;
            let mut errors = Vec::new();
            for (name, worktree_path) in agent_worktrees {
                if std::path::Path::new(&worktree_path).exists() {
                    if let Err(e) = worktree.create_symlinks(&worktree_path, &symlinks) {
                        errors.push(format!("{}: {}", name, e));
                    } else {
                        refreshed_count += 1;
                    }
                }
            }

            for error in errors {
                state.log_error(format!("Failed to create symlinks for {}", error));
            }

            state.settings.file_browser.active = false;
            state.log_info(format!(
                "Symlinks saved and refreshed for {} worktrees",
                refreshed_count
            ));
        }

        Action::FileBrowserToggle => {
            let fb = &mut state.settings.file_browser;
            if let Some(entry) = fb.entries.get(fb.selected_index) {
                if !entry.is_dir || entry.name == ".." {
                    if fb.selected_files.contains(&entry.path) {
                        fb.selected_files.remove(&entry.path);
                    } else {
                        fb.selected_files.insert(entry.path.clone());
                    }
                    fb.entries = grove::ui::components::file_browser::load_directory_entries(
                        &fb.current_path,
                        &fb.selected_files,
                        &fb.current_path,
                    );
                }
            }
        }

        Action::FileBrowserSelectNext => {
            let fb = &mut state.settings.file_browser;
            fb.selected_index = (fb.selected_index + 1).min(fb.entries.len().saturating_sub(1));
        }

        Action::FileBrowserSelectPrev => {
            let fb = &mut state.settings.file_browser;
            fb.selected_index = fb.selected_index.saturating_sub(1);
        }

        Action::FileBrowserEnterDir => {
            let fb = &mut state.settings.file_browser;
            if let Some(entry) = fb.entries.get(fb.selected_index) {
                if entry.is_dir {
                    fb.current_path = entry.path.clone();
                    fb.selected_index = 0;
                    fb.entries = grove::ui::components::file_browser::load_directory_entries(
                        &fb.current_path,
                        &fb.selected_files,
                        &fb.current_path,
                    );
                }
            }
        }

        Action::FileBrowserGoParent => {
            let fb = &mut state.settings.file_browser;
            if let Some(parent) = fb.current_path.parent() {
                fb.current_path = parent.to_path_buf();
                fb.selected_index = 0;
                fb.entries = grove::ui::components::file_browser::load_directory_entries(
                    &fb.current_path,
                    &fb.selected_files,
                    &fb.current_path,
                );
            }
        }

        // Global Setup Wizard Actions
        Action::GlobalSetupNextStep => {
            if let Some(wizard) = &mut state.global_setup {
                wizard.step = grove::app::GlobalSetupStep::AgentSettings;
            }
        }
        Action::GlobalSetupPrevStep => {
            if let Some(wizard) = &mut state.global_setup {
                wizard.step = grove::app::GlobalSetupStep::WorktreeLocation;
            }
        }
        Action::GlobalSetupSelectNext => {
            if let Some(wizard) = &mut state.global_setup {
                let all = grove::app::config::WorktreeLocation::all();
                let current_idx = all
                    .iter()
                    .position(|l| *l == wizard.worktree_location)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % all.len();
                wizard.worktree_location = all[next_idx];
            }
        }
        Action::GlobalSetupSelectPrev => {
            if let Some(wizard) = &mut state.global_setup {
                let all = grove::app::config::WorktreeLocation::all();
                let current_idx = all
                    .iter()
                    .position(|l| *l == wizard.worktree_location)
                    .unwrap_or(0);
                let prev_idx = if current_idx == 0 {
                    all.len() - 1
                } else {
                    current_idx - 1
                };
                wizard.worktree_location = all[prev_idx];
            }
        }
        Action::GlobalSetupNavigateUp => {
            if let Some(wizard) = &mut state.global_setup {
                if wizard.field_index > 0 {
                    wizard.field_index -= 1;
                }
            }
        }
        Action::GlobalSetupNavigateDown => {
            if let Some(wizard) = &mut state.global_setup {
                if wizard.field_index < 1 {
                    wizard.field_index += 1;
                }
            }
        }
        Action::GlobalSetupToggleDropdown => {
            if let Some(wizard) = &mut state.global_setup {
                wizard.dropdown_open = !wizard.dropdown_open;
                // Set dropdown_index to current value
                if wizard.field_index == 0 {
                    wizard.dropdown_index = grove::app::config::AiAgent::all()
                        .iter()
                        .position(|a| *a == wizard.ai_agent)
                        .unwrap_or(0);
                } else {
                    wizard.dropdown_index = grove::app::config::LogLevel::all()
                        .iter()
                        .position(|l| *l == wizard.log_level)
                        .unwrap_or(0);
                }
            }
        }
        Action::GlobalSetupDropdownPrev => {
            if let Some(wizard) = &mut state.global_setup {
                if wizard.dropdown_index > 0 {
                    wizard.dropdown_index -= 1;
                }
            }
        }
        Action::GlobalSetupDropdownNext => {
            if let Some(wizard) = &mut state.global_setup {
                let max = if wizard.field_index == 0 {
                    grove::app::config::AiAgent::all().len()
                } else {
                    grove::app::config::LogLevel::all().len()
                };
                if wizard.dropdown_index < max.saturating_sub(1) {
                    wizard.dropdown_index += 1;
                }
            }
        }
        Action::GlobalSetupConfirmDropdown => {
            if let Some(wizard) = &mut state.global_setup {
                if wizard.field_index == 0 {
                    let all_agents = grove::app::config::AiAgent::all();
                    if wizard.dropdown_index < all_agents.len() {
                        wizard.ai_agent = all_agents[wizard.dropdown_index].clone();
                    }
                } else {
                    let all_levels = grove::app::config::LogLevel::all();
                    if wizard.dropdown_index < all_levels.len() {
                        wizard.log_level = all_levels[wizard.dropdown_index];
                    }
                }
                wizard.dropdown_open = false;
            }
        }
        Action::GlobalSetupComplete => {
            if let Some(wizard) = state.global_setup.take() {
                state.config.global.ai_agent = wizard.ai_agent;
                state.config.global.log_level = wizard.log_level;
                state.config.global.worktree_location = wizard.worktree_location;

                state.worktree_base = state.config.worktree_base_path(&state.repo_path);

                if let Err(e) = state.config.save() {
                    state.log_error(format!("Failed to save config: {}", e));
                }

                state.show_global_setup = false;
                state.log_info("Global setup complete".to_string());

                // Show project setup if needed
                let repo_config_path = grove::app::RepoConfig::config_path(&state.repo_path).ok();
                let project_needs_setup = repo_config_path
                    .as_ref()
                    .map(|p| !p.exists())
                    .unwrap_or(true);
                if project_needs_setup {
                    state.show_project_setup = true;
                    let wizard = grove::app::ProjectSetupState {
                        config: state.settings.repo_config.clone(),
                        ..Default::default()
                    };
                    state.project_setup = Some(wizard);
                }
            }
        }

        // Project Setup Wizard Actions
        Action::ProjectSetupNavigateNext => {
            if let Some(wizard) = &mut state.project_setup {
                if wizard.selected_index < 5 {
                    wizard.selected_index += 1;
                }
            }
        }
        Action::ProjectSetupNavigatePrev => {
            if let Some(wizard) = &mut state.project_setup {
                if wizard.selected_index > 0 {
                    wizard.selected_index -= 1;
                }
            }
        }
        Action::ProjectSetupSelect => {
            if let Some(wizard) = &mut state.project_setup {
                match wizard.selected_index {
                    0 => {
                        wizard.git_provider_dropdown_open = true;
                        let all_providers = grove::app::config::GitProvider::all();
                        wizard.git_provider_dropdown_index = all_providers
                            .iter()
                            .position(|p| *p == wizard.config.git.provider)
                            .unwrap_or(0);
                    }
                    1 => {
                        state.git_setup.active = true;
                        state.git_setup.source = grove::app::state::SetupSource::ProjectSetup;
                        state.git_setup.step = grove::app::state::GitSetupStep::Token;
                        state.git_setup.error = None;
                        state.git_setup.field_index = 0;
                        state.git_setup.advanced_expanded = false;
                        state.git_setup.editing_text = false;
                        state.git_setup.dropdown_open = false;
                        state.git_setup.dropdown_index = 0;
                        state.git_setup.text_buffer.clear();
                        state.git_setup.loading = false;
                        state.git_setup.project_id.clear();
                        state.git_setup.owner.clear();
                        state.git_setup.repo.clear();
                        state.git_setup.base_url.clear();
                        state.git_setup.detected_from_remote = false;
                        state.git_setup.project_name = None;
                        state.git_setup.ci_provider =
                            grove::app::config::CodebergCiProvider::default();
                        state.git_setup.woodpecker_repo_id.clear();

                        // Try to auto-detect from git remote
                        let provider = wizard.config.git.provider;
                        if let Some(remote_info) = grove::git::parse_remote_info(&state.repo_path) {
                            state.git_setup.owner = remote_info.owner.clone();
                            state.git_setup.repo = remote_info.repo.clone();
                            state.git_setup.detected_from_remote = remote_info.provider == provider;
                            if remote_info.provider == provider {
                                if let Some(url) = remote_info.base_url {
                                    state.git_setup.base_url = url;
                                }
                            }
                        }
                    }
                    2 => {
                        wizard.pm_provider_dropdown_open = true;
                        let all_providers = grove::app::config::ProjectMgmtProvider::all();
                        wizard.pm_provider_dropdown_index = all_providers
                            .iter()
                            .position(|p| *p == wizard.config.project_mgmt.provider)
                            .unwrap_or(0);
                    }
                    3 => {
                        state.pm_setup.active = true;
                        state.pm_setup.source = grove::app::state::SetupSource::ProjectSetup;
                        state.pm_setup.step = grove::app::state::PmSetupStep::Token;
                        state.pm_setup.teams.clear();
                        state.pm_setup.all_databases.clear();
                        state.pm_setup.teams_loading = false;
                        state.pm_setup.error = None;
                        state.pm_setup.selected_team_index = 0;
                        state.pm_setup.field_index = 0;
                        state.pm_setup.advanced_expanded = false;
                        state.pm_setup.manual_team_id.clear();
                        state.pm_setup.in_progress_state.clear();
                        state.pm_setup.done_state.clear();
                    }
                    4 => {
                        if let Some(wizard) = state.project_setup.take() {
                            if let Err(e) = wizard.config.save(&state.repo_path) {
                                state.log_error(format!("Failed to save project config: {}", e));
                            } else {
                                state.settings.repo_config = wizard.config.clone();
                                state.log_info("Project setup complete".to_string());
                            }
                        }
                        state.show_project_setup = false;
                    }
                    5 => {
                        state.show_project_setup = false;
                        state.project_setup = None;
                        state.log_info("Project setup closed".to_string());
                    }
                    _ => {}
                }
            }
        }
        Action::ProjectSetupToggleDropdown => {
            if let Some(wizard) = &mut state.project_setup {
                wizard.git_provider_dropdown_open = false;
                wizard.pm_provider_dropdown_open = false;
            }
        }
        Action::ProjectSetupDropdownPrev => {
            if let Some(wizard) = &mut state.project_setup {
                if wizard.git_provider_dropdown_index > 0 {
                    wizard.git_provider_dropdown_index -= 1;
                }
            }
        }
        Action::ProjectSetupDropdownNext => {
            if let Some(wizard) = &mut state.project_setup {
                let max = grove::app::config::GitProvider::all().len();
                if wizard.git_provider_dropdown_index < max.saturating_sub(1) {
                    wizard.git_provider_dropdown_index += 1;
                }
            }
        }
        Action::ProjectSetupConfirmDropdown => {
            if let Some(wizard) = &mut state.project_setup {
                let all_providers = grove::app::config::GitProvider::all();
                if wizard.git_provider_dropdown_index < all_providers.len() {
                    wizard.config.git.provider = all_providers[wizard.git_provider_dropdown_index];
                    state.settings.repo_config.git.provider =
                        all_providers[wizard.git_provider_dropdown_index];
                }
                wizard.git_provider_dropdown_open = false;
            }
        }
        Action::ProjectSetupPmDropdownPrev => {
            if let Some(wizard) = &mut state.project_setup {
                if wizard.pm_provider_dropdown_index > 0 {
                    wizard.pm_provider_dropdown_index -= 1;
                }
            }
        }
        Action::ProjectSetupPmDropdownNext => {
            if let Some(wizard) = &mut state.project_setup {
                let max = grove::app::config::ProjectMgmtProvider::all().len();
                if wizard.pm_provider_dropdown_index < max.saturating_sub(1) {
                    wizard.pm_provider_dropdown_index += 1;
                }
            }
        }
        Action::ProjectSetupConfirmPmDropdown => {
            if let Some(wizard) = &mut state.project_setup {
                let all_providers = grove::app::config::ProjectMgmtProvider::all();
                if wizard.pm_provider_dropdown_index < all_providers.len() {
                    wizard.config.project_mgmt.provider =
                        all_providers[wizard.pm_provider_dropdown_index];
                    state.settings.repo_config.project_mgmt.provider =
                        all_providers[wizard.pm_provider_dropdown_index];
                }
                wizard.pm_provider_dropdown_open = false;
            }
        }
        Action::ProjectSetupSkip => {
            state.show_project_setup = false;
            state.project_setup = None;
            state.log_info("Project setup closed".to_string());
        }
        Action::ProjectSetupComplete => {
            if let Some(wizard) = state.project_setup.take() {
                if let Err(e) = wizard.config.save(&state.repo_path) {
                    state.log_error(format!("Failed to save project config: {}", e));
                } else {
                    state.settings.repo_config = wizard.config.clone();
                    state.log_info("Project setup complete".to_string());
                }
            }
            state.show_project_setup = false;
        }

        // PM Setup Wizard Actions
        Action::OpenPmSetup => {
            state.pm_setup.active = true;
            state.pm_setup.source = grove::app::state::SetupSource::Settings;
            state.pm_setup.step = grove::app::state::PmSetupStep::Token;
            state.pm_setup.teams.clear();
            state.pm_setup.all_databases.clear();
            state.pm_setup.error = None;
            state.pm_setup.selected_team_index = 0;
            state.pm_setup.field_index = 0;
            state.pm_setup.advanced_expanded = false;
            state.pm_setup.manual_team_id.clear();
            state.pm_setup.in_progress_state.clear();
            state.pm_setup.done_state.clear();
        }
        Action::ClosePmSetup => {
            state.pm_setup.active = false;
            if state.pm_setup.source == grove::app::state::SetupSource::Settings {
                state.settings.active = true;
            }
            state.pm_setup.source = grove::app::state::SetupSource::default();
        }
        Action::PmSetupNextStep => match state.pm_setup.step {
            grove::app::state::PmSetupStep::Token => {
                state.pm_setup.step = grove::app::state::PmSetupStep::Workspace;
                let provider = state.settings.repo_config.project_mgmt.provider;
                if state.pm_setup.teams.is_empty() {
                    match provider {
                        grove::app::config::ProjectMgmtProvider::Linear
                            if grove::app::Config::linear_token().is_some() =>
                        {
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let linear_client = Arc::clone(linear_client);
                            tokio::spawn(async move {
                                match linear_client.get_teams().await {
                                    Ok(teams) => {
                                        let _ = tx.send(Action::PmSetupTeamsLoaded { teams });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                        grove::app::config::ProjectMgmtProvider::Notion
                            if grove::app::Config::notion_token().is_some() =>
                        {
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let token = grove::app::Config::notion_token().unwrap();
                            tokio::spawn(async move {
                                match grove::core::projects::notion::fetch_databases(&token).await {
                                    Ok(databases) => {
                                        let parent_pages =
                                            grove::core::projects::notion::extract_parent_pages(
                                                &databases,
                                            );
                                        let _ = tx.send(Action::PmSetupNotionDatabasesLoaded {
                                            databases,
                                            parent_pages,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                        grove::app::config::ProjectMgmtProvider::Asana
                            if grove::app::Config::asana_token().is_some() =>
                        {
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let asana_client = Arc::clone(asana_client);
                            tokio::spawn(async move {
                                match asana_client.fetch_workspaces().await {
                                    Ok(workspaces) => {
                                        let _ = tx
                                            .send(Action::PmSetupTeamsLoaded { teams: workspaces });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                        grove::app::config::ProjectMgmtProvider::Clickup
                            if grove::app::Config::clickup_token().is_some() =>
                        {
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let token = grove::app::Config::clickup_token().unwrap();
                            tokio::spawn(async move {
                                match grove::core::projects::clickup::fetch_teams(&token).await {
                                    Ok(teams) => {
                                        let _ = tx.send(Action::PmSetupTeamsLoaded { teams });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                        grove::app::config::ProjectMgmtProvider::Airtable
                            if grove::app::Config::airtable_token().is_some() =>
                        {
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let token = grove::app::Config::airtable_token().unwrap();
                            tokio::spawn(async move {
                                match grove::core::projects::airtable::fetch_bases(&token).await {
                                    Ok(bases) => {
                                        let _ =
                                            tx.send(Action::PmSetupTeamsLoaded { teams: bases });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                        _ => {}
                    }
                }
            }
            grove::app::state::PmSetupStep::Workspace => {
                let provider = state.settings.repo_config.project_mgmt.provider;
                match provider {
                    grove::app::config::ProjectMgmtProvider::Notion => {
                        if let Some(parent_page) = state
                            .pm_setup
                            .teams
                            .get(state.pm_setup.selected_team_index)
                            .map(|t| t.0.clone())
                        {
                            state.pm_setup.selected_workspace_gid = Some(parent_page.clone());
                            state.pm_setup.step = grove::app::state::PmSetupStep::Project;
                            let filtered: Vec<(String, String, String)> = state
                                .pm_setup
                                .all_databases
                                .iter()
                                .filter(|(_, _, parent)| parent == &parent_page)
                                .map(|(id, title, _)| (id.clone(), title.clone(), String::new()))
                                .collect();
                            state.pm_setup.teams = filtered;
                            state.pm_setup.selected_team_index = 0;
                        }
                    }
                    grove::app::config::ProjectMgmtProvider::Asana => {
                        if let Some(ws_gid) = state
                            .pm_setup
                            .teams
                            .get(state.pm_setup.selected_team_index)
                            .map(|t| t.0.clone())
                        {
                            state.pm_setup.selected_workspace_gid = Some(ws_gid.clone());
                            state.pm_setup.step = grove::app::state::PmSetupStep::Project;
                            state.pm_setup.teams.clear();
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let asana_client = Arc::clone(asana_client);
                            tokio::spawn(async move {
                                match asana_client.fetch_projects(&ws_gid).await {
                                    Ok(projects) => {
                                        let _ =
                                            tx.send(Action::PmSetupTeamsLoaded { teams: projects });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                    }
                    grove::app::config::ProjectMgmtProvider::Clickup => {
                        if let Some(team_id) = state
                            .pm_setup
                            .teams
                            .get(state.pm_setup.selected_team_index)
                            .map(|t| t.0.clone())
                        {
                            state.pm_setup.selected_workspace_gid = Some(team_id.clone());
                            state.pm_setup.step = grove::app::state::PmSetupStep::Project;
                            state.pm_setup.teams.clear();
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let token = grove::app::Config::clickup_token().unwrap();
                            tokio::spawn(async move {
                                match grove::core::projects::clickup::fetch_lists_for_team(
                                    &token, &team_id,
                                )
                                .await
                                {
                                    Ok(lists) => {
                                        let _ =
                                            tx.send(Action::PmSetupTeamsLoaded { teams: lists });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                    }
                    grove::app::config::ProjectMgmtProvider::Airtable => {
                        if let Some(base_id) = state
                            .pm_setup
                            .teams
                            .get(state.pm_setup.selected_team_index)
                            .map(|t| t.0.clone())
                        {
                            state.pm_setup.selected_workspace_gid = Some(base_id.clone());
                            state.pm_setup.step = grove::app::state::PmSetupStep::Project;
                            state.pm_setup.teams.clear();
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let token = grove::app::Config::airtable_token().unwrap();
                            tokio::spawn(async move {
                                match grove::core::projects::airtable::fetch_tables(
                                    &token, &base_id,
                                )
                                .await
                                {
                                    Ok(tables) => {
                                        let _ =
                                            tx.send(Action::PmSetupTeamsLoaded { teams: tables });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                    }
                    _ => {
                        state.pm_setup.step = grove::app::state::PmSetupStep::Advanced;
                    }
                }
            }
            grove::app::state::PmSetupStep::Project => {
                state.pm_setup.step = grove::app::state::PmSetupStep::Advanced;
            }
            grove::app::state::PmSetupStep::Advanced => {}
        },
        Action::PmSetupPrevStep => match state.pm_setup.step {
            grove::app::state::PmSetupStep::Token => {
                state.pm_setup.active = false;
                if state.pm_setup.source == grove::app::state::SetupSource::Settings {
                    state.settings.active = true;
                }
                state.pm_setup.source = grove::app::state::SetupSource::default();
            }
            grove::app::state::PmSetupStep::Workspace => {
                state.pm_setup.step = grove::app::state::PmSetupStep::Token;
            }
            grove::app::state::PmSetupStep::Project => {
                let provider = state.settings.repo_config.project_mgmt.provider;
                match provider {
                    grove::app::config::ProjectMgmtProvider::Notion => {
                        state.pm_setup.step = grove::app::state::PmSetupStep::Workspace;
                        state.pm_setup.teams = grove::core::projects::notion::extract_parent_pages(
                            &state.pm_setup.all_databases,
                        );
                        state.pm_setup.selected_team_index = 0;
                    }
                    grove::app::config::ProjectMgmtProvider::Asana => {
                        state.pm_setup.step = grove::app::state::PmSetupStep::Workspace;
                        state.pm_setup.teams.clear();
                        state.pm_setup.selected_team_index = 0;
                        if grove::app::Config::asana_token().is_some() {
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let asana_client = Arc::clone(asana_client);
                            tokio::spawn(async move {
                                match asana_client.fetch_workspaces().await {
                                    Ok(workspaces) => {
                                        let _ = tx
                                            .send(Action::PmSetupTeamsLoaded { teams: workspaces });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                    }
                    grove::app::config::ProjectMgmtProvider::Clickup => {
                        state.pm_setup.step = grove::app::state::PmSetupStep::Workspace;
                        state.pm_setup.teams.clear();
                        state.pm_setup.selected_team_index = 0;
                        if grove::app::Config::clickup_token().is_some() {
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let token = grove::app::Config::clickup_token().unwrap();
                            tokio::spawn(async move {
                                match grove::core::projects::clickup::fetch_teams(&token).await {
                                    Ok(teams) => {
                                        let _ = tx.send(Action::PmSetupTeamsLoaded { teams });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                    }
                    grove::app::config::ProjectMgmtProvider::Airtable => {
                        state.pm_setup.step = grove::app::state::PmSetupStep::Workspace;
                        state.pm_setup.teams.clear();
                        state.pm_setup.selected_team_index = 0;
                        if grove::app::Config::airtable_token().is_some() {
                            state.pm_setup.teams_loading = true;
                            let tx = action_tx.clone();
                            let token = grove::app::Config::airtable_token().unwrap();
                            tokio::spawn(async move {
                                match grove::core::projects::airtable::fetch_bases(&token).await {
                                    Ok(bases) => {
                                        let _ =
                                            tx.send(Action::PmSetupTeamsLoaded { teams: bases });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::PmSetupTeamsError {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                    }
                    _ => {
                        state.pm_setup.step = grove::app::state::PmSetupStep::Workspace;
                    }
                }
            }
            grove::app::state::PmSetupStep::Advanced => {
                let provider = state.settings.repo_config.project_mgmt.provider;
                match provider {
                    grove::app::config::ProjectMgmtProvider::Asana
                    | grove::app::config::ProjectMgmtProvider::Clickup
                    | grove::app::config::ProjectMgmtProvider::Airtable
                    | grove::app::config::ProjectMgmtProvider::Notion => {
                        state.pm_setup.step = grove::app::state::PmSetupStep::Project;
                    }
                    _ => {
                        state.pm_setup.step = grove::app::state::PmSetupStep::Workspace;
                    }
                }
            }
        },
        Action::PmSetupToggleAdvanced => {
            state.pm_setup.advanced_expanded = !state.pm_setup.advanced_expanded;
        }
        Action::PmSetupNavigateNext => {
            let max_fields = if state.pm_setup.advanced_expanded {
                3
            } else {
                0
            };
            if state.pm_setup.field_index < max_fields {
                state.pm_setup.field_index += 1;
            }
        }
        Action::PmSetupNavigatePrev => {
            if state.pm_setup.field_index > 0 {
                state.pm_setup.field_index -= 1;
            }
        }
        Action::PmSetupToggleDropdown => {
            if !state.pm_setup.teams.is_empty() && state.pm_setup.field_index == 0 {
                state.pm_setup.dropdown_open = !state.pm_setup.dropdown_open;
                state.pm_setup.dropdown_index = state.pm_setup.selected_team_index;
            }
        }
        Action::PmSetupDropdownNext => {
            if state.pm_setup.dropdown_index < state.pm_setup.teams.len().saturating_sub(1) {
                state.pm_setup.dropdown_index += 1;
            }
        }
        Action::PmSetupDropdownPrev => {
            if state.pm_setup.dropdown_index > 0 {
                state.pm_setup.dropdown_index -= 1;
            }
        }
        Action::PmSetupConfirmDropdown => {
            if state.pm_setup.dropdown_index < state.pm_setup.teams.len() {
                state.pm_setup.selected_team_index = state.pm_setup.dropdown_index;
            }
            state.pm_setup.dropdown_open = false;
        }
        Action::PmSetupInputChar(c) => {
            if state.pm_setup.field_index == 1 {
                state.pm_setup.manual_team_id.push(c);
            } else if state.pm_setup.field_index == 2 {
                state.pm_setup.in_progress_state.push(c);
            } else if state.pm_setup.field_index == 3 {
                state.pm_setup.done_state.push(c);
            }
        }
        Action::PmSetupBackspace => {
            if state.pm_setup.field_index == 1 {
                state.pm_setup.manual_team_id.pop();
            } else if state.pm_setup.field_index == 2 {
                state.pm_setup.in_progress_state.pop();
            } else if state.pm_setup.field_index == 3 {
                state.pm_setup.done_state.pop();
            }
        }
        Action::PmSetupTeamsLoaded { teams } => {
            state.pm_setup.teams = teams;
            state.pm_setup.teams_loading = false;
            state.pm_setup.selected_team_index = 0;
        }
        Action::PmSetupNotionDatabasesLoaded {
            databases,
            parent_pages,
        } => {
            state.pm_setup.all_databases = databases;
            state.pm_setup.teams = parent_pages;
            state.pm_setup.teams_loading = false;
            state.pm_setup.selected_team_index = 0;
        }
        Action::PmSetupTeamsError { message } => {
            state.pm_setup.teams_loading = false;
            state.pm_setup.error = Some(message);
        }
        Action::LinearUserFetched { username } => {
            state.settings.repo_config.project_mgmt.linear.username = Some(username.clone());
            if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                state.log_error(format!("Failed to save Linear username: {}", e));
            } else {
                state.log_info(format!("Linear username saved: {}", username));
            }
        }
        Action::LinearUserFetchError { message } => {
            state.log_error(format!("Failed to fetch Linear username: {}", message));
        }
        Action::PmSetupComplete => {
            let provider = state.settings.repo_config.project_mgmt.provider;
            let manual_id = state.pm_setup.manual_team_id.clone();
            let in_progress_state = state.pm_setup.in_progress_state.clone();
            let done_state = state.pm_setup.done_state.clone();

            let selected_id = if !manual_id.is_empty() {
                Some(manual_id)
            } else {
                state
                    .pm_setup
                    .teams
                    .get(state.pm_setup.selected_team_index)
                    .map(|t| t.0.clone())
            };

            let selected_name = state
                .pm_setup
                .teams
                .get(state.pm_setup.selected_team_index)
                .map(|t| t.1.clone())
                .unwrap_or_else(|| "manual".to_string());

            if let Some(id) = selected_id {
                match provider {
                    grove::app::config::ProjectMgmtProvider::Linear => {
                        state.settings.repo_config.project_mgmt.linear.team_id = Some(id.clone());
                        if !in_progress_state.is_empty() {
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .linear
                                .in_progress_state = Some(in_progress_state);
                        }
                        if !done_state.is_empty() {
                            state.settings.repo_config.project_mgmt.linear.done_state =
                                Some(done_state);
                        }
                        if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                            state.log_error(format!("Failed to save project config: {}", e));
                        } else {
                            state.log_info(format!(
                                "Linear setup complete: team '{}'",
                                selected_name
                            ));
                            linear_client.reconfigure(
                                grove::app::Config::linear_token().as_deref(),
                                Some(id.clone()),
                            );
                            let linear_client_clone = linear_client.clone();
                            let action_tx_clone = action_tx.clone();
                            tokio::spawn(async move {
                                match linear_client_clone.get_viewer().await {
                                    Ok(username) => {
                                        let _ = action_tx_clone
                                            .send(Action::LinearUserFetched { username });
                                    }
                                    Err(e) => {
                                        let _ =
                                            action_tx_clone.send(Action::LinearUserFetchError {
                                                message: e.to_string(),
                                            });
                                    }
                                }
                            });
                        }
                    }
                    grove::app::config::ProjectMgmtProvider::Notion => {
                        state.settings.repo_config.project_mgmt.notion.database_id =
                            Some(id.clone());
                        if !in_progress_state.is_empty() {
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .notion
                                .in_progress_option = Some(in_progress_state);
                        }
                        if !done_state.is_empty() {
                            state.settings.repo_config.project_mgmt.notion.done_option =
                                Some(done_state);
                        }
                        if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                            state.log_error(format!("Failed to save project config: {}", e));
                        } else {
                            state.log_info(format!(
                                "Notion setup complete: database '{}'",
                                selected_name
                            ));
                            notion_client.reconfigure(
                                grove::app::Config::notion_token().as_deref(),
                                Some(id),
                                state
                                    .settings
                                    .repo_config
                                    .project_mgmt
                                    .notion
                                    .status_property_name
                                    .clone(),
                            );
                        }
                    }
                    grove::app::config::ProjectMgmtProvider::Asana => {
                        state.settings.repo_config.project_mgmt.asana.project_gid =
                            Some(id.clone());
                        if !in_progress_state.is_empty() {
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .asana
                                .in_progress_section_gid = Some(in_progress_state);
                        }
                        if !done_state.is_empty() {
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .asana
                                .done_section_gid = Some(done_state);
                        }
                        if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                            state.log_error(format!("Failed to save project config: {}", e));
                        } else {
                            state.log_info(format!(
                                "Asana setup complete: project '{}'",
                                selected_name
                            ));
                            asana_client.reconfigure(
                                grove::app::Config::asana_token().as_deref(),
                                Some(id),
                            );
                        }
                    }
                    grove::app::config::ProjectMgmtProvider::Clickup => {
                        state.settings.repo_config.project_mgmt.clickup.list_id = Some(id.clone());
                        if !in_progress_state.is_empty() {
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .clickup
                                .in_progress_status = Some(in_progress_state);
                        }
                        if !done_state.is_empty() {
                            state.settings.repo_config.project_mgmt.clickup.done_status =
                                Some(done_state);
                        }
                        if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                            state.log_error(format!("Failed to save project config: {}", e));
                        } else {
                            state.log_info(format!(
                                "ClickUp setup complete: list '{}'",
                                selected_name
                            ));
                            clickup_client.reconfigure(
                                grove::app::Config::clickup_token().as_deref(),
                                Some(id),
                            );
                        }
                    }
                    grove::app::config::ProjectMgmtProvider::Airtable => {
                        let base_id = state.pm_setup.selected_workspace_gid.clone();
                        state.settings.repo_config.project_mgmt.airtable.base_id = base_id.clone();
                        state.settings.repo_config.project_mgmt.airtable.table_name =
                            Some(selected_name.clone());
                        if !in_progress_state.is_empty() {
                            state
                                .settings
                                .repo_config
                                .project_mgmt
                                .airtable
                                .in_progress_option = Some(in_progress_state);
                        }
                        if !done_state.is_empty() {
                            state.settings.repo_config.project_mgmt.airtable.done_option =
                                Some(done_state);
                        }
                        if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                            state.log_error(format!("Failed to save project config: {}", e));
                        } else {
                            state.log_info(format!(
                                "Airtable setup complete: table '{}'",
                                selected_name
                            ));
                            airtable_client.reconfigure(
                                grove::app::Config::airtable_token().as_deref(),
                                base_id,
                                Some(selected_name),
                                state
                                    .settings
                                    .repo_config
                                    .project_mgmt
                                    .airtable
                                    .status_field_name
                                    .clone(),
                            );
                        }
                    }
                }
            }
            if let Some(wizard) = &mut state.project_setup {
                wizard.config = state.settings.repo_config.clone();
            }
            state.pm_setup.active = false;
            if state.pm_setup.source == grove::app::state::SetupSource::Settings {
                state.settings.active = true;
            }
            state.pm_setup.source = grove::app::state::SetupSource::default();
        }

        // Git Setup Wizard Actions
        Action::OpenGitSetup => {
            state.settings.active = false;
            let provider = state.settings.repo_config.git.provider;
            state.git_setup.active = true;
            state.git_setup.source = grove::app::state::SetupSource::Settings;
            state.git_setup.step = grove::app::state::GitSetupStep::Token;
            state.git_setup.error = None;
            state.git_setup.field_index = 0;
            state.git_setup.advanced_expanded = false;
            state.git_setup.editing_text = false;
            state.git_setup.dropdown_open = false;
            state.git_setup.dropdown_index = 0;
            state.git_setup.text_buffer.clear();
            state.git_setup.loading = false;
            state.git_setup.project_id.clear();
            state.git_setup.owner.clear();
            state.git_setup.repo.clear();
            state.git_setup.base_url.clear();
            state.git_setup.detected_from_remote = false;
            state.git_setup.project_name = None;
            state.git_setup.ci_provider = grove::app::config::CodebergCiProvider::default();
            state.git_setup.woodpecker_repo_id.clear();

            // Try to auto-detect from git remote (always extract owner/repo)
            if let Some(remote_info) = grove::git::parse_remote_info(&state.repo_path) {
                state.git_setup.owner = remote_info.owner.clone();
                state.git_setup.repo = remote_info.repo.clone();
                state.git_setup.detected_from_remote = remote_info.provider == provider;
                if remote_info.provider == provider {
                    if let Some(url) = remote_info.base_url {
                        state.git_setup.base_url = url;
                    }
                }
            }

            // Pre-fill from existing config
            match provider {
                grove::app::config::GitProvider::GitLab => {
                    if let Some(id) = state.settings.repo_config.git.gitlab.project_id {
                        state.git_setup.project_id = id.to_string();
                    }
                    if state.settings.repo_config.git.gitlab.base_url != "https://gitlab.com" {
                        state.git_setup.base_url =
                            state.settings.repo_config.git.gitlab.base_url.clone();
                    }
                }
                grove::app::config::GitProvider::GitHub => {
                    if let Some(ref owner) = state.settings.repo_config.git.github.owner {
                        state.git_setup.owner = owner.clone();
                    }
                    if let Some(ref repo) = state.settings.repo_config.git.github.repo {
                        state.git_setup.repo = repo.clone();
                    }
                }
                grove::app::config::GitProvider::Codeberg => {
                    if let Some(ref owner) = state.settings.repo_config.git.codeberg.owner {
                        state.git_setup.owner = owner.clone();
                    }
                    if let Some(ref repo) = state.settings.repo_config.git.codeberg.repo {
                        state.git_setup.repo = repo.clone();
                    }
                    if state.settings.repo_config.git.codeberg.base_url != "https://codeberg.org" {
                        state.git_setup.base_url =
                            state.settings.repo_config.git.codeberg.base_url.clone();
                    }
                    state.git_setup.ci_provider =
                        state.settings.repo_config.git.codeberg.ci_provider;
                    if let Some(wp_id) = state.settings.repo_config.git.codeberg.woodpecker_repo_id
                    {
                        state.git_setup.woodpecker_repo_id = wp_id.to_string();
                    }
                }
            }
        }
        Action::CloseGitSetup => {
            state.git_setup.active = false;
            state.settings.active = true;
        }
        Action::GitSetupNextStep => match state.git_setup.step {
            grove::app::state::GitSetupStep::Token => {
                state.git_setup.step = grove::app::state::GitSetupStep::Repository;

                // Auto-fetch GitLab project ID if conditions are met
                let provider = state.settings.repo_config.git.provider;
                if matches!(provider, grove::app::config::GitProvider::GitLab)
                    && state.git_setup.project_id.is_empty()
                    && !state.git_setup.owner.is_empty()
                    && !state.git_setup.repo.is_empty()
                    && grove::app::Config::gitlab_token().is_some()
                {
                    state.git_setup.loading = true;
                    let owner = state.git_setup.owner.clone();
                    let repo = state.git_setup.repo.clone();
                    let base_url = if state.git_setup.base_url.is_empty() {
                        "https://gitlab.com".to_string()
                    } else {
                        state.git_setup.base_url.clone()
                    };
                    let token = grove::app::Config::gitlab_token().unwrap();
                    let tx = action_tx.clone();

                    tokio::spawn(async move {
                        let path = format!("{}/{}", owner, repo);
                        match grove::core::git_providers::gitlab::fetch_project_by_path(
                            &base_url, &path, &token,
                        )
                        .await
                        {
                            Ok((id, name)) => {
                                let _ = tx.send(Action::GitSetupProjectIdFetched { id, name });
                            }
                            Err(e) => {
                                let _ = tx.send(Action::GitSetupProjectIdError {
                                    message: e.to_string(),
                                });
                            }
                        }
                    });
                }
            }
            grove::app::state::GitSetupStep::Repository => {
                // Skip Advanced step - just complete setup
            }
            grove::app::state::GitSetupStep::Advanced => {}
        },
        Action::GitSetupPrevStep => match state.git_setup.step {
            grove::app::state::GitSetupStep::Token => {
                state.git_setup.active = false;
                if state.git_setup.source == grove::app::state::SetupSource::Settings {
                    state.settings.active = true;
                }
                state.git_setup.source = grove::app::state::SetupSource::default();
            }
            grove::app::state::GitSetupStep::Repository => {
                state.git_setup.step = grove::app::state::GitSetupStep::Token;
            }
            grove::app::state::GitSetupStep::Advanced => {
                state.git_setup.step = grove::app::state::GitSetupStep::Repository;
            }
        },
        Action::GitSetupToggleAdvanced => {
            state.git_setup.advanced_expanded = !state.git_setup.advanced_expanded;
        }
        Action::GitSetupNavigateNext => {
            let provider = state.settings.repo_config.git.provider;
            let max_field = if state.git_setup.advanced_expanded {
                match provider {
                    grove::app::config::GitProvider::GitLab => 3,
                    grove::app::config::GitProvider::GitHub => 2,
                    grove::app::config::GitProvider::Codeberg => 3,
                }
            } else {
                match provider {
                    grove::app::config::GitProvider::GitLab => 2,
                    grove::app::config::GitProvider::GitHub => 1,
                    grove::app::config::GitProvider::Codeberg => 3,
                }
            };
            if state.git_setup.field_index < max_field {
                state.git_setup.field_index += 1;
            }
        }
        Action::GitSetupNavigatePrev => {
            if state.git_setup.field_index > 0 {
                state.git_setup.field_index -= 1;
            }
        }
        Action::GitSetupStartEdit => {
            state.git_setup.editing_text = true;
            state.git_setup.text_buffer = match state.git_setup.field_index {
                0 => state.git_setup.owner.clone(),
                1 => state.git_setup.repo.clone(),
                2 => state.git_setup.project_id.clone(),
                _ => state.git_setup.base_url.clone(),
            };
        }
        Action::GitSetupCancelEdit => {
            state.git_setup.editing_text = false;
            state.git_setup.text_buffer.clear();
        }
        Action::GitSetupConfirmEdit => {
            match state.git_setup.field_index {
                0 => state.git_setup.owner = state.git_setup.text_buffer.clone(),
                1 => state.git_setup.repo = state.git_setup.text_buffer.clone(),
                2 => state.git_setup.project_id = state.git_setup.text_buffer.clone(),
                _ => state.git_setup.base_url = state.git_setup.text_buffer.clone(),
            }
            state.git_setup.editing_text = false;
            state.git_setup.text_buffer.clear();
        }
        Action::GitSetupInputChar(c) => {
            state.git_setup.text_buffer.push(c);
        }
        Action::GitSetupBackspace => {
            state.git_setup.text_buffer.pop();
        }
        Action::GitSetupComplete => {
            let provider = state.settings.repo_config.git.provider;

            match provider {
                grove::app::config::GitProvider::GitLab => {
                    if let Ok(id) = state.git_setup.project_id.parse::<u64>() {
                        state.settings.repo_config.git.gitlab.project_id = Some(id);
                    }
                    if !state.git_setup.base_url.is_empty() {
                        state.settings.repo_config.git.gitlab.base_url =
                            state.git_setup.base_url.clone();
                    }
                    if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                        state.log_error(format!("Failed to save project config: {}", e));
                    } else {
                        state.log_info(format!(
                            "GitLab setup complete: project {}",
                            state.git_setup.project_id
                        ));
                        gitlab_client.reconfigure(
                            &state.settings.repo_config.git.gitlab.base_url,
                            state.settings.repo_config.git.gitlab.project_id,
                            grove::app::Config::gitlab_token().as_deref(),
                        );
                    }
                }
                grove::app::config::GitProvider::GitHub => {
                    state.log_debug(format!(
                        "GitHub setup: owner='{}', repo='{}'",
                        state.git_setup.owner, state.git_setup.repo
                    ));
                    state.settings.repo_config.git.github.owner =
                        Some(state.git_setup.owner.clone());
                    state.settings.repo_config.git.github.repo = Some(state.git_setup.repo.clone());
                    if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                        state.log_error(format!("Failed to save project config: {}", e));
                    } else {
                        state.log_info(format!(
                            "GitHub setup complete: {}/{}",
                            state.git_setup.owner, state.git_setup.repo
                        ));
                        github_client.reconfigure(
                            Some(&state.git_setup.owner),
                            Some(&state.git_setup.repo),
                            grove::app::Config::github_token().as_deref(),
                        );
                    }
                }
                grove::app::config::GitProvider::Codeberg => {
                    state.log_debug(format!(
                        "Codeberg setup: owner='{}', repo='{}', base_url='{}', ci_provider={:?}, woodpecker_id='{}'",
                        state.git_setup.owner,
                        state.git_setup.repo,
                        state.git_setup.base_url,
                        state.git_setup.ci_provider,
                        state.git_setup.woodpecker_repo_id
                    ));
                    state.settings.repo_config.git.codeberg.owner =
                        Some(state.git_setup.owner.clone());
                    state.settings.repo_config.git.codeberg.repo =
                        Some(state.git_setup.repo.clone());
                    state.settings.repo_config.git.codeberg.ci_provider =
                        state.git_setup.ci_provider;
                    if !state.git_setup.woodpecker_repo_id.is_empty() {
                        state.settings.repo_config.git.codeberg.woodpecker_repo_id =
                            state.git_setup.woodpecker_repo_id.parse().ok();
                    }
                    if !state.git_setup.base_url.is_empty() {
                        state.settings.repo_config.git.codeberg.base_url =
                            state.git_setup.base_url.clone();
                    }
                    if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                        state.log_error(format!("Failed to save project config: {}", e));
                    } else {
                        state.log_info(format!(
                            "Codeberg setup complete: {}/{} (CI: {:?})",
                            state.git_setup.owner,
                            state.git_setup.repo,
                            state.git_setup.ci_provider
                        ));
                        codeberg_client.reconfigure(
                            Some(&state.git_setup.owner),
                            Some(&state.git_setup.repo),
                            Some(&state.settings.repo_config.git.codeberg.base_url),
                            grove::app::Config::codeberg_token().as_deref(),
                            state.git_setup.ci_provider,
                            grove::app::Config::woodpecker_token().as_deref(),
                            state.settings.repo_config.git.codeberg.woodpecker_repo_id,
                        );
                    }
                }
            }

            if let Some(wizard) = &mut state.project_setup {
                wizard.config = state.settings.repo_config.clone();
            }

            state.git_setup.active = false;
            if state.git_setup.source == grove::app::state::SetupSource::Settings {
                state.settings.active = true;
            }
            state.git_setup.source = grove::app::state::SetupSource::default();
        }
        Action::GitSetupFetchProjectId => {
            if grove::app::Config::gitlab_token().is_some() {
                state.git_setup.loading = true;
                state.git_setup.error = None;

                let owner = state.git_setup.owner.clone();
                let repo = state.git_setup.repo.clone();
                let base_url = if state.git_setup.base_url.is_empty() {
                    "https://gitlab.com".to_string()
                } else {
                    state.git_setup.base_url.clone()
                };
                let token = grove::app::Config::gitlab_token().unwrap();
                let tx = action_tx.clone();

                tokio::spawn(async move {
                    let path = format!("{}/{}", owner, repo);
                    match grove::core::git_providers::gitlab::fetch_project_by_path(
                        &base_url, &path, &token,
                    )
                    .await
                    {
                        Ok((id, name)) => {
                            let _ = tx.send(Action::GitSetupProjectIdFetched { id, name });
                        }
                        Err(e) => {
                            let _ = tx.send(Action::GitSetupProjectIdError {
                                message: e.to_string(),
                            });
                        }
                    }
                });
            }
        }
        Action::GitSetupProjectIdFetched { id, name } => {
            state.git_setup.loading = false;
            state.git_setup.project_id = id.to_string();
            state.git_setup.project_name = Some(name);
        }
        Action::GitSetupProjectIdError { message } => {
            state.git_setup.loading = false;
            state.git_setup.error = Some(message);
        }
        Action::GitSetupToggleDropdown => {
            if state.git_setup.dropdown_open {
                // Confirm the selection
                state.git_setup.dropdown_open = false;
                if state.git_setup.dropdown_index == 0 {
                    state.git_setup.ci_provider =
                        grove::app::config::CodebergCiProvider::ForgejoActions;
                } else {
                    state.git_setup.ci_provider =
                        grove::app::config::CodebergCiProvider::Woodpecker;
                }
            } else {
                // Open the dropdown
                state.git_setup.dropdown_open = true;
                state.git_setup.dropdown_index = match state.git_setup.ci_provider {
                    grove::app::config::CodebergCiProvider::ForgejoActions => 0,
                    grove::app::config::CodebergCiProvider::Woodpecker => 1,
                };
            }
        }
        Action::GitSetupDropdownNext => {
            state.git_setup.dropdown_index = (state.git_setup.dropdown_index + 1) % 2;
        }
        Action::GitSetupDropdownPrev => {
            state.git_setup.dropdown_index = if state.git_setup.dropdown_index == 0 {
                1
            } else {
                0
            };
        }
        Action::GitSetupConfirmDropdown => {
            state.git_setup.dropdown_open = false;
            if state.git_setup.dropdown_index == 0 {
                state.git_setup.ci_provider =
                    grove::app::config::CodebergCiProvider::ForgejoActions;
            } else {
                state.git_setup.ci_provider = grove::app::config::CodebergCiProvider::Woodpecker;
            }
        }
        Action::GitSetupCloseDropdown => {
            state.git_setup.dropdown_open = false;
        }

        // Dev Server Actions
        Action::RequestStartDevServer => {
            if let Some(agent) = state.selected_agent() {
                let agent_id = agent.id;
                if let Ok(manager) = devserver_manager.try_lock() {
                    let current_running = manager
                        .get(agent_id)
                        .map(|s| s.status().is_running())
                        .unwrap_or(false);

                    if current_running {
                        drop(manager);
                        action_tx.send(Action::StopDevServer)?;
                    } else if manager.has_running_server() {
                        let running = manager.running_servers();
                        state.devserver_warning = Some(grove::app::DevServerWarning {
                            agent_id,
                            running_servers: running
                                .into_iter()
                                .map(|(_, name, port)| (name, port))
                                .collect(),
                        });
                    } else {
                        drop(manager);
                        action_tx.send(Action::StartDevServer)?;
                    }
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
                let worktree = std::path::PathBuf::from(agent.worktree_path.clone());
                let agent_id = agent.id;
                let agent_name = agent.name.clone();
                let manager = Arc::clone(devserver_manager);

                state.log_info(format!("Starting dev server for '{}'", agent.name));

                tokio::spawn(async move {
                    let mut m = manager.lock().await;
                    if let Err(e) = m.start(agent_id, agent_name, &config, &worktree).await {
                        tracing::error!("Failed to start dev server: {}", e);
                    }
                });
            }
        }

        Action::StopDevServer => {
            if let Some(agent) = state.selected_agent() {
                let agent_id = agent.id;
                let manager = Arc::clone(devserver_manager);
                let name = agent.name.clone();

                state.log_info(format!("Stopping dev server for '{}'", name));

                tokio::spawn(async move {
                    let mut m = manager.lock().await;
                    let _ = m.stop(agent_id).await;
                });
            }
        }

        Action::RestartDevServer => {
            if let Some(agent) = state.selected_agent() {
                let config = state.settings.repo_config.dev_server.clone();
                let worktree = std::path::PathBuf::from(agent.worktree_path.clone());
                let agent_id = agent.id;
                let agent_name = agent.name.clone();
                let manager = Arc::clone(devserver_manager);

                state.log_info(format!("Restarting dev server for '{}'", agent.name));

                tokio::spawn(async move {
                    let mut m = manager.lock().await;
                    let _ = m.stop(agent_id).await;
                    if let Err(e) = m.start(agent_id, agent_name, &config, &worktree).await {
                        tracing::error!("Failed to restart dev server: {}", e);
                    }
                });
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
                let agent_id = agent.id;
                let mut manager = devserver_manager.lock().await;
                if let Some(server) = manager.get_mut(agent_id) {
                    server.clear_logs();
                }
            }
        }

        Action::OpenDevServerInBrowser => {
            if let Some(agent) = state.selected_agent() {
                let agent_id = agent.id;
                let manager = devserver_manager.lock().await;
                if let Some(server) = manager.get(agent_id) {
                    if let Some(port) = server.status().port() {
                        let url = format!("http://localhost:{}", port);
                        match open::that(&url) {
                            Ok(_) => state.log_info("Opening dev server in browser"),
                            Err(e) => state.log_error(format!("Failed to open browser: {}", e)),
                        }
                    }
                }
            }
        }

        Action::AppendDevServerLog { agent_id, line } => {
            let mut manager = devserver_manager.lock().await;
            if let Some(server) = manager.get_mut(agent_id) {
                server.append_log(line);
            }
        }

        Action::UpdateDevServerStatus { agent_id, status } => {
            state.log_debug(format!(
                "Dev server {} status: {}",
                agent_id,
                status.label()
            ));
        }
    }

    Ok(false)
}

/// Background task to poll agent status from tmux sessions.
async fn poll_agents(
    mut agent_rx: watch::Receiver<HashSet<Uuid>>,
    mut selected_rx: watch::Receiver<Option<Uuid>>,
    tx: mpsc::UnboundedSender<Action>,
    ai_agent: grove::app::config::AiAgent,
    debug_mode: bool,
) {
    use std::collections::HashMap;

    // Track previous content hash for activity detection
    let mut previous_content: HashMap<Uuid, u64> = HashMap::new();
    // Track which agents already have MR URLs detected (skip deep scans for them)
    let mut agents_with_mr: HashSet<Uuid> = HashSet::new();
    // Counter for periodic deep MR URL scan (~every 5s = 20 ticks at 250ms)
    let mut deep_scan_counter: u32 = 0;
    // Track previous selected_id to log changes
    let mut prev_selected_id: Option<Uuid> = None;

    loop {
        deep_scan_counter += 1;

        // Poll every 250ms for responsive status updates
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Get current agent list and selected agent
        let agent_ids = agent_rx.borrow_and_update().clone();
        let selected_id = *selected_rx.borrow_and_update();

        // Log when selected_id changes
        if selected_id != prev_selected_id {
            tracing::debug!("poll_agents: selected_id changed to {:?}", selected_id);
            prev_selected_id = selected_id;
        }

        for id in agent_ids {
            let is_selected = selected_id == Some(id);
            let session_name = format!("grove-{}", id.as_simple());

            // PRIORITY 1: Capture preview for selected agent FIRST
            // This ensures preview updates even if status detection crashes
            if is_selected {
                match std::process::Command::new("tmux")
                    .args([
                        "capture-pane",
                        "-t",
                        &session_name,
                        "-p",
                        "-e",
                        "-J",
                        "-S",
                        "-1000",
                    ])
                    .output()
                {
                    Ok(output) => {
                        if output.status.success() {
                            let preview = String::from_utf8_lossy(&output.stdout).to_string();
                            if let Err(e) = tx.send(Action::UpdatePreviewContent(Some(preview))) {
                                tracing::error!(
                                    "poll_agents: FAILED to send UpdatePreviewContent: {}",
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("poll_agents: tmux preview command FAILED: {}", e);
                    }
                }
            }

            // PRIORITY 2: Status detection (can be slow, may crash)
            // Always do a plain capture (no ANSI, consistent line count) for status detection
            // -J joins wrapped lines so URLs and long text aren't split across lines
            let capture_result = std::process::Command::new("tmux")
                .args([
                    "capture-pane",
                    "-t",
                    &session_name,
                    "-p",
                    "-J",
                    "-S",
                    "-100",
                ])
                .output();

            if let Ok(output) = capture_result {
                if output.status.success() {
                    let content = String::from_utf8_lossy(&output.stdout).to_string();

                    // Track activity by comparing content hash
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    content.hash(&mut hasher);
                    let content_hash = hasher.finish();

                    let had_activity = previous_content
                        .get(&id)
                        .map(|&prev| prev != content_hash)
                        .unwrap_or(false);

                    previous_content.insert(id, content_hash);
                    let _ = tx.send(Action::RecordActivity { id, had_activity });

                    // Query foreground process for ground-truth status detection
                    let foreground = {
                        let cmd_output = std::process::Command::new("tmux")
                            .args([
                                "display-message",
                                "-t",
                                &session_name,
                                "-p",
                                "#{pane_current_command}",
                            ])
                            .output();
                        match cmd_output {
                            Ok(o) if o.status.success() => {
                                let cmd = String::from_utf8_lossy(&o.stdout).trim().to_string();
                                ForegroundProcess::from_command_for_agent(&cmd, ai_agent.clone())
                            }
                            _ => ForegroundProcess::Unknown,
                        }
                    };
                    let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        detect_status_for_agent(&content, foreground, ai_agent.clone())
                    }))
                    .unwrap_or_else(|e| {
                        tracing::warn!("detect_status_for_agent panicked: {:?}", e);
                        StatusDetection::new(AgentStatus::Idle)
                    });

                    let status_reason = if debug_mode {
                        status.to_status_reason()
                    } else {
                        None
                    };

                    let _ = tx.send(Action::UpdateAgentStatus {
                        id,
                        status: status.status,
                        status_reason,
                    });

                    // Check for MR URLs detection
                    if !agents_with_mr.contains(&id) {
                        if let Some(mr_status) = detect_mr_url(&content) {
                            agents_with_mr.insert(id);
                            let _ = tx.send(Action::UpdateMrStatus {
                                id,
                                status: mr_status,
                            });
                        }
                    }

                    // Check for checklist progress (wrap in catch_unwind to prevent crashing the loop)
                    let progress = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        detect_checklist_progress(&content, ai_agent.clone())
                    }))
                    .unwrap_or_else(|e| {
                        tracing::warn!("detect_checklist_progress panicked, skipping: {:?}", e);
                        None
                    });
                    let _ = tx.send(Action::UpdateChecklistProgress { id, progress });
                }
            } else {
                tracing::warn!(
                    "poll_agents: capture-pane command FAILED for session {}",
                    session_name
                );
            }

            // Deep MR URL scan: capture 500 lines every ~5s for agents without MR detected
            if deep_scan_counter.is_multiple_of(20) && !agents_with_mr.contains(&id) {
                if let Ok(output) = std::process::Command::new("tmux")
                    .args([
                        "capture-pane",
                        "-t",
                        &session_name,
                        "-p",
                        "-J",
                        "-S",
                        "-500",
                    ])
                    .output()
                {
                    if output.status.success() {
                        let deep_content = String::from_utf8_lossy(&output.stdout).to_string();
                        if let Some(mr_status) = detect_mr_url(&deep_content) {
                            agents_with_mr.insert(id);
                            let _ = tx.send(Action::UpdateMrStatus {
                                id,
                                status: mr_status,
                            });
                        }
                    }
                }
            }
        }

        // Clear preview if no agents or no selection
        if selected_id.is_none() {
            let _ = tx.send(Action::UpdatePreviewContent(None));
        }
    }
}

/// Background task to poll global system metrics (CPU/memory).
async fn poll_system_metrics(tx: mpsc::UnboundedSender<Action>) {
    let mut sys = System::new_all();

    loop {
        // Poll every 1 second
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Refresh CPU and memory
        sys.refresh_cpu_usage();
        sys.refresh_memory();

        // Calculate global CPU usage (average across all CPUs)
        let cpu_percent = sys.global_cpu_usage();

        // Get memory usage
        let memory_used = sys.used_memory();
        let memory_total = sys.total_memory();

        // Send update
        let _ = tx.send(Action::UpdateGlobalSystemMetrics {
            cpu_percent,
            memory_used,
            memory_total,
        });
    }
}

/// Sort tasks so children appear directly after their parents.
fn sort_tasks_by_parent(tasks: &mut [TaskListItem]) {
    use std::collections::HashMap;

    let mut parent_to_children: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        if let Some(parent_id) = &task.parent_id {
            parent_to_children
                .entry(parent_id.clone())
                .or_default()
                .push(idx);
        }
    }

    let root_indices: Vec<usize> = tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.parent_id.is_none())
        .map(|(i, _)| i)
        .collect();

    let mut result = Vec::with_capacity(tasks.len());
    let mut processed = std::collections::HashSet::new();

    fn collect_tree(
        task_idx: usize,
        tasks: &[TaskListItem],
        parent_to_children: &HashMap<String, Vec<usize>>,
        processed: &mut std::collections::HashSet<usize>,
        result: &mut Vec<TaskListItem>,
    ) {
        if processed.contains(&task_idx) {
            return;
        }
        processed.insert(task_idx);
        result.push(tasks[task_idx].clone());

        let task_id = &tasks[task_idx].id;
        if let Some(children) = parent_to_children.get(task_id) {
            for &child_idx in children {
                collect_tree(child_idx, tasks, parent_to_children, processed, result);
            }
        }
    }

    for root_idx in root_indices {
        collect_tree(
            root_idx,
            tasks,
            &parent_to_children,
            &mut processed,
            &mut result,
        );
    }

    for (idx, task) in tasks.iter().enumerate() {
        if !processed.contains(&idx) {
            result.push(task.clone());
        }
    }

    if result.len() == tasks.len() {
        for (i, item) in result.into_iter().enumerate() {
            tasks[i] = item;
        }
    }
}

/// Parse an Asana task GID from a URL or bare GID.
/// Supports: `https://app.asana.com/0/{project}/{task}/f`, `https://app.asana.com/0/{project}/{task}`, or bare `{task_gid}`.
fn parse_asana_task_gid(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.contains("asana.com") {
        let parts: Vec<&str> = trimmed.trim_end_matches('/').split('/').collect();
        // New format: https://app.asana.com/1/{workspace}/project/{project}/task/{task_gid}
        for (i, part) in parts.iter().enumerate() {
            if *part == "task" && i + 1 < parts.len() {
                let candidate = parts[i + 1];
                if candidate.chars().all(|c| c.is_ascii_digit()) {
                    return candidate.to_string();
                }
            }
        }
        // Old format: https://app.asana.com/0/{project}/{task}[/f]
        for (i, part) in parts.iter().enumerate() {
            if *part == "0" && i + 2 < parts.len() {
                let candidate = parts[i + 2];
                if candidate != "f" && candidate.chars().all(|c| c.is_ascii_digit()) {
                    return candidate.to_string();
                }
            }
        }
    }
    // Bare GID (just digits)
    trimmed.to_string()
}

fn compute_visible_task_indices(
    tasks: &[TaskListItem],
    expanded_ids: &std::collections::HashSet<String>,
) -> Vec<usize> {
    use std::collections::{HashMap, HashSet};

    let child_to_parent: HashMap<&str, &str> = tasks
        .iter()
        .filter_map(|t| t.parent_id.as_ref().map(|p| (t.id.as_str(), p.as_str())))
        .collect();

    fn is_ancestor_expanded(
        task: &TaskListItem,
        child_to_parent: &HashMap<&str, &str>,
        expanded_ids: &HashSet<String>,
    ) -> bool {
        let mut current_id = task.id.as_str();
        loop {
            match child_to_parent.get(current_id) {
                None => return true,
                Some(&parent_id) => {
                    if !expanded_ids.contains(parent_id) {
                        return false;
                    }
                    current_id = parent_id;
                }
            }
        }
    }

    tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| {
            if task.parent_id.is_none() {
                true
            } else {
                is_ancestor_expanded(task, &child_to_parent, expanded_ids)
            }
        })
        .map(|(i, _)| i)
        .collect()
}

/// Background task to poll Asana for task status updates.
async fn poll_asana_tasks(
    asana_rx: watch::Receiver<Vec<(Uuid, String)>>,
    asana_client: Arc<OptionalAsanaClient>,
    tx: mpsc::UnboundedSender<Action>,
    refresh_secs: u64,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(refresh_secs)).await;

        let tasks = asana_rx.borrow().clone();
        for (id, gid) in tasks {
            match asana_client.get_task(&gid).await {
                Ok(task) => {
                    let url = task
                        .permalink_url
                        .unwrap_or_else(|| format!("https://app.asana.com/0/0/{}/f", task.gid));
                    let is_subtask = task.parent.is_some();
                    let status = if task.completed {
                        ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::Completed {
                            gid: task.gid,
                            name: task.name,
                            is_subtask,
                            status_name: "Complete".to_string(),
                        })
                    } else {
                        ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::InProgress {
                            gid: task.gid,
                            name: task.name,
                            url,
                            is_subtask,
                            status_name: "In Progress".to_string(),
                        })
                    };
                    let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                }
                Err(e) => {
                    tracing::warn!("Failed to poll Asana task {}: {}", gid, e);
                }
            }
        }
    }
}

/// Background task to poll Notion for task status updates.
async fn poll_notion_tasks(
    notion_rx: watch::Receiver<Vec<(Uuid, String)>>,
    notion_client: Arc<OptionalNotionClient>,
    tx: mpsc::UnboundedSender<Action>,
    refresh_secs: u64,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(refresh_secs)).await;

        let tasks = notion_rx.borrow().clone();
        for (id, page_id) in tasks {
            match notion_client.get_page(&page_id).await {
                Ok(page) => {
                    let status = ProjectMgmtTaskStatus::Notion(NotionTaskStatus::Linked {
                        page_id: page.id,
                        name: page.name,
                        url: page.url,
                        status_option_id: page.status_id.unwrap_or_default(),
                        status_name: page.status_name.unwrap_or_default(),
                    });
                    let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                }
                Err(e) => {
                    tracing::warn!("Failed to poll Notion page {}: {}", page_id, e);
                }
            }
        }
    }
}

/// Background task to poll ClickUp for task status updates.
async fn poll_clickup_tasks(
    clickup_rx: watch::Receiver<Vec<(Uuid, String)>>,
    clickup_client: Arc<OptionalClickUpClient>,
    tx: mpsc::UnboundedSender<Action>,
    refresh_secs: u64,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(refresh_secs)).await;

        let tasks = clickup_rx.borrow().clone();
        for (id, task_id) in tasks {
            match clickup_client.get_task(&task_id).await {
                Ok(task) => {
                    let url = task.url.clone().unwrap_or_default();
                    let is_subtask = task.parent.is_some();
                    let status = if task.status.status_type == "closed" {
                        ProjectMgmtTaskStatus::ClickUp(ClickUpTaskStatus::Completed {
                            id: task.id,
                            name: task.name,
                            is_subtask,
                        })
                    } else {
                        ProjectMgmtTaskStatus::ClickUp(ClickUpTaskStatus::InProgress {
                            id: task.id,
                            name: task.name,
                            url,
                            status: task.status.status,
                            is_subtask,
                        })
                    };
                    let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                }
                Err(e) => {
                    tracing::warn!("Failed to poll ClickUp task {}: {}", task_id, e);
                }
            }
        }
    }
}

/// Background task to poll Airtable for task status updates.
async fn poll_airtable_tasks(
    airtable_rx: watch::Receiver<Vec<(Uuid, String)>>,
    airtable_client: Arc<OptionalAirtableClient>,
    tx: mpsc::UnboundedSender<Action>,
    refresh_secs: u64,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(refresh_secs)).await;

        let tasks = airtable_rx.borrow().clone();
        for (id, record_id) in tasks {
            match airtable_client.get_record(&record_id).await {
                Ok(record) => {
                    let status_name = record.status.clone().unwrap_or_default();
                    let is_subtask = record.parent_id.is_some();
                    let is_completed = status_name.to_lowercase().contains("done")
                        || status_name.to_lowercase().contains("complete");
                    let status = if is_completed {
                        ProjectMgmtTaskStatus::Airtable(AirtableTaskStatus::Completed {
                            id: record.id,
                            name: record.name,
                            is_subtask,
                        })
                    } else if status_name.to_lowercase().contains("progress") {
                        ProjectMgmtTaskStatus::Airtable(AirtableTaskStatus::InProgress {
                            id: record.id,
                            name: record.name,
                            url: record.url,
                            is_subtask,
                        })
                    } else {
                        ProjectMgmtTaskStatus::Airtable(AirtableTaskStatus::NotStarted {
                            id: record.id,
                            name: record.name,
                            url: record.url,
                            is_subtask,
                        })
                    };
                    let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                }
                Err(e) => {
                    tracing::warn!("Failed to poll Airtable record {}: {}", record_id, e);
                }
            }
        }
    }
}

/// Background task to poll Linear for task status updates.
async fn poll_linear_tasks(
    linear_rx: watch::Receiver<Vec<(Uuid, String)>>,
    linear_client: Arc<OptionalLinearClient>,
    tx: mpsc::UnboundedSender<Action>,
    refresh_secs: u64,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(refresh_secs)).await;

        let tasks = linear_rx.borrow().clone();
        for (id, issue_id) in tasks {
            match linear_client.get_issue(&issue_id).await {
                Ok(issue) => {
                    let is_subtask = issue.parent_id.is_some();
                    let status = match issue.state_type.as_str() {
                        "completed" | "cancelled" => {
                            ProjectMgmtTaskStatus::Linear(LinearTaskStatus::Completed {
                                id: issue.id,
                                identifier: issue.identifier,
                                name: issue.title,
                                status_name: issue.state_name,
                                is_subtask,
                            })
                        }
                        "started" => ProjectMgmtTaskStatus::Linear(LinearTaskStatus::InProgress {
                            id: issue.id,
                            identifier: issue.identifier,
                            name: issue.title,
                            status_name: issue.state_name,
                            url: issue.url,
                            is_subtask,
                        }),
                        _ => ProjectMgmtTaskStatus::Linear(LinearTaskStatus::NotStarted {
                            id: issue.id,
                            identifier: issue.identifier,
                            name: issue.title,
                            status_name: issue.state_name,
                            url: issue.url,
                            is_subtask,
                        }),
                    };
                    let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                }
                Err(e) => {
                    tracing::warn!("Failed to poll Linear issue {}: {}", issue_id, e);
                }
            }
        }
    }
}

/// Background task to poll GitLab for MR status.
async fn poll_gitlab_mrs(
    branch_rx: watch::Receiver<Vec<(Uuid, String)>>,
    gitlab_client: Arc<OptionalGitLabClient>,
    tx: mpsc::UnboundedSender<Action>,
    refresh_secs: u64,
) {
    let mut first_run = true;

    loop {
        if first_run {
            first_run = false;
            tokio::time::sleep(Duration::from_millis(500)).await;
        } else {
            tokio::time::sleep(Duration::from_secs(refresh_secs)).await;
        }

        let branches = branch_rx.borrow().clone();

        for (id, branch) in branches {
            let status = gitlab_client.get_mr_for_branch(&branch).await;
            if !matches!(
                status,
                grove::core::git_providers::gitlab::MergeRequestStatus::None
            ) {
                let _ = tx.send(Action::UpdateMrStatus { id, status });
            }
        }
    }
}

/// Background task to poll GitHub for PR status.
async fn poll_github_prs(
    branch_rx: watch::Receiver<Vec<(Uuid, String)>>,
    github_client: Arc<OptionalGitHubClient>,
    tx: mpsc::UnboundedSender<Action>,
    refresh_secs: u64,
) {
    let mut first_run = true;

    loop {
        if first_run {
            first_run = false;
            tokio::time::sleep(Duration::from_millis(500)).await;
        } else {
            tokio::time::sleep(Duration::from_secs(refresh_secs)).await;
        }

        let branches = branch_rx.borrow().clone();
        tracing::info!("GitHub poll: checking {} branches", branches.len());

        for (id, branch) in branches {
            tracing::info!("GitHub poll: checking branch {}", branch);
            let status = github_client.get_pr_for_branch(&branch).await;
            tracing::info!("GitHub poll: branch {} -> {:?}", branch, status);
            if !matches!(
                status,
                grove::core::git_providers::github::PullRequestStatus::None
            ) {
                let _ = tx.send(Action::UpdatePrStatus { id, status });
            }
        }
    }
}

/// Background task to poll Codeberg for PR status.
async fn poll_codeberg_prs(
    branch_rx: watch::Receiver<Vec<(Uuid, String)>>,
    codeberg_client: Arc<OptionalCodebergClient>,
    tx: mpsc::UnboundedSender<Action>,
    refresh_secs: u64,
) {
    let mut first_run = true;

    loop {
        if first_run {
            first_run = false;
            tokio::time::sleep(Duration::from_millis(500)).await;
        } else {
            tokio::time::sleep(Duration::from_secs(refresh_secs)).await;
        }

        let branches = branch_rx.borrow().clone();
        tracing::info!("Codeberg poll: checking {} branches", branches.len());

        for (id, branch) in branches {
            tracing::info!("Codeberg poll: checking branch {}", branch);
            let status = codeberg_client.get_pr_for_branch(&branch).await;
            tracing::info!("Codeberg poll: branch {} -> {:?}", branch, status);
            if !matches!(
                status,
                grove::core::git_providers::codeberg::PullRequestStatus::None
            ) {
                let _ = tx.send(Action::UpdateCodebergPrStatus { id, status });
            }
        }
    }
}
