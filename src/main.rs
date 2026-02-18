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
    AgentStatus, ForegroundProcess,
};
use flock::app::{Action, AppState, Config, InputMode};
use flock::asana::{AsanaTaskStatus, OptionalAsanaClient};
use flock::git::GitSync;
use flock::gitlab::OptionalGitLabClient;
use flock::storage::{save_session, SessionStorage};
use flock::tmux::is_tmux_available;
use flock::ui::AppWidget;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("flock=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

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

    // Check if this is first launch (no config file exists)
    let is_first_launch = Config::config_path().map(|p| !p.exists()).unwrap_or(false);

    // Initialize storage
    let storage = SessionStorage::new(&repo_path)?;

    // Create app state
    let mut state = AppState::new(config.clone(), repo_path.clone());
    state.log_info(format!("Flock started in {}", repo_path));

    // Show settings on first launch
    if is_first_launch {
        state.settings.active = true;
        state.settings.section = flock::app::SettingsSection::Global;
        state.settings.field_index = 0;
        state.settings.pending_ai_agent = config.global.ai_agent.clone();
        state.settings.pending_git_provider = config.global.git_provider.clone();
        state.settings.pending_log_level = config.global.log_level;
        state.log_info("First launch - showing settings".to_string());
    }

    // Load existing session if any
    if let Ok(Some(session)) = storage.load() {
        let count = session.agents.len();
        for agent in session.agents {
            state.add_agent(agent);
        }
        state.selected_index = session
            .selected_index
            .min(state.agent_order.len().saturating_sub(1));
        state.log_info(format!("Loaded {} agents from session", count));
    }

    // Create agent manager
    let agent_manager = Arc::new(AgentManager::new(&repo_path));

    // Create GitLab client
    let gitlab_client = Arc::new(OptionalGitLabClient::new(
        &config.gitlab.base_url,
        config.gitlab.project_id,
        Config::gitlab_token().as_deref(),
    ));

    // Create Asana client
    let asana_client = Arc::new(OptionalAsanaClient::new(
        Config::asana_token().as_deref(),
        config.asana.project_gid.clone(),
    ));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create action channel
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();

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
    let (selected_watch_tx, selected_watch_rx) = watch::channel(initial_selected);

    // Start background polling task for agent status
    let agent_poll_tx = action_tx.clone();
    let selected_rx_clone = selected_watch_rx.clone();
    let ai_agent = config.global.ai_agent.clone();
    tokio::spawn(async move {
        poll_agents(agent_watch_rx, selected_rx_clone, agent_poll_tx, ai_agent).await;
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
        tokio::spawn(async move {
            poll_gitlab_mrs(branch_watch_rx, gitlab_client_clone, gitlab_poll_tx).await;
        });
        state.log_info("GitLab integration enabled".to_string());
    } else {
        state.log_debug("GitLab not configured (set GITLAB_TOKEN and project_id)".to_string());
    }

    // Create watch channel for Asana task tracking (agent_id, task_gid) pairs
    let initial_asana_tasks: Vec<(Uuid, String)> = state
        .agents
        .values()
        .filter_map(|a| a.asana_task_status.gid().map(|gid| (a.id, gid.to_string())))
        .collect();
    let (asana_watch_tx, asana_watch_rx) = watch::channel(initial_asana_tasks);

    // Start Asana polling task (if configured)
    if asana_client.is_configured() {
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

    // Main event loop
    let poll_timeout = Duration::from_millis(50);
    let tick_interval = Duration::from_millis(100);
    let mut last_tick = std::time::Instant::now();
    let mut pending_attach: Option<Uuid> = None;

    loop {
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
                let attach_result = agent_manager.attach_to_agent(&agent);

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
            AppWidget::new(&state).render(f);
        })?;

        // Poll for keyboard input (non-blocking with timeout)
        if poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                if let Some(action) = handle_key_event(key, &state) {
                    // Check if it's an attach action
                    if let Action::AttachToAgent { id } = action {
                        pending_attach = Some(id);
                        continue;
                    }
                    action_tx.send(action)?;
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
                &asana_client,
                &storage,
                &action_tx,
                &agent_watch_tx,
                &branch_watch_tx,
                &selected_watch_tx,
                &asana_watch_tx,
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

    // Handle input mode
    if state.is_input_mode() {
        return handle_input_mode_key(key.code, state);
    }

    // Handle help overlay
    if state.show_help {
        return Some(Action::ToggleHelp);
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

        // Yank (copy) agent name to clipboard
        KeyCode::Char('y') => state
            .selected_agent_id()
            .map(|id| Action::CopyAgentName { id }),

        // Notes (always allowed)
        KeyCode::Char('N') => Some(Action::EnterInputMode(InputMode::SetNote)),

        // These actions work regardless of pause state
        KeyCode::Char('n') => Some(Action::EnterInputMode(InputMode::NewAgent)),
        KeyCode::Char('d') => {
            let has_asana = state
                .selected_agent()
                .map(|a| a.asana_task_status.is_linked())
                .unwrap_or(false);
            if has_asana {
                Some(Action::EnterInputMode(InputMode::ConfirmDeleteAsana))
            } else {
                Some(Action::EnterInputMode(InputMode::ConfirmDelete))
            }
        }
        KeyCode::Enter => state
            .selected_agent_id()
            .map(|id| Action::AttachToAgent { id }),

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
        KeyCode::Char('s') if !is_paused => state
            .selected_agent_id()
            .map(|id| Action::RequestSummary { id }),
        KeyCode::Char('/') => Some(Action::ToggleDiffView),
        KeyCode::Char('L') => Some(Action::ToggleLogs),
        KeyCode::Char('S') => Some(Action::ToggleSettings),

        // GitLab operations
        KeyCode::Char('o') => state
            .selected_agent_id()
            .map(|id| Action::OpenMrInBrowser { id }),

        // Asana operations
        KeyCode::Char('a') => Some(Action::EnterInputMode(InputMode::AssignAsana)),
        KeyCode::Char('A') => state
            .selected_agent_id()
            .map(|id| Action::OpenAsanaInBrowser { id }),

        // Other
        KeyCode::Char('R') => Some(Action::RefreshAll),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Esc => Some(Action::ClearError),

        _ => None,
    }
}

/// Handle key events in input mode.
fn handle_input_mode_key(key: KeyCode, state: &AppState) -> Option<Action> {
    // Special 3-way confirm for delete with Asana: y=delete+complete, n=delete only, Esc=cancel
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

    // Check if we're in a confirmation mode
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
            KeyCode::Up | KeyCode::Char('k') => Some(Action::SettingsSelectPrev),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::SettingsSelectNext),
            _ => None,
        };
    }

    // Normal settings navigation
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::SettingsSave),
        KeyCode::Tab => Some(Action::SettingsSwitchSection),
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
    _gitlab_client: &Arc<OptionalGitLabClient>,
    asana_client: &Arc<OptionalAsanaClient>,
    _storage: &SessionStorage,
    action_tx: &mpsc::UnboundedSender<Action>,
    agent_watch_tx: &watch::Sender<HashSet<Uuid>>,
    branch_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
    selected_watch_tx: &watch::Sender<Option<Uuid>>,
    asana_watch_tx: &watch::Sender<Vec<(Uuid, String)>>,
) -> Result<bool> {
    match action {
        Action::Quit => {
            state.running = false;
            return Ok(true);
        }

        // Navigation (clear any lingering messages)
        Action::SelectNext => {
            state.error_message = None;
            state.select_next();
            let _ = selected_watch_tx.send(state.selected_agent_id());
        }
        Action::SelectPrevious => {
            state.error_message = None;
            state.select_previous();
            let _ = selected_watch_tx.send(state.selected_agent_id());
        }
        Action::SelectFirst => {
            state.error_message = None;
            state.select_first();
            let _ = selected_watch_tx.send(state.selected_agent_id());
        }
        Action::SelectLast => {
            state.error_message = None;
            state.select_last();
            let _ = selected_watch_tx.send(state.selected_agent_id());
        }

        // Agent lifecycle
        Action::CreateAgent { name, branch } => {
            state.log_info(format!("Creating agent '{}' on branch '{}'", name, branch));
            let ai_agent = state.config.global.ai_agent.clone();
            match agent_manager.create_agent(&name, &branch, &ai_agent) {
                Ok(agent) => {
                    state.log_info(format!("Agent '{}' created successfully", agent.name));
                    state.add_agent(agent);
                    state.select_last();
                    state.error_message = None;
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
                    state.error_message = Some(format!("Failed to create agent: {}", e));
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
                    let done_gid = state.config.asana.done_section_gid.clone();
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

        Action::DetachFromAgent => {
            // Handled in main loop
        }

        // Status updates
        Action::UpdateAgentStatus { id, status } => {
            if let Some(agent) = state.agents.get_mut(&id) {
                // Don't overwrite Paused status - that's manually controlled
                if matches!(agent.status, flock::agent::AgentStatus::Paused) {
                    return Ok(false);
                }

                // Trust the process-informed detector directly
                let final_status = status;

                // Only log if status actually changed
                let old_label = agent.status.label();
                let new_label = final_status.label();
                let name = agent.name.clone();
                let changed = old_label != new_label;

                // Check if transitioning to Running with a NotStarted Asana task
                let should_move_asana = changed
                    && matches!(&final_status, AgentStatus::Running)
                    && matches!(&agent.asana_task_status, AsanaTaskStatus::NotStarted { .. });
                let asana_info = if should_move_asana {
                    if let AsanaTaskStatus::NotStarted { gid, name, url } = &agent.asana_task_status
                    {
                        Some((id, gid.clone(), name.clone(), url.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                };

                agent.set_status(final_status);
                if changed {
                    state.log_debug(format!("Agent '{}': {} -> {}", name, old_label, new_label));
                }

                // Move Asana task to In Progress when agent starts running
                if let Some((agent_id, task_gid, task_name, task_url)) = asana_info {
                    if let Some(agent) = state.agents.get_mut(&agent_id) {
                        agent.asana_task_status = AsanaTaskStatus::InProgress {
                            gid: task_gid.clone(),
                            name: task_name.clone(),
                            url: task_url.clone(),
                        };
                    }
                    let client = Arc::clone(asana_client);
                    let override_gid = state.config.asana.in_progress_section_gid.clone();
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = client
                            .move_to_in_progress(&task_gid, override_gid.as_deref())
                            .await
                        {
                            let _ = tx.send(Action::UpdateAsanaTaskStatus {
                                id: agent_id,
                                status: AsanaTaskStatus::Error {
                                    gid: task_gid,
                                    message: format!("Failed to move to In Progress: {}", e),
                                },
                            });
                        }
                    });
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

                    // 3. Remove worktree (keeps branch)
                    let _ = std::process::Command::new("git")
                        .args(["worktree", "remove", "--force", &worktree_path])
                        .output();

                    // Prune worktrees
                    let _ = std::process::Command::new("git")
                        .args(["worktree", "prune"])
                        .output();

                    // 4. Copy checkout command to clipboard
                    let checkout_cmd = format!("git checkout {}", branch_clone);
                    let clipboard_result = Clipboard::new().and_then(|mut c| c.set_text(&checkout_cmd));
                    let message = if clipboard_result.is_ok() {
                        "Paused. Checkout command copied. Press 'r' to resume.".to_string()
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
            // Clone data we need before mutable borrows
            let main_branch = state.config.gitlab.main_branch.clone();
            let agent_info = state
                .agents
                .get(&id)
                .map(|a| (a.name.clone(), a.tmux_session.clone()));

            if let Some((name, tmux_session)) = agent_info {
                // Send a prompt to Claude to merge main into this branch
                let prompt = format!(
                    "Please merge {} into this branch. Handle any merge conflicts if they arise.",
                    main_branch
                );
                let session = flock::tmux::TmuxSession::new(&tmux_session);
                match session.send_keys(&prompt) {
                    Ok(()) => {
                        if let Some(agent) = state.agents.get_mut(&id) {
                            agent.custom_note = Some("merging main...".to_string());
                        }
                        state.log_info(format!("Sent merge request to agent '{}'", name));
                        state.error_message =
                            Some(format!("Sent merge {} request to Claude", main_branch));
                    }
                    Err(e) => {
                        state.log_error(format!("Failed to send merge request: {}", e));
                        state.error_message = Some(format!("Failed to send merge request: {}", e));
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
                let push_prompt = agent_type.push_prompt();

                let mut success = false;

                if let Some(cmd) = push_cmd {
                    match session.send_keys(cmd) {
                        Ok(()) => {
                            state.log_info(format!("Sent {} to agent '{}'", cmd, name));
                            success = true;
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to send {}: {}", cmd, e));
                            state.error_message = Some(format!("Failed to send {}: {}", cmd, e));
                        }
                    }
                }

                if let Some(prompt) = push_prompt {
                    match session.send_keys(prompt) {
                        Ok(()) => {
                            state.log_info(format!("Sent push prompt to agent '{}'", name));
                            success = true;
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to send push prompt: {}", e));
                            state.error_message = Some(format!("Failed to send push prompt: {}", e));
                        }
                    }
                }

                if success {
                    if let Some(agent) = state.agents.get_mut(&id) {
                        agent.custom_note = Some("pushing...".to_string());
                    }
                    state.error_message = Some(format!("Sent push command to {}", agent_type.display_name()));
                }
            }
        }

        Action::FetchRemote { id } => {
            if let Some(agent) = state.agents.get(&id) {
                let git_sync = GitSync::new(&agent.worktree_path);
                if let Err(e) = git_sync.fetch() {
                    state.error_message = Some(format!("Fetch failed: {}", e));
                }
            }
        }

        Action::RequestSummary { id } => {
            let agent_info = state
                .agents
                .get(&id)
                .map(|a| (a.name.clone(), a.tmux_session.clone()));

            if let Some((name, tmux_session)) = agent_info {
                let prompt = "Please provide a brief, non-technical summary of the work done on this branch. Format it as 1-5 bullet points suitable for sharing with non-technical colleagues on Slack. Focus on what was accomplished and why, not implementation details. Keep each bullet point to one sentence.";
                let session = flock::tmux::TmuxSession::new(&tmux_session);
                match session.send_keys(prompt) {
                    Ok(()) => {
                        if let Some(agent) = state.agents.get_mut(&id) {
                            agent.summary_requested = true;
                            agent.custom_note = Some("summary...".to_string());
                        }
                        state.log_info(format!("Requested summary from agent '{}'", name));
                        state.error_message =
                            Some("Requested work summary from Claude".to_string());
                    }
                    Err(e) => {
                        state.log_error(format!("Failed to request summary: {}", e));
                        state.error_message = Some(format!("Failed to request summary: {}", e));
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
            let should_log = state
                .agents
                .get(&id)
                .and_then(|agent| {
                    let was_none =
                        matches!(agent.mr_status, flock::gitlab::MergeRequestStatus::None);
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
                state.error_message = Some(format!("MR !{}: {}", iid, url));
            }
        }

        Action::OpenMrInBrowser { id } => {
            if let Some(agent) = state.agents.get(&id) {
                if let Some(url) = agent.mr_status.url() {
                    match std::process::Command::new("open").arg(url).spawn() {
                        Ok(_) => {
                            state.log_info("Opening MR in browser".to_string());
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to open browser: {}", e));
                            state.error_message = Some(format!("Failed to open browser: {}", e));
                        }
                    }
                } else {
                    state.error_message = Some("No MR available for this agent".to_string());
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
                agent.asana_task_status = status;
            }
            if let Some(msg) = log_msg {
                state.log_info(&msg);
                state.error_message = Some(msg);
            }
            // Update the Asana watch channel
            let asana_tasks: Vec<(Uuid, String)> = state
                .agents
                .values()
                .filter_map(|a| a.asana_task_status.gid().map(|gid| (a.id, gid.to_string())))
                .collect();
            let _ = asana_watch_tx.send(asana_tasks);
        }

        Action::OpenAsanaInBrowser { id } => {
            if let Some(agent) = state.agents.get(&id) {
                if let Some(url) = agent.asana_task_status.url() {
                    match std::process::Command::new("open").arg(url).spawn() {
                        Ok(_) => {
                            state.log_info("Opening Asana task in browser".to_string());
                        }
                        Err(e) => {
                            state.log_error(format!("Failed to open browser: {}", e));
                            state.error_message = Some(format!("Failed to open browser: {}", e));
                        }
                    }
                } else {
                    state.error_message = Some("No Asana task linked to this agent".to_string());
                }
            }
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
            state.error_message = Some(msg);
        }

        Action::ClearError => {
            state.error_message = None;
        }

        Action::EnterInputMode(mode) => {
            state.enter_input_mode(mode);
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
                            // Use branch name as both agent name and branch
                            action_tx.send(Action::CreateAgent {
                                name: input.clone(),
                                branch: input,
                            })?;
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
                    InputMode::ConfirmDeleteAsana => {
                        // Handled directly by key handler (y/n/Esc), not through SubmitInput
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
                        state.error_message = Some(format!("Copied '{}'", name));
                    }
                    Err(e) => {
                        state.error_message = Some(format!("Copy failed: {}", e));
                    }
                }
            }
        }

        // Application
        Action::RefreshAll => {
            // Trigger refresh of all status
            state.error_message = Some("Refreshing...".to_string());

            // Refresh git status for selected agent
            if let Some(agent) = state.selected_agent() {
                let git_sync = GitSync::new(&agent.worktree_path);
                if let Ok(status) = git_sync.get_status(&state.config.gitlab.main_branch) {
                    let id = agent.id;
                    action_tx.send(Action::UpdateGitStatus { id, status })?;
                }
            }
        }

        Action::Tick => {
            // Advance animation frame for spinner
            state.advance_animation();
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
            state.error_message = Some(message);
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
            } else {
                state.log_error(&message);
            }
            state.error_message = Some(message);
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
            } else {
                state.log_error(&message);
            }
            state.error_message = Some(message);
        }

        // Settings actions
        Action::ToggleSettings => {
            if state.settings.active {
                state.settings.active = false;
            } else {
                state.settings.active = true;
                state.settings.section = flock::app::SettingsSection::Global;
                state.settings.field_index = 0;
                state.settings.dropdown = flock::app::DropdownState::Closed;
                state.settings.editing_text = false;
                state.settings.pending_ai_agent = state.config.global.ai_agent.clone();
                state.settings.pending_git_provider = state.config.global.git_provider.clone();
                state.settings.pending_log_level = state.config.global.log_level;
            }
        }

        Action::SettingsSwitchSection => {
            state.settings.section = match state.settings.section {
                flock::app::SettingsSection::Global => flock::app::SettingsSection::Project,
                flock::app::SettingsSection::Project => flock::app::SettingsSection::Global,
            };
            state.settings.field_index = 0;
            state.settings.dropdown = flock::app::DropdownState::Closed;
            state.settings.editing_text = false;
        }

        Action::SettingsSelectNext => {
            let field = state.settings.current_field();
            if let flock::app::DropdownState::Open { selected_index } = &state.settings.dropdown {
                let max = match field {
                    flock::app::SettingsField::AiAgent => flock::app::AiAgent::all().len(),
                    flock::app::SettingsField::GitProvider => flock::app::GitProvider::all().len(),
                    flock::app::SettingsField::LogLevel => flock::app::ConfigLogLevel::all().len(),
                    _ => 0,
                };
                state.settings.dropdown = flock::app::DropdownState::Open {
                    selected_index: (*selected_index + 1).min(max.saturating_sub(1)),
                };
            } else if state.settings.editing_text {
                // No navigation in text mode
            } else {
                let total = state.settings.total_fields();
                state.settings.field_index = (state.settings.field_index + 1) % total;
            }
        }

        Action::SettingsSelectPrev => {
            if let flock::app::DropdownState::Open { selected_index } = &state.settings.dropdown {
                state.settings.dropdown = flock::app::DropdownState::Open {
                    selected_index: selected_index.saturating_sub(1),
                };
            } else if state.settings.editing_text {
                // No navigation in text mode
            } else {
                let total = state.settings.total_fields();
                state.settings.field_index = if state.settings.field_index == 0 {
                    total.saturating_sub(1)
                } else {
                    state.settings.field_index - 1
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
                    state.settings.dropdown = flock::app::DropdownState::Open { selected_index: idx };
                }
                flock::app::SettingsField::GitProvider => {
                    let current = &state.settings.pending_git_provider;
                    let idx = flock::app::GitProvider::all()
                        .iter()
                        .position(|g| g == current)
                        .unwrap_or(0);
                    state.settings.dropdown = flock::app::DropdownState::Open { selected_index: idx };
                }
                flock::app::SettingsField::LogLevel => {
                    let current = &state.settings.pending_log_level;
                    let idx = flock::app::ConfigLogLevel::all()
                        .iter()
                        .position(|l| l == current)
                        .unwrap_or(0);
                    state.settings.dropdown = flock::app::DropdownState::Open { selected_index: idx };
                }
                flock::app::SettingsField::BranchPrefix => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state.settings.project_config.branch_prefix.clone();
                }
                flock::app::SettingsField::MainBranch => {
                    state.settings.editing_text = true;
                    state.settings.text_buffer = state.settings.project_config.main_branch.clone();
                }
            }
        }

        Action::SettingsConfirmSelection => {
            if state.settings.editing_text {
                let field = state.settings.current_field();
                match field {
                    flock::app::SettingsField::BranchPrefix => {
                        state.settings.project_config.branch_prefix = state.settings.text_buffer.clone();
                    }
                    flock::app::SettingsField::MainBranch => {
                        state.settings.project_config.main_branch = state.settings.text_buffer.clone();
                    }
                    _ => {}
                }
                state.settings.editing_text = false;
                state.settings.text_buffer.clear();
            } else if let flock::app::DropdownState::Open { selected_index } = state.settings.dropdown {
                let field = state.settings.current_field();
                match field {
                    flock::app::SettingsField::AiAgent => {
                        if let Some(agent) = flock::app::AiAgent::all().get(selected_index) {
                            state.settings.pending_ai_agent = agent.clone();
                        }
                    }
                    flock::app::SettingsField::GitProvider => {
                        if let Some(provider) = flock::app::GitProvider::all().get(selected_index) {
                            state.settings.pending_git_provider = provider.clone();
                        }
                    }
                    flock::app::SettingsField::LogLevel => {
                        if let Some(level) = flock::app::ConfigLogLevel::all().get(selected_index) {
                            state.settings.pending_log_level = *level;
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
            state.settings.text_buffer.clear();
        }

        Action::SettingsInputChar(c) => {
            state.settings.text_buffer.push(c);
        }

        Action::SettingsBackspace => {
            state.settings.text_buffer.pop();
        }

        Action::SettingsSave => {
            state.config.global.ai_agent = state.settings.pending_ai_agent.clone();
            state.config.global.git_provider = state.settings.pending_git_provider.clone();
            state.config.global.log_level = state.settings.pending_log_level;

            if let Err(e) = state.config.save() {
                state.log_error(format!("Failed to save config: {}", e));
            }

            if let Err(e) = state.settings.project_config.save(&state.repo_path) {
                state.log_error(format!("Failed to save project config: {}", e));
            }

            state.settings.active = false;
            state.log_info("Settings saved".to_string());
        }
    }

    Ok(false)
}

/// Background task to poll agent status from tmux sessions.
async fn poll_agents(
    agent_rx: watch::Receiver<HashSet<Uuid>>,
    selected_rx: watch::Receiver<Option<Uuid>>,
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

    loop {
        // Poll every 250ms for responsive status updates
        tokio::time::sleep(Duration::from_millis(250)).await;
        deep_scan_counter += 1;

        // Get current agent list and selected agent
        let agent_ids = agent_rx.borrow().clone();
        let selected_id = *selected_rx.borrow();

        for id in agent_ids {
            let is_selected = selected_id == Some(id);
            let session_name = format!("flock-{}", id.as_simple());

            // Always do a plain capture (no ANSI, consistent line count) for status detection
            // -J joins wrapped lines so URLs and long text aren't split across lines
            if let Ok(output) = std::process::Command::new("tmux")
                .args([
                    "capture-pane",
                    "-t",
                    &session_name,
                    "-p",
                    "-J",
                    "-S",
                    "-100",
                ])
                .output()
            {
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
                    let status = detect_status_for_agent(&content, foreground, ai_agent.clone());

                    let _ = tx.send(Action::UpdateAgentStatus { id, status });

                    // Check for MR URLs in the short capture (only if not already tracked)
                    if !agents_with_mr.contains(&id) {
                        if let Some(mr_status) = detect_mr_url(&content) {
                            agents_with_mr.insert(id);
                            let _ = tx.send(Action::UpdateMrStatus {
                                id,
                                status: mr_status,
                            });
                        }
                    }

                    // Check for checklist progress
                    let progress = detect_checklist_progress(&content);
                    let _ = tx.send(Action::UpdateChecklistProgress { id, progress });
                }
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

            // Separate ANSI capture for the selected agent's preview
            if is_selected {
                if let Ok(output) = std::process::Command::new("tmux")
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
                    if output.status.success() {
                        let preview = String::from_utf8_lossy(&output.stdout).to_string();
                        let _ = tx.send(Action::UpdatePreviewContent(Some(preview)));
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
                        AsanaTaskStatus::Completed {
                            gid: task.gid,
                            name: task.name,
                        }
                    } else {
                        // Preserve InProgress if already in progress
                        AsanaTaskStatus::InProgress {
                            gid: task.gid,
                            name: task.name,
                            url,
                        }
                    };
                    let _ = tx.send(Action::UpdateAsanaTaskStatus { id, status });
                }
                Err(e) => {
                    tracing::warn!("Failed to poll Asana task {}: {}", gid, e);
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
) {
    // Do an immediate poll on startup
    let mut first_run = true;

    loop {
        if first_run {
            first_run = false;
            // Small delay to let the UI initialize
            tokio::time::sleep(Duration::from_millis(500)).await;
        } else {
            // Poll every 60 seconds after first run
            tokio::time::sleep(Duration::from_secs(60)).await;
        }

        // Get current agent branches
        let branches = branch_rx.borrow().clone();

        for (id, branch) in branches {
            let status = gitlab_client.get_mr_for_branch(&branch).await;
            // Only send update if there's an actual MR
            if !matches!(status, flock::gitlab::MergeRequestStatus::None) {
                let _ = tx.send(Action::UpdateMrStatus { id, status });
            }
        }
    }
}
