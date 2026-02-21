use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::path::Path;
use uuid::Uuid;

use crate::app::{Action, DevServerConfig};
use crate::tmux::TmuxSession;

const MAX_LOG_LINES: usize = 5000;

pub fn tmux_session_name(agent_id: Uuid) -> String {
    format!("flock-dev-{}", &agent_id.to_string()[..8])
}

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

    pub fn port(&self) -> Option<u16> {
        match self {
            DevServerStatus::Running { port, .. } => *port,
            _ => None,
        }
    }
}

pub struct DevServer {
    status: DevServerStatus,
    logs: VecDeque<String>,
    tmux_session: Option<String>,
    agent_name: String,
}

impl DevServer {
    pub fn new() -> Self {
        Self {
            status: DevServerStatus::Stopped,
            logs: VecDeque::with_capacity(MAX_LOG_LINES),
            tmux_session: None,
            agent_name: String::new(),
        }
    }

    pub async fn start(
        &mut self,
        config: &DevServerConfig,
        worktree_path: &Path,
        agent_id: Uuid,
        agent_name: String,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> Result<()> {
        if self.status.is_running() {
            anyhow::bail!("Dev server is already running");
        }

        let command = config
            .command
            .as_ref()
            .context("No dev server command configured")?;

        let working_dir = if config.working_dir.is_empty() {
            worktree_path.to_path_buf()
        } else {
            worktree_path.join(&config.working_dir)
        };

        self.status = DevServerStatus::Starting;
        self.agent_name = agent_name.clone();

        for cmd in &config.run_before {
            self.run_before_command(cmd, &working_dir)?;
            self.append_log(format!("$ {}", cmd));
        }

        let session_name = tmux_session_name(agent_id);
        let session = TmuxSession::new(&session_name);

        if session.exists() {
            session.kill()?;
        }

        session
            .create(&working_dir.to_string_lossy(), command)
            .context("Failed to create tmux session for dev server")?;

        let pid = self.get_tmux_session_pid(&session_name)?;

        self.tmux_session = Some(session_name.clone());
        self.status = DevServerStatus::Running {
            pid,
            port: config.port,
        };

        self.append_log(format!("$ {}", command));
        self.append_log(format!("Dev server started (PID: {})", pid));

        self.spawn_log_poller(agent_id, session_name, action_tx);

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if !self.status.is_running() {
            return Ok(());
        }

        self.status = DevServerStatus::Stopping;
        self.append_log("Stopping dev server...".to_string());

        if let Some(session_name) = &self.tmux_session {
            let session = TmuxSession::new(session_name);
            if session.exists() {
                session.kill()?;
            }
        }

        self.tmux_session = None;
        self.status = DevServerStatus::Stopped;
        self.append_log("Dev server stopped".to_string());

        Ok(())
    }

    pub async fn restart(
        &mut self,
        config: &DevServerConfig,
        worktree_path: &Path,
        agent_id: Uuid,
        agent_name: String,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> Result<()> {
        self.stop().await?;
        self.start(config, worktree_path, agent_id, agent_name, action_tx)
            .await
    }

    pub fn status(&self) -> &DevServerStatus {
        &self.status
    }

    pub fn logs(&self) -> &[String] {
        let (front, _) = self.logs.as_slices();
        front
    }

    pub fn append_log(&mut self, line: impl Into<String>) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(line.into());
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    pub fn set_agent_name(&mut self, name: String) {
        self.agent_name = name;
    }

    fn run_before_command(&mut self, command: &str, working_dir: &Path) -> Result<()> {
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .output()
            .context("Failed to run pre-command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            self.append_log(format!("Pre-command failed: {}", stderr));
        }

        Ok(())
    }

    fn get_tmux_session_pid(&self, session_name: &str) -> Result<u32> {
        let output = std::process::Command::new("tmux")
            .args(["list-panes", "-t", session_name, "-F", "#{pane_pid}"])
            .output()
            .context("Failed to get tmux pane PID")?;

        if !output.status.success() {
            anyhow::bail!("Failed to list panes for session {}", session_name);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid = stdout
            .lines()
            .next()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .context("Failed to parse pane PID")?;

        Ok(pid)
    }

    pub fn tmux_session(&self) -> Option<&str> {
        self.tmux_session.as_deref()
    }

    fn spawn_log_poller(
        &self,
        agent_id: Uuid,
        session_name: String,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) {
        let tx = action_tx;
        let id = agent_id;
        let session = session_name;

        tokio::spawn(async move {
            use tokio::time::{sleep, Duration};

            let mut last_content = String::new();

            loop {
                sleep(Duration::from_millis(500)).await;

                let tmux = TmuxSession::new(&session);
                if !tmux.exists() {
                    let _ = tx.send(Action::UpdateDevServerStatus {
                        agent_id: id,
                        status: DevServerStatus::Stopped,
                    });
                    break;
                }

                match tmux.capture_pane(100) {
                    Ok(content) => {
                        if content != last_content {
                            let new_lines: Vec<&str> = content
                                .lines()
                                .skip_while(|line| last_content.lines().any(|l| l == *line))
                                .collect();

                            for line in new_lines {
                                if !line.trim().is_empty() {
                                    let _ = tx.send(Action::AppendDevServerLog {
                                        agent_id: id,
                                        line: line.to_string(),
                                    });
                                }
                            }

                            last_content = content;
                        }
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }
        });
    }
}

impl Default for DevServer {
    fn default() -> Self {
        Self::new()
    }
}
