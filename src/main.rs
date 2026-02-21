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

use flock::agent::{
    detect_checklist_progress, detect_mr_url, detect_status_for_agent, Agent, AgentManager,
    AgentStatus, ForegroundProcess, ProjectMgmtTaskStatus,
};
use flock::app::{
    Action, AppState, Config, InputMode, PreviewTab, ProjectMgmtProvider, StatusOption,
    TaskItemStatus, TaskListItem, TaskStatusDropdownState, Toast, ToastLevel,
};
use flock::asana::{AsanaTaskStatus, OptionalAsanaClient};
use flock::codeberg::OptionalCodebergClient;
use flock::devserver::DevServerManager;
use flock::git::GitSync;
use flock::github::OptionalGitHubClient;
use flock::gitlab::OptionalGitLabClient;
use flock::notion::{parse_notion_page_id, NotionTaskStatus, OptionalNotionClient};
use flock::storage::{save_session, SessionStorage};
use flock::tmux::is_tmux_available;
use flock::ui::{AppWidget, DevServerRenderInfo};

#[tokio::main]
async fn main() -> Result<()> {
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/flock-debug.log")
        .ok();

    if let Some(file) = log_file {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("flock=debug".parse().unwrap()),
            )
            .with_writer(std::sync::Arc::new(file))
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("flock=info".parse().unwrap()),
            )
            .with_writer(std::io::stderr)
            .init();
    }

    tracing::info!("=== Flock starting ===");

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
            "Not a git repository: {}. Please run flock from a git repository.",
            repo_path
        );
    }

    // Load configuration
    let config = Config::load().unwrap_or_default();

    // Check if this is first launch (no ~/.flock directory exists)
    let is_first_launch = !Config::exists();

    // Check if project config exists
    let repo_config_path = flock::app::RepoConfig::config_path(&repo_path).ok();
    let project_needs_setup = repo_config_path
        .as_ref()
        .map(|p| !p.exists())
        .unwrap_or(true);

    // Initialize storage
    let storage = SessionStorage::new(&repo_path)?;

    // Create app state
    let mut state = AppState::new(config.clone(), repo_path.clone());
    state.log_info(format!("Flock started in {}", repo_path));

    // Show global setup wizard if first launch
    if is_first_launch {
        state.show_global_setup = true;
        state.global_setup = Some(flock::app::GlobalSetupState::default());
        state.log_info("First launch - showing global setup wizard".to_string());
    } else if project_needs_setup {
        // Show project setup wizard if project not configured
        state.show_project_setup = true;
        state.project_setup = Some(flock::app::ProjectSetupState::default());
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
    tokio::spawn(async move {
        use futures::future::FutureExt;
        use std::panic::AssertUnwindSafe;

        let result = AssertUnwindSafe(async {
            poll_agents(agent_watch_rx, selected_rx_clone, agent_poll_tx, ai_agent).await
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
    if gitlab_client.is_configured() {
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
    if github_client.is_configured() {
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
    if codeberg_client.is_configured() {
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
    ));

    let notion_client = Arc::new(OptionalNotionClient::new(
        Config::notion_token().as_deref(),
        notion_database_id,
        notion_status_property,
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

    if asana_client.is_configured() && matches!(pm_provider, ProjectMgmtProvider::Asana) {
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

    if notion_client.is_configured() && matches!(pm_provider, ProjectMgmtProvider::Notion) {
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

    // Main event loop
    let poll_timeout = Duration::from_millis(50);
    let tick_interval = Duration::from_millis(100);
    let mut last_tick = std::time::Instant::now();
    let mut pending_attach: Option<Uuid> = None;
    let mut pending_devserver_attach: Option<Uuid> = None;

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
                let tmux_session = flock::tmux::TmuxSession::new(&session_name);
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
                pm_provider,
                &storage,
                &action_tx,
                &agent_watch_tx,
                &branch_watch_tx,
                &selected_watch_tx,
                &asana_watch_tx,
                &notion_watch_tx,
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

    // Handle global setup wizard
    if state.show_global_setup {
        if let Some(wizard) = &state.global_setup {
            return match key.code {
                KeyCode::Esc => {
                    if wizard.dropdown_open {
                        Some(Action::GlobalSetupToggleDropdown)
                    } else if matches!(wizard.step, flock::app::GlobalSetupStep::AgentSettings) {
                        Some(Action::GlobalSetupPrevStep)
                    } else {
                        None // Can't go back from first step
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if wizard.dropdown_open {
                        Some(Action::GlobalSetupDropdownPrev)
                    } else if matches!(wizard.step, flock::app::GlobalSetupStep::WorktreeLocation) {
                        Some(Action::GlobalSetupSelectPrev)
                    } else {
                        Some(Action::GlobalSetupNavigateUp)
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if wizard.dropdown_open {
                        Some(Action::GlobalSetupDropdownNext)
                    } else if matches!(wizard.step, flock::app::GlobalSetupStep::WorktreeLocation) {
                        Some(Action::GlobalSetupSelectNext)
                    } else {
                        Some(Action::GlobalSetupNavigateDown)
                    }
                }
                KeyCode::Enter => {
                    if wizard.dropdown_open {
                        Some(Action::GlobalSetupConfirmDropdown)
                    } else if matches!(wizard.step, flock::app::GlobalSetupStep::AgentSettings) {
                        Some(Action::GlobalSetupToggleDropdown)
                    } else {
                        Some(Action::GlobalSetupNextStep)
                    }
                }
                KeyCode::Char('c') => {
                    if matches!(wizard.step, flock::app::GlobalSetupStep::AgentSettings)
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
                KeyCode::Char(c) if wizard.editing_text => Some(Action::ProjectSetupInputChar(c)),
                KeyCode::Backspace if wizard.editing_text => Some(Action::ProjectSetupBackspace),
                KeyCode::Esc => {
                    if wizard.editing_text {
                        Some(Action::ProjectSetupCancelEdit)
                    } else if wizard.dropdown_open {
                        Some(Action::ProjectSetupToggleDropdown)
                    } else {
                        Some(Action::ProjectSetupSkip)
                    }
                }
                KeyCode::Up | KeyCode::Char('k') if !wizard.editing_text => {
                    if wizard.dropdown_open {
                        Some(Action::ProjectSetupDropdownPrev)
                    } else {
                        Some(Action::ProjectSetupNavigatePrev)
                    }
                }
                KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') if !wizard.editing_text => {
                    if wizard.dropdown_open {
                        Some(Action::ProjectSetupDropdownNext)
                    } else {
                        Some(Action::ProjectSetupNavigateNext)
                    }
                }
                KeyCode::Enter => {
                    if wizard.editing_text {
                        Some(Action::ProjectSetupConfirmEdit)
                    } else if wizard.dropdown_open {
                        Some(Action::ProjectSetupConfirmDropdown)
                    } else {
                        Some(Action::ProjectSetupEditField)
                    }
                }
                KeyCode::Char('c') if !wizard.editing_text && !wizard.dropdown_open => {
                    Some(Action::ProjectSetupComplete)
                }
                _ => None,
            };
        }
    }

    // Check if selected agent is paused
    let is_paused = state
        .selected_agent()
        .map(|a| matches!(a.status, flock::agent::AgentStatus::Paused))
        .unwrap_or(false);

    // Normal mode key handling
    match key.code {
        // Quit
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),

        // Navigation (always allowed)
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SelectPrevious),
        KeyCode::Char('g') => Some(Action::SelectFirst),
        KeyCode::Char('G') => Some(Action::SelectLast),

        // Resume (only when paused)
        KeyCode::Char('r') if is_paused => state
            .selected_agent_id()
            .map(|id| Action::ResumeAgent { id }),

        // Refresh selected agent status (only when not paused)
        KeyCode::Char('r') if !is_paused => {
            if state.selected_agent_id().is_some() {
                Some(Action::RefreshSelected)
            } else {
                None
            }
        }

        // Yank (copy) agent name to clipboard
        KeyCode::Char('y') => state
            .selected_agent_id()
            .map(|id| Action::CopyAgentName { id }),

        // Notes (always allowed)
        KeyCode::Char('N') => Some(Action::EnterInputMode(InputMode::SetNote)),

        // These actions work regardless of pause state
        KeyCode::Char('n') => Some(Action::EnterInputMode(InputMode::NewAgent)),
        KeyCode::Char('d') => {
            let has_task = state
                .selected_agent()
                .map(|a| a.pm_task_status.is_linked())
                .unwrap_or(false);
            if has_task {
                Some(Action::EnterInputMode(InputMode::ConfirmDeleteTask))
            } else {
                Some(Action::EnterInputMode(InputMode::ConfirmDelete))
            }
        }
        KeyCode::Enter => match state.preview_tab {
            PreviewTab::Preview => state
                .selected_agent_id()
                .map(|id| Action::AttachToAgent { id }),
            PreviewTab::DevServer => state
                .selected_agent_id()
                .map(|id| Action::AttachToDevServer { agent_id: id }),
        },

        // Pause/checkout (only when not paused)
        KeyCode::Char('c') if !is_paused => state
            .selected_agent_id()
            .map(|id| Action::PauseAgent { id }),
        KeyCode::Char('m') if !is_paused => {
            if state.selected_agent_id().is_some() {
                Some(Action::EnterInputMode(InputMode::ConfirmMerge))
            } else {
                None
            }
        }
        KeyCode::Char('p') if !is_paused => {
            if state.selected_agent_id().is_some() {
                Some(Action::EnterInputMode(InputMode::ConfirmPush))
            } else {
                None
            }
        }
        KeyCode::Char('f') if !is_paused => state
            .selected_agent_id()
            .map(|id| Action::FetchRemote { id }),
        KeyCode::Char('s') if !is_paused && !key.modifiers.contains(KeyModifiers::CONTROL) => state
            .selected_agent_id()
            .map(|id| Action::RequestSummary { id }),
        KeyCode::Char('/') => Some(Action::ToggleDiffView),
        KeyCode::Char('L') => Some(Action::ToggleLogs),
        KeyCode::Char('S') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::ToggleSettings)
        }

        // Git provider operations
        KeyCode::Char('o') => {
            let provider = state.settings.repo_config.git.provider;
            match provider {
                flock::app::GitProvider::GitLab => state
                    .selected_agent_id()
                    .map(|id| Action::OpenMrInBrowser { id }),
                flock::app::GitProvider::GitHub => state
                    .selected_agent_id()
                    .map(|id| Action::OpenPrInBrowser { id }),
                flock::app::GitProvider::Codeberg => state
                    .selected_agent_id()
                    .map(|id| Action::OpenCodebergPrInBrowser { id }),
            }
        }

        KeyCode::Char('a') => Some(Action::EnterInputMode(InputMode::AssignProjectTask)),
        KeyCode::Char('A') => state
            .selected_agent_id()
            .map(|id| Action::OpenProjectTaskInBrowser { id }),
        KeyCode::Char('t') => Some(Action::EnterInputMode(InputMode::BrowseTasks)),
        KeyCode::Char('T') => {
            tracing::info!("Shift+T pressed");
            let selected_id = state.selected_agent_id();
            if selected_id.is_none() {
                tracing::info!("No agent selected");
            }
            let result = selected_id
                .filter(|id| {
                    let is_linked = state
                        .agents
                        .get(id)
                        .map(|a| a.pm_task_status.is_linked())
                        .unwrap_or(false);
                    tracing::info!("Agent has linked task: {}", is_linked);
                    is_linked
                })
                .map(|id| Action::OpenTaskStatusDropdown { id });
            if result.is_none() {
                tracing::info!("Shift+T: no action generated");
            }
            result
        }

        // Other
        KeyCode::Char('R') => Some(Action::RefreshAll),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
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
    use flock::app::DropdownState;

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

    // Normal settings navigation
    match key.code {
        KeyCode::Esc => Some(Action::SettingsClose),
        KeyCode::Char('q') => Some(Action::SettingsSave),
        KeyCode::Tab => Some(Action::SettingsSwitchSection),
        KeyCode::BackTab => Some(Action::SettingsSwitchSectionBack),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::SettingsSelectPrev),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::SettingsSelectNext),
        KeyCode::Enter => Some(Action::SettingsSelectField),
        _ => None,
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
    pm_provider: ProjectMgmtProvider,
    _storage: &SessionStorage,
    action_tx: &mpsc::UnboundedSender<Action>,
    agent_watch_tx: &watch::Sender<HashSet<Uuid>>,
    branch_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
    selected_watch_tx: &watch::Sender<Option<Uuid>>,
    asana_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
    notion_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
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
            let worktree_symlinks = state.settings.repo_config.git.worktree_symlinks.clone();
            match agent_manager.create_agent(&name, &branch, &ai_agent, &worktree_symlinks) {
                Ok(mut agent) => {
                    state.log_info(format!("Agent '{}' created successfully", agent.name));

                    if let Some(ref task_item) = task {
                        let pm_status = match pm_provider {
                            ProjectMgmtProvider::Asana => {
                                ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted {
                                    gid: task_item.id.clone(),
                                    name: task_item.name.clone(),
                                    url: task_item.url.clone(),
                                })
                            }
                            ProjectMgmtProvider::Notion => {
                                ProjectMgmtTaskStatus::Notion(NotionTaskStatus::NotStarted {
                                    page_id: task_item.id.clone(),
                                    name: task_item.name.clone(),
                                    url: task_item.url.clone(),
                                    status_option_id: String::new(),
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
                    let session = flock::tmux::TmuxSession::new(&tmux_session);
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
        Action::UpdateAgentStatus { id, status } => {
            if let Some(agent) = state.agents.get_mut(&id) {
                if matches!(agent.status, flock::agent::AgentStatus::Paused) {
                    return Ok(false);
                }

                let old_label = agent.status.label();
                let new_label = status.label();
                let name = agent.name.clone();
                let changed = old_label != new_label;

                agent.set_status(status);
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
                                &format!("[flock] paused '{}'", name_clone),
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
                let worktree_symlinks = state.settings.repo_config.git.worktree_symlinks.clone();
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
                        let worktree = flock::git::Worktree::new(&repo_path, worktree_base);
                        if let Err(e) = worktree.create_symlinks(&worktree_path, &worktree_symlinks)
                        {
                            // Log but don't fail - symlinks are optional
                            eprintln!("Warning: Failed to create symlinks: {}", e);
                        }
                    }

                    let session = flock::tmux::TmuxSession::new(&tmux_session);
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
                let session = flock::tmux::TmuxSession::new(&tmux_session);
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
                let session = flock::tmux::TmuxSession::new(&tmux_session);
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
                let session = flock::tmux::TmuxSession::new(&tmux_session);
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
                let was_none = matches!(agent.mr_status, flock::gitlab::MergeRequestStatus::None);
                let is_open = matches!(&status, flock::gitlab::MergeRequestStatus::Open { .. });
                if was_none && is_open {
                    if let flock::gitlab::MergeRequestStatus::Open { iid, url, .. } = &status {
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
                let was_none = matches!(agent.mr_status, flock::gitlab::MergeRequestStatus::None);
                let was_pushing = agent.custom_note.as_deref() == Some("pushing...");
                let was_merging = agent.custom_note.as_deref() == Some("merging main...");

                match &status {
                    flock::gitlab::MergeRequestStatus::Open { .. } if was_none || was_pushing => {
                        Some("pushed".to_string())
                    }
                    flock::gitlab::MergeRequestStatus::Merged { .. } => Some("merged".to_string()),
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

        // GitHub operations
        Action::UpdatePrStatus { id, status } => {
            let should_log = state.agents.get(&id).and_then(|agent| {
                let was_none = matches!(agent.pr_status, flock::github::PullRequestStatus::None);
                let is_open = matches!(&status, flock::github::PullRequestStatus::Open { .. });
                if was_none && is_open {
                    if let flock::github::PullRequestStatus::Open { number, url, .. } = &status {
                        Some((agent.name.clone(), *number, url.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            let auto_note = state.agents.get(&id).and_then(|agent| {
                let was_none = matches!(agent.pr_status, flock::github::PullRequestStatus::None);
                let was_pushing = agent.custom_note.as_deref() == Some("pushing...");
                let was_merging = agent.custom_note.as_deref() == Some("merging main...");

                match &status {
                    flock::github::PullRequestStatus::Open { .. } if was_none || was_pushing => {
                        Some("pushed".to_string())
                    }
                    flock::github::PullRequestStatus::Merged { .. } => Some("merged".to_string()),
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
                    flock::codeberg::PullRequestStatus::None
                );
                let is_open = matches!(&status, flock::codeberg::PullRequestStatus::Open { .. });
                if was_none && is_open {
                    if let flock::codeberg::PullRequestStatus::Open { number, url, .. } = &status {
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
                        let status = if task.completed {
                            AsanaTaskStatus::Completed {
                                gid: task.gid,
                                name: task.name,
                            }
                        } else {
                            AsanaTaskStatus::NotStarted {
                                gid: task.gid,
                                name: task.name,
                                url,
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

        Action::AssignProjectTask { id, url_or_id } => match pm_provider {
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
                            let status = if task.completed {
                                AsanaTaskStatus::Completed {
                                    gid: task.gid,
                                    name: task.name,
                                }
                            } else {
                                AsanaTaskStatus::NotStarted {
                                    gid: task.gid,
                                    name: task.name,
                                    url,
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
                            let status = if page.is_completed {
                                NotionTaskStatus::Completed {
                                    page_id: page.id,
                                    name: page.name,
                                }
                            } else {
                                NotionTaskStatus::NotStarted {
                                    page_id: page.id,
                                    name: page.name,
                                    url: page.url,
                                    status_option_id: page.status_id.unwrap_or_default(),
                                }
                            };
                            let _ = tx.send(Action::UpdateProjectTaskStatus {
                                id,
                                status: ProjectMgmtTaskStatus::Notion(status),
                            });
                        }
                        Err(e) => {
                            let status = ProjectMgmtTaskStatus::Notion(NotionTaskStatus::Error {
                                page_id,
                                message: e.to_string(),
                            });
                            let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                        }
                    }
                });
            }
        },

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
                    NotionTaskStatus::NotStarted { name, .. } => {
                        Some(format!("Notion task '{}' linked", name))
                    }
                    NotionTaskStatus::InProgress { name, .. } => {
                        Some(format!("Notion task '{}' in progress", name))
                    }
                    NotionTaskStatus::Completed { name, .. } => {
                        Some(format!("Notion task '{}' completed", name))
                    }
                    NotionTaskStatus::Error { message, .. } => {
                        Some(format!("Notion error: {}", message))
                    }
                    NotionTaskStatus::None => None,
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
        }

        Action::CycleTaskStatus { id } => {
            if let Some(agent) = state.agents.get(&id) {
                let current_status = agent.pm_task_status.clone();
                match &current_status {
                    ProjectMgmtTaskStatus::Asana(asana_status) => match asana_status {
                        AsanaTaskStatus::NotStarted { gid, name, url } => {
                            let gid = gid.clone();
                            let name = name.clone();
                            let url = url.clone();
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::InProgress {
                                        gid: gid.clone(),
                                        name: name.clone(),
                                        url: url.clone(),
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
                        AsanaTaskStatus::InProgress { gid, name, .. } => {
                            let gid = gid.clone();
                            let name = name.clone();
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::Completed {
                                        gid: gid.clone(),
                                        name: name.clone(),
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
                        AsanaTaskStatus::Completed { name, .. } => {
                            let gid = match asana_status.gid() {
                                Some(g) => g.to_string(),
                                None => return Ok(false),
                            };
                            let name = name.clone();
                            let agent_id = id;
                            if let Some(agent) = state.agents.get_mut(&id) {
                                agent.pm_task_status =
                                    ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted {
                                        gid: gid.clone(),
                                        name: name.clone(),
                                        url: String::new(),
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
                    ProjectMgmtTaskStatus::Notion(_) | ProjectMgmtTaskStatus::None => {}
                }
            }
        }

        Action::OpenTaskStatusDropdown { id } => {
            tracing::info!("OpenTaskStatusDropdown called for agent {}", id);
            if let Some(agent) = state.agents.get(&id) {
                tracing::info!("Agent found, pm_task_status: {:?}", agent.pm_task_status);
                match &agent.pm_task_status {
                    ProjectMgmtTaskStatus::Notion(notion_status) => {
                        if !notion_client.is_configured() {
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
                        if !asana_client.is_configured() {
                            state.show_error(
                                "Asana not configured. Set ASANA_TOKEN and project_gid.",
                            );
                            return Ok(false);
                        }
                        if asana_status.gid().is_none() {
                            state.show_error("No Asana task linked to this agent");
                            return Ok(false);
                        }
                        let options = vec![
                            StatusOption {
                                id: "not_started".to_string(),
                                name: "Not Started".to_string(),
                            },
                            StatusOption {
                                id: "in_progress".to_string(),
                                name: "In Progress".to_string(),
                            },
                            StatusOption {
                                id: "done".to_string(),
                                name: "Done".to_string(),
                            },
                        ];
                        state.task_status_dropdown = Some(TaskStatusDropdownState {
                            agent_id: id,
                            status_options: options,
                            selected_index: 0,
                        });
                        state.input_mode = Some(InputMode::SelectTaskStatus);
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

        Action::TaskStatusDropdownSelect => {
            tracing::info!("TaskStatusDropdownSelect triggered");
            let dropdown = state.task_status_dropdown.take();
            state.exit_input_mode();
            if let Some(dropdown) = dropdown {
                let agent_id = dropdown.agent_id;
                if let Some(selected_option) = dropdown.status_options.get(dropdown.selected_index)
                {
                    let option_id = selected_option.id.clone();
                    let option_name = selected_option.name.clone();

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
                                        NotionTaskStatus::NotStarted { name, .. } => name.clone(),
                                        NotionTaskStatus::InProgress { name, .. } => name.clone(),
                                        NotionTaskStatus::Completed { name, .. } => name.clone(),
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

                                    let new_status = if option_name.to_lowercase().contains("done")
                                        || option_name.to_lowercase().contains("complete")
                                    {
                                        ProjectMgmtTaskStatus::Notion(NotionTaskStatus::Completed {
                                            page_id: page_id.clone(),
                                            name: task_name.clone(),
                                        })
                                    } else if option_name.to_lowercase().contains("progress") {
                                        ProjectMgmtTaskStatus::Notion(
                                            NotionTaskStatus::InProgress {
                                                page_id: page_id.clone(),
                                                name: task_name.clone(),
                                                url: String::new(),
                                                status_option_id: option_id.clone(),
                                            },
                                        )
                                    } else {
                                        ProjectMgmtTaskStatus::Notion(
                                            NotionTaskStatus::NotStarted {
                                                page_id: page_id.clone(),
                                                name: task_name.clone(),
                                                url: String::new(),
                                                status_option_id: option_id.clone(),
                                            },
                                        )
                                    };

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
                                    let gid = gid_str.to_string();
                                    let name = asana_status.format_short();
                                    let client = Arc::clone(asana_client);
                                    let in_progress_gid = state
                                        .settings
                                        .repo_config
                                        .project_mgmt
                                        .asana
                                        .in_progress_section_gid
                                        .clone();
                                    let done_gid = state
                                        .settings
                                        .repo_config
                                        .project_mgmt
                                        .asana
                                        .done_section_gid
                                        .clone();
                                    let agent_id_clone = agent_id;

                                    let new_status = match option_id.as_str() {
                                        "not_started" => {
                                            let status = ProjectMgmtTaskStatus::Asana(
                                                AsanaTaskStatus::NotStarted {
                                                    gid: gid.clone(),
                                                    name: name.clone(),
                                                    url: String::new(),
                                                },
                                            );
                                            tokio::spawn(async move {
                                                let _ = client.uncomplete_task(&gid).await;
                                            });
                                            status
                                        }
                                        "in_progress" => {
                                            let status = ProjectMgmtTaskStatus::Asana(
                                                AsanaTaskStatus::InProgress {
                                                    gid: gid.clone(),
                                                    name: name.clone(),
                                                    url: String::new(),
                                                },
                                            );
                                            let client = Arc::clone(&client);
                                            tokio::spawn(async move {
                                                let _ = client
                                                    .move_to_in_progress(
                                                        &gid,
                                                        in_progress_gid.as_deref(),
                                                    )
                                                    .await;
                                            });
                                            status
                                        }
                                        "done" => {
                                            let status = ProjectMgmtTaskStatus::Asana(
                                                AsanaTaskStatus::Completed {
                                                    gid: gid.clone(),
                                                    name: name.clone(),
                                                },
                                            );
                                            tokio::spawn(async move {
                                                let _ = client.complete_task(&gid).await;
                                                let _ = client
                                                    .move_to_done(&gid, done_gid.as_deref())
                                                    .await;
                                            });
                                            status
                                        }
                                        _ => return Ok(false),
                                    };

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
                    ProjectMgmtTaskStatus::None => {}
                }
            }

            action_tx.send(Action::DeleteAgent { id })?;
        }

        Action::FetchTaskList => {
            let provider = pm_provider;
            let asana_client = Arc::clone(asana_client);
            let notion_client = Arc::clone(notion_client);
            let tx = action_tx.clone();
            tokio::spawn(async move {
                let result = match provider {
                    ProjectMgmtProvider::Asana => {
                        match asana_client.get_project_tasks_with_subtasks().await {
                            Ok(tasks) => {
                                let items: Vec<TaskListItem> = tasks
                                    .into_iter()
                                    .filter(|t| !t.completed)
                                    .map(|t| TaskListItem {
                                        id: t.gid,
                                        name: t.name,
                                        status: TaskItemStatus::NotStarted,
                                        status_name: "Not Started".to_string(),
                                        url: t.permalink_url.unwrap_or_default(),
                                        parent_id: t.parent_gid,
                                        has_children: t.num_subtasks > 0,
                                    })
                                    .collect();
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

                                let items: Vec<TaskListItem> = pages
                                    .into_iter()
                                    .map(|p| {
                                        let status_name = p
                                            .status_name
                                            .clone()
                                            .unwrap_or_else(|| "Unknown".to_string());
                                        let status =
                                            if status_name.to_lowercase().contains("progress") {
                                                TaskItemStatus::InProgress
                                            } else {
                                                TaskItemStatus::NotStarted
                                            };
                                        let has_children = parent_ids.contains(&p.id);
                                        TaskListItem {
                                            id: p.id,
                                            name: p.name,
                                            status,
                                            status_name,
                                            url: p.url,
                                            parent_id: p.parent_page_id,
                                            has_children,
                                        }
                                    })
                                    .collect();
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

        Action::CreateAgentFromSelectedTask => {
            if let Some(task) = state.task_list.get(state.task_list_selected).cloned() {
                let branch = flock::util::sanitize_branch_name(&task.name);
                if branch.is_empty() {
                    state.show_error("Invalid task name for branch");
                } else {
                    let name = task.name.clone();
                    state.log_info(format!("Creating agent '{}' on branch '{}'", name, branch));
                    let ai_agent = state.config.global.ai_agent.clone();
                    let worktree_symlinks =
                        state.settings.repo_config.git.worktree_symlinks.clone();
                    match agent_manager.create_agent(&name, &branch, &ai_agent, &worktree_symlinks)
                    {
                        Ok(mut agent) => {
                            state.log_info(format!("Agent '{}' created successfully", agent.name));

                            let pm_status = match pm_provider {
                                ProjectMgmtProvider::Asana => {
                                    ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::NotStarted {
                                        gid: task.id.clone(),
                                        name: task.name.clone(),
                                        url: task.url.clone(),
                                    })
                                }
                                ProjectMgmtProvider::Notion => {
                                    ProjectMgmtTaskStatus::Notion(NotionTaskStatus::NotStarted {
                                        page_id: task.id.clone(),
                                        name: task.name.clone(),
                                        url: task.url.clone(),
                                        status_option_id: String::new(),
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
                            Some(flock::app::TaskReassignmentWarning {
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
                            let branch = flock::util::sanitize_branch_name(&input);
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
                                        Some(flock::app::TaskReassignmentWarning {
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
                    if !matches!(status, flock::gitlab::MergeRequestStatus::None) {
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
                    if !matches!(status, flock::github::PullRequestStatus::None) {
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
                    if !matches!(status, flock::codeberg::PullRequestStatus::None) {
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
                                    let status = if task.completed {
                                        flock::asana::AsanaTaskStatus::Completed {
                                            gid: task.gid,
                                            name: task.name,
                                        }
                                    } else {
                                        flock::asana::AsanaTaskStatus::InProgress {
                                            gid: task.gid,
                                            name: task.name,
                                            url,
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
                                    let status = if page.is_completed {
                                        NotionTaskStatus::Completed {
                                            page_id: page.id,
                                            name: page.name,
                                        }
                                    } else {
                                        NotionTaskStatus::InProgress {
                                            page_id: page.id,
                                            name: page.name,
                                            url: page.url,
                                            status_option_id: page.status_id.unwrap_or_default(),
                                        }
                                    };
                                    let _ = tx_clone.send(Action::UpdateProjectTaskStatus {
                                        id,
                                        status: ProjectMgmtTaskStatus::Notion(status),
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
                    agent.status = flock::agent::AgentStatus::Paused;
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
                    agent.status = flock::agent::AgentStatus::Running;
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
                state.settings.tab = flock::app::SettingsTab::General;
                state.settings.field_index = 0;
                state.settings.dropdown = flock::app::DropdownState::Closed;
                state.settings.editing_text = false;
                state.settings.pending_ai_agent = state.config.global.ai_agent.clone();
                state.settings.pending_log_level = state.config.global.log_level;
                state.settings.pending_worktree_location = state.config.global.worktree_location;
                state.settings.pending_ui = state.config.ui.clone();
            }
        }

        Action::SettingsSwitchSection => {
            state.settings.tab = state.settings.next_tab();
            state.settings.field_index = 0;
            state.settings.dropdown = flock::app::DropdownState::Closed;
            state.settings.editing_text = false;
        }

        Action::SettingsSwitchSectionBack => {
            state.settings.tab = state.settings.prev_tab();
            state.settings.field_index = 0;
            state.settings.dropdown = flock::app::DropdownState::Closed;
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
            if let flock::app::DropdownState::Open { selected_index } = &state.settings.dropdown {
                state.settings.dropdown = flock::app::DropdownState::Open {
                    selected_index: selected_index.saturating_sub(1),
                };
            }
        }

        Action::SettingsDropdownNext => {
            let field = state.settings.current_field();
            if let flock::app::DropdownState::Open { selected_index } = &state.settings.dropdown {
                let max = match field {
                    flock::app::SettingsField::AiAgent => flock::app::AiAgent::all().len(),
                    flock::app::SettingsField::GitProvider => flock::app::GitProvider::all().len(),
                    flock::app::SettingsField::LogLevel => flock::app::ConfigLogLevel::all().len(),
                    flock::app::SettingsField::WorktreeLocation => {
                        flock::app::WorktreeLocation::all().len()
                    }
                    flock::app::SettingsField::CodebergCiProvider => {
                        flock::app::CodebergCiProvider::all().len()
                    }
                    flock::app::SettingsField::ProjectMgmtProvider => {
                        flock::app::ProjectMgmtProvider::all().len()
                    }
                    _ => 0,
                };
                state.settings.dropdown = flock::app::DropdownState::Open {
                    selected_index: (*selected_index + 1).min(max.saturating_sub(1)),
                };
            }
        }

        Action::SettingsSelectField => {
            let field = state.settings.current_field();
            match field {
                flock::app::SettingsField::AiAgent => {
                    let current = &state.settings.pending_ai_agent;
                    let idx = flock::app::AiAgent::all()
                        .iter()
                        .position(|a| a == current)
                        .unwrap_or(0);
                    state.settings.dropdown = flock::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                flock::app::SettingsField::GitProvider => {
                    let current = &state.settings.repo_config.git.provider;
                    let idx = flock::app::GitProvider::all()
                        .iter()
                        .position(|g| g == current)
                        .unwrap_or(0);
                    state.settings.dropdown = flock::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                flock::app::SettingsField::LogLevel => {
                    let current = &state.settings.pending_log_level;
                    let idx = flock::app::ConfigLogLevel::all()
                        .iter()
                        .position(|l| l == current)
                        .unwrap_or(0);
                    state.settings.dropdown = flock::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                flock::app::SettingsField::WorktreeLocation => {
                    let current = &state.settings.pending_worktree_location;
                    let idx = flock::app::WorktreeLocation::all()
                        .iter()
                        .position(|w| w == current)
                        .unwrap_or(0);
                    state.settings.dropdown = flock::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                flock::app::SettingsField::CodebergCiProvider => {
                    let current = &state.settings.repo_config.git.codeberg.ci_provider;
                    let idx = flock::app::CodebergCiProvider::all()
                        .iter()
                        .position(|c| c == current)
                        .unwrap_or(0);
                    state.settings.dropdown = flock::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                flock::app::SettingsField::BranchPrefix => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.git.branch_prefix.clone();
                }
                flock::app::SettingsField::MainBranch => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state.settings.repo_config.git.main_branch.clone();
                }
                flock::app::SettingsField::WorktreeSymlinks => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.git.worktree_symlinks.join(", ");
                }
                flock::app::SettingsField::GitLabProjectId => {
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
                flock::app::SettingsField::GitLabBaseUrl => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.git.gitlab.base_url.clone();
                }
                flock::app::SettingsField::GitHubOwner => {
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
                flock::app::SettingsField::GitHubRepo => {
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
                flock::app::SettingsField::CodebergOwner => {
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
                flock::app::SettingsField::CodebergRepo => {
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
                flock::app::SettingsField::CodebergBaseUrl => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.git.codeberg.base_url.clone();
                }
                flock::app::SettingsField::AsanaProjectGid => {
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
                flock::app::SettingsField::AsanaInProgressGid => {
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
                flock::app::SettingsField::AsanaDoneGid => {
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
                flock::app::SettingsField::SummaryPrompt => {
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
                flock::app::SettingsField::MergePrompt => {
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
                flock::app::SettingsField::PushPrompt => {
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
                        flock::app::AiAgent::Opencode => {
                            &state.settings.repo_config.prompts.push_prompt_opencode
                        }
                        flock::app::AiAgent::Codex => {
                            &state.settings.repo_config.prompts.push_prompt_codex
                        }
                        flock::app::AiAgent::Gemini => {
                            &state.settings.repo_config.prompts.push_prompt_gemini
                        }
                        flock::app::AiAgent::ClaudeCode => &None,
                    };
                    state.settings.text_buffer = current
                        .clone()
                        .unwrap_or_else(|| default_prompt.to_string());
                }
                flock::app::SettingsField::ShowPreview => {
                    state.settings.pending_ui.show_preview =
                        !state.settings.pending_ui.show_preview;
                    state.config.ui.show_preview = state.settings.pending_ui.show_preview;
                    state.show_logs = state.config.ui.show_logs;
                }
                flock::app::SettingsField::ShowMetrics => {
                    state.settings.pending_ui.show_metrics =
                        !state.settings.pending_ui.show_metrics;
                    state.config.ui.show_metrics = state.settings.pending_ui.show_metrics;
                }
                flock::app::SettingsField::ShowLogs => {
                    state.settings.pending_ui.show_logs = !state.settings.pending_ui.show_logs;
                    state.config.ui.show_logs = state.settings.pending_ui.show_logs;
                    state.show_logs = state.config.ui.show_logs;
                }
                flock::app::SettingsField::ShowBanner => {
                    state.settings.pending_ui.show_banner = !state.settings.pending_ui.show_banner;
                    state.config.ui.show_banner = state.settings.pending_ui.show_banner;
                }
                flock::app::SettingsField::ProjectMgmtProvider => {
                    let current = state.settings.repo_config.project_mgmt.provider;
                    let idx = flock::app::ProjectMgmtProvider::all()
                        .iter()
                        .position(|p| *p == current)
                        .unwrap_or(0);
                    state.settings.dropdown = flock::app::DropdownState::Open {
                        selected_index: idx,
                    };
                }
                flock::app::SettingsField::NotionDatabaseId => {
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
                flock::app::SettingsField::NotionStatusProperty => {
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
                flock::app::SettingsField::NotionInProgressOption => {
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
                flock::app::SettingsField::NotionDoneOption => {
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
                flock::app::SettingsField::DevServerCommand => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .dev_server
                        .command
                        .clone()
                        .unwrap_or_default();
                }
                flock::app::SettingsField::DevServerRunBefore => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.dev_server.run_before.join(", ");
                }
                flock::app::SettingsField::DevServerWorkingDir => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer =
                        state.settings.repo_config.dev_server.working_dir.clone();
                }
                flock::app::SettingsField::DevServerPort => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state
                        .settings
                        .repo_config
                        .dev_server
                        .port
                        .map(|p| p.to_string())
                        .unwrap_or_default();
                }
                flock::app::SettingsField::DevServerAutoStart => {
                    state.settings.repo_config.dev_server.auto_start =
                        !state.settings.repo_config.dev_server.auto_start;
                }
            }
        }

        Action::SettingsConfirmSelection => {
            if state.settings.editing_text || state.settings.editing_prompt {
                let field = state.settings.current_field();
                match field {
                    flock::app::SettingsField::BranchPrefix => {
                        state.settings.repo_config.git.branch_prefix =
                            state.settings.text_buffer.clone();
                    }
                    flock::app::SettingsField::MainBranch => {
                        state.settings.repo_config.git.main_branch =
                            state.settings.text_buffer.clone();
                    }
                    flock::app::SettingsField::WorktreeSymlinks => {
                        state.settings.repo_config.git.worktree_symlinks = state
                            .settings
                            .text_buffer
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    flock::app::SettingsField::GitLabProjectId => {
                        state.settings.repo_config.git.gitlab.project_id =
                            state.settings.text_buffer.parse().ok();
                    }
                    flock::app::SettingsField::GitLabBaseUrl => {
                        state.settings.repo_config.git.gitlab.base_url =
                            state.settings.text_buffer.clone();
                    }
                    flock::app::SettingsField::GitHubOwner => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.git.github.owner =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::GitHubRepo => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.git.github.repo =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::CodebergOwner => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.git.codeberg.owner =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::CodebergRepo => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.git.codeberg.repo =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::CodebergBaseUrl => {
                        state.settings.repo_config.git.codeberg.base_url =
                            state.settings.text_buffer.clone();
                    }
                    flock::app::SettingsField::AsanaProjectGid => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.asana.project_gid =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::AsanaInProgressGid => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .asana
                            .in_progress_section_gid =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::AsanaDoneGid => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .asana
                            .done_section_gid = if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::DevServerCommand => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.dev_server.command =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::DevServerRunBefore => {
                        state.settings.repo_config.dev_server.run_before = state
                            .settings
                            .text_buffer
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    flock::app::SettingsField::DevServerWorkingDir => {
                        state.settings.repo_config.dev_server.working_dir =
                            state.settings.text_buffer.clone();
                    }
                    flock::app::SettingsField::DevServerPort => {
                        state.settings.repo_config.dev_server.port =
                            state.settings.text_buffer.parse().ok();
                    }
                    flock::app::SettingsField::SummaryPrompt => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.prompts.summary_prompt =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::MergePrompt => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.prompts.merge_prompt =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::PushPrompt => {
                        let val = state.settings.text_buffer.clone();
                        match state.settings.pending_ai_agent {
                            flock::app::AiAgent::Opencode => {
                                state.settings.repo_config.prompts.push_prompt_opencode =
                                    if val.is_empty() { None } else { Some(val) };
                            }
                            flock::app::AiAgent::Codex => {
                                state.settings.repo_config.prompts.push_prompt_codex =
                                    if val.is_empty() { None } else { Some(val) };
                            }
                            flock::app::AiAgent::Gemini => {
                                state.settings.repo_config.prompts.push_prompt_gemini =
                                    if val.is_empty() { None } else { Some(val) };
                            }
                            flock::app::AiAgent::ClaudeCode => {}
                        }
                    }
                    flock::app::SettingsField::NotionDatabaseId => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.notion.database_id =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::NotionStatusProperty => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .notion
                            .status_property_name = if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::NotionInProgressOption => {
                        let val = state.settings.text_buffer.clone();
                        state
                            .settings
                            .repo_config
                            .project_mgmt
                            .notion
                            .in_progress_option = if val.is_empty() { None } else { Some(val) };
                    }
                    flock::app::SettingsField::NotionDoneOption => {
                        let val = state.settings.text_buffer.clone();
                        state.settings.repo_config.project_mgmt.notion.done_option =
                            if val.is_empty() { None } else { Some(val) };
                    }
                    _ => {}
                }
                state.settings.editing_text = false;
                state.settings.editing_prompt = false;
                state.settings.text_buffer.clear();
            } else if let flock::app::DropdownState::Open { selected_index } =
                state.settings.dropdown
            {
                let field = state.settings.current_field();
                match field {
                    flock::app::SettingsField::AiAgent => {
                        if let Some(agent) = flock::app::AiAgent::all().get(selected_index) {
                            state.settings.pending_ai_agent = agent.clone();
                            state.config.global.ai_agent = agent.clone();
                        }
                    }
                    flock::app::SettingsField::GitProvider => {
                        if let Some(provider) = flock::app::GitProvider::all().get(selected_index) {
                            state.settings.repo_config.git.provider = *provider;
                        }
                    }
                    flock::app::SettingsField::LogLevel => {
                        if let Some(level) = flock::app::ConfigLogLevel::all().get(selected_index) {
                            state.settings.pending_log_level = *level;
                            state.config.global.log_level = *level;
                        }
                    }
                    flock::app::SettingsField::WorktreeLocation => {
                        if let Some(loc) = flock::app::WorktreeLocation::all().get(selected_index) {
                            state.settings.pending_worktree_location = *loc;
                            state.config.global.worktree_location = *loc;
                            state.worktree_base = state.config.worktree_base_path(&state.repo_path);
                        }
                    }
                    flock::app::SettingsField::CodebergCiProvider => {
                        if let Some(provider) =
                            flock::app::CodebergCiProvider::all().get(selected_index)
                        {
                            state.settings.repo_config.git.codeberg.ci_provider = *provider;
                        }
                    }
                    flock::app::SettingsField::ProjectMgmtProvider => {
                        if let Some(provider) =
                            flock::app::ProjectMgmtProvider::all().get(selected_index)
                        {
                            state.settings.repo_config.project_mgmt.provider = *provider;
                        }
                    }
                    _ => {}
                }
                state.settings.dropdown = flock::app::DropdownState::Closed;
            }
        }

        Action::SettingsCancelSelection => {
            state.settings.dropdown = flock::app::DropdownState::Closed;
            state.settings.editing_text = false;
            state.settings.editing_prompt = false;
            state.settings.text_buffer.clear();
        }

        Action::SettingsPromptSave => {
            let field = state.settings.current_field();
            match field {
                flock::app::SettingsField::SummaryPrompt => {
                    let val = state.settings.text_buffer.clone();
                    state.settings.repo_config.prompts.summary_prompt =
                        if val.is_empty() { None } else { Some(val) };
                }
                flock::app::SettingsField::MergePrompt => {
                    let val = state.settings.text_buffer.clone();
                    state.settings.repo_config.prompts.merge_prompt =
                        if val.is_empty() { None } else { Some(val) };
                }
                flock::app::SettingsField::PushPrompt => {
                    let val = state.settings.text_buffer.clone();
                    match state.settings.pending_ai_agent {
                        flock::app::AiAgent::Opencode => {
                            state.settings.repo_config.prompts.push_prompt_opencode =
                                if val.is_empty() { None } else { Some(val) };
                        }
                        flock::app::AiAgent::Codex => {
                            state.settings.repo_config.prompts.push_prompt_codex =
                                if val.is_empty() { None } else { Some(val) };
                        }
                        flock::app::AiAgent::Gemini => {
                            state.settings.repo_config.prompts.push_prompt_gemini =
                                if val.is_empty() { None } else { Some(val) };
                        }
                        flock::app::AiAgent::ClaudeCode => {}
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
            state.config.global.ai_agent = state.settings.pending_ai_agent.clone();
            state.config.global.log_level = state.settings.pending_log_level;
            state.config.global.worktree_location = state.settings.pending_worktree_location;
            state.config.ui = state.settings.pending_ui.clone();

            if let Err(e) = state.config.save() {
                state.log_error(format!("Failed to save config: {}", e));
            }

            if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
                state.log_error(format!("Failed to save repo config: {}", e));
            }

            state.show_logs = state.config.ui.show_logs;
            state.worktree_base = state.config.worktree_base_path(&state.repo_path);
            state.settings.active = false;
            state.log_info("Settings saved".to_string());
        }

        // Global Setup Wizard Actions
        Action::GlobalSetupNextStep => {
            if let Some(wizard) = &mut state.global_setup {
                wizard.step = flock::app::GlobalSetupStep::AgentSettings;
            }
        }
        Action::GlobalSetupPrevStep => {
            if let Some(wizard) = &mut state.global_setup {
                wizard.step = flock::app::GlobalSetupStep::WorktreeLocation;
            }
        }
        Action::GlobalSetupSelectNext => {
            if let Some(wizard) = &mut state.global_setup {
                let all = flock::app::config::WorktreeLocation::all();
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
                let all = flock::app::config::WorktreeLocation::all();
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
                    wizard.dropdown_index = flock::app::config::AiAgent::all()
                        .iter()
                        .position(|a| *a == wizard.ai_agent)
                        .unwrap_or(0);
                } else {
                    wizard.dropdown_index = flock::app::config::LogLevel::all()
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
                    flock::app::config::AiAgent::all().len()
                } else {
                    flock::app::config::LogLevel::all().len()
                };
                if wizard.dropdown_index < max.saturating_sub(1) {
                    wizard.dropdown_index += 1;
                }
            }
        }
        Action::GlobalSetupConfirmDropdown => {
            if let Some(wizard) = &mut state.global_setup {
                if wizard.field_index == 0 {
                    let all_agents = flock::app::config::AiAgent::all();
                    if wizard.dropdown_index < all_agents.len() {
                        wizard.ai_agent = all_agents[wizard.dropdown_index].clone();
                    }
                } else {
                    let all_levels = flock::app::config::LogLevel::all();
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
                let repo_config_path = flock::app::RepoConfig::config_path(&state.repo_path).ok();
                let project_needs_setup = repo_config_path
                    .as_ref()
                    .map(|p| !p.exists())
                    .unwrap_or(true);
                if project_needs_setup {
                    state.show_project_setup = true;
                    state.project_setup = Some(flock::app::ProjectSetupState::default());
                }
            }
        }

        // Project Setup Wizard Actions
        Action::ProjectSetupNavigateNext => {
            if let Some(wizard) = &mut state.project_setup {
                let max_fields = get_project_fields(&wizard.config.git.provider).len();
                if wizard.field_index < max_fields.saturating_sub(1) {
                    wizard.field_index += 1;
                }
            }
        }
        Action::ProjectSetupNavigatePrev => {
            if let Some(wizard) = &mut state.project_setup {
                if wizard.field_index > 0 {
                    wizard.field_index -= 1;
                }
            }
        }
        Action::ProjectSetupEditField => {
            if let Some(wizard) = &mut state.project_setup {
                let fields = get_project_fields(&wizard.config.git.provider);
                if let Some(field) = fields.get(wizard.field_index) {
                    if *field == ProjectSetupField::GitProvider {
                        wizard.dropdown_open = true;
                        wizard.dropdown_index = 0;
                    } else {
                        wizard.editing_text = true;
                        wizard.text_buffer = get_project_field_value(&wizard.config, field);
                    }
                }
            }
        }
        Action::ProjectSetupCancelEdit => {
            if let Some(wizard) = &mut state.project_setup {
                wizard.editing_text = false;
                wizard.text_buffer.clear();
            }
        }
        Action::ProjectSetupConfirmEdit => {
            if let Some(wizard) = &mut state.project_setup {
                let fields = get_project_fields(&wizard.config.git.provider);
                if let Some(field) = fields.get(wizard.field_index) {
                    set_project_field_value(&mut wizard.config, field, &wizard.text_buffer);
                }
                wizard.editing_text = false;
                wizard.text_buffer.clear();
            }
        }
        Action::ProjectSetupInputChar(c) => {
            if let Some(wizard) = &mut state.project_setup {
                wizard.text_buffer.push(c);
            }
        }
        Action::ProjectSetupBackspace => {
            if let Some(wizard) = &mut state.project_setup {
                wizard.text_buffer.pop();
            }
        }
        Action::ProjectSetupToggleDropdown => {
            if let Some(wizard) = &mut state.project_setup {
                wizard.dropdown_open = false;
            }
        }
        Action::ProjectSetupDropdownPrev => {
            if let Some(wizard) = &mut state.project_setup {
                if wizard.dropdown_index > 0 {
                    wizard.dropdown_index -= 1;
                }
            }
        }
        Action::ProjectSetupDropdownNext => {
            if let Some(wizard) = &mut state.project_setup {
                let max = flock::app::config::GitProvider::all().len();
                if wizard.dropdown_index < max.saturating_sub(1) {
                    wizard.dropdown_index += 1;
                }
            }
        }
        Action::ProjectSetupConfirmDropdown => {
            if let Some(wizard) = &mut state.project_setup {
                let all_providers = flock::app::config::GitProvider::all();
                if wizard.dropdown_index < all_providers.len() {
                    wizard.config.git.provider = all_providers[wizard.dropdown_index];
                }
                wizard.dropdown_open = false;
            }
        }
        Action::ProjectSetupSkip => {
            state.show_project_setup = false;
            state.project_setup = None;
            state.log_info("Project setup skipped".to_string());
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
                        state.devserver_warning = Some(flock::app::DevServerWarning {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectSetupField {
    GitProvider,
    GitLabProjectId,
    GitLabBaseUrl,
    GitHubOwner,
    GitHubRepo,
    CodebergOwner,
    CodebergRepo,
    CodebergBaseUrl,
    BranchPrefix,
    MainBranch,
    AsanaProjectGid,
}

fn get_project_fields(provider: &flock::app::config::GitProvider) -> Vec<ProjectSetupField> {
    use flock::app::config::GitProvider;
    let mut fields = vec![ProjectSetupField::GitProvider];
    match provider {
        GitProvider::GitLab => {
            fields.push(ProjectSetupField::GitLabProjectId);
            fields.push(ProjectSetupField::GitLabBaseUrl);
        }
        GitProvider::GitHub => {
            fields.push(ProjectSetupField::GitHubOwner);
            fields.push(ProjectSetupField::GitHubRepo);
        }
        GitProvider::Codeberg => {
            fields.push(ProjectSetupField::CodebergOwner);
            fields.push(ProjectSetupField::CodebergRepo);
            fields.push(ProjectSetupField::CodebergBaseUrl);
        }
    }
    fields.push(ProjectSetupField::BranchPrefix);
    fields.push(ProjectSetupField::MainBranch);
    fields.push(ProjectSetupField::AsanaProjectGid);
    fields
}

fn get_project_field_value(config: &flock::app::RepoConfig, field: &ProjectSetupField) -> String {
    match field {
        ProjectSetupField::GitProvider => config.git.provider.display_name().to_string(),
        ProjectSetupField::GitLabProjectId => config
            .git
            .gitlab
            .project_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        ProjectSetupField::GitLabBaseUrl => config.git.gitlab.base_url.clone(),
        ProjectSetupField::GitHubOwner => config.git.github.owner.clone().unwrap_or_default(),
        ProjectSetupField::GitHubRepo => config.git.github.repo.clone().unwrap_or_default(),
        ProjectSetupField::CodebergOwner => config.git.codeberg.owner.clone().unwrap_or_default(),
        ProjectSetupField::CodebergRepo => config.git.codeberg.repo.clone().unwrap_or_default(),
        ProjectSetupField::CodebergBaseUrl => config.git.codeberg.base_url.clone(),
        ProjectSetupField::BranchPrefix => config.git.branch_prefix.clone(),
        ProjectSetupField::MainBranch => config.git.main_branch.clone(),
        ProjectSetupField::AsanaProjectGid => config
            .project_mgmt
            .asana
            .project_gid
            .clone()
            .unwrap_or_default(),
    }
}

fn set_project_field_value(
    config: &mut flock::app::RepoConfig,
    field: &ProjectSetupField,
    value: &str,
) {
    match field {
        ProjectSetupField::GitLabProjectId => {
            config.git.gitlab.project_id = value.parse().ok();
        }
        ProjectSetupField::GitLabBaseUrl => {
            config.git.gitlab.base_url = value.to_string();
        }
        ProjectSetupField::GitHubOwner => {
            config.git.github.owner = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        ProjectSetupField::GitHubRepo => {
            config.git.github.repo = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        ProjectSetupField::BranchPrefix => {
            config.git.branch_prefix = if value.is_empty() {
                "feature/".to_string()
            } else {
                value.to_string()
            };
        }
        ProjectSetupField::MainBranch => {
            config.git.main_branch = if value.is_empty() {
                "main".to_string()
            } else {
                value.to_string()
            };
        }
        ProjectSetupField::AsanaProjectGid => {
            config.project_mgmt.asana.project_gid = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        ProjectSetupField::CodebergOwner => {
            config.git.codeberg.owner = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        ProjectSetupField::CodebergRepo => {
            config.git.codeberg.repo = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        ProjectSetupField::CodebergBaseUrl => {
            config.git.codeberg.base_url = if value.is_empty() {
                "https://codeberg.org".to_string()
            } else {
                value.to_string()
            };
        }
        _ => {}
    }
}

/// Background task to poll agent status from tmux sessions.
async fn poll_agents(
    mut agent_rx: watch::Receiver<HashSet<Uuid>>,
    mut selected_rx: watch::Receiver<Option<Uuid>>,
    tx: mpsc::UnboundedSender<Action>,
    ai_agent: flock::app::config::AiAgent,
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
            let session_name = format!("flock-{}", id.as_simple());

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
                        AgentStatus::Idle
                    });

                    let _ = tx.send(Action::UpdateAgentStatus { id, status });

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
                    let status = if task.completed {
                        ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::Completed {
                            gid: task.gid,
                            name: task.name,
                        })
                    } else {
                        ProjectMgmtTaskStatus::Asana(AsanaTaskStatus::InProgress {
                            gid: task.gid,
                            name: task.name,
                            url,
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
                    let status = if page.is_completed {
                        ProjectMgmtTaskStatus::Notion(NotionTaskStatus::Completed {
                            page_id: page.id,
                            name: page.name,
                        })
                    } else {
                        ProjectMgmtTaskStatus::Notion(NotionTaskStatus::InProgress {
                            page_id: page.id,
                            name: page.name,
                            url: page.url,
                            status_option_id: page.status_id.unwrap_or_default(),
                        })
                    };
                    let _ = tx.send(Action::UpdateProjectTaskStatus { id, status });
                }
                Err(e) => {
                    tracing::warn!("Failed to poll Notion page {}: {}", page_id, e);
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
            if !matches!(status, flock::gitlab::MergeRequestStatus::None) {
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
            if !matches!(status, flock::github::PullRequestStatus::None) {
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
            if !matches!(status, flock::codeberg::PullRequestStatus::None) {
                let _ = tx.send(Action::UpdateCodebergPrStatus { id, status });
            }
        }
    }
}
