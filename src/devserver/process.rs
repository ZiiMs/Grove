use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use uuid::Uuid;

use crate::app::{Action, DevServerConfig};

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
    process: Option<Child>,
    agent_name: String,
}

impl DevServer {
    pub fn new() -> Self {
        Self {
            status: DevServerStatus::Stopped,
            logs: VecDeque::with_capacity(MAX_LOG_LINES),
            process: None,
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

        let child = self.spawn_process(command, &working_dir)?;
        let pid = child.id().context("Failed to get process ID")?;

        self.spawn_log_streamer(child, agent_id, action_tx.clone());
        self.process = None;

        self.status = DevServerStatus::Running {
            pid,
            port: config.port,
        };

        self.append_log(format!("$ {}", command));
        self.append_log(format!("Dev server started (PID: {})", pid));

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if !self.status.is_running() {
            return Ok(());
        }

        let pid = match &self.status {
            DevServerStatus::Running { pid, .. } => *pid,
            _ => return Ok(()),
        };

        self.status = DevServerStatus::Stopping;
        self.append_log("Stopping dev server...".to_string());

        self.kill_process_tree(pid)?;

        self.status = DevServerStatus::Stopped;
        self.process = None;
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

    fn spawn_process(&mut self, command: &str, working_dir: &Path) -> Result<Child> {
        use std::process::Stdio;
        use tokio::process::Command;

        let child = Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .context("Failed to spawn dev server")?;

        Ok(child)
    }

    fn kill_process_tree(&self, pid: u32) -> Result<()> {
        let _ = std::process::Command::new("kill")
            .arg(format!("-{}", pid))
            .output();
        Ok(())
    }

    fn spawn_log_streamer(
        &self,
        mut child: Child,
        agent_id: Uuid,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        if let Some(stdout) = stdout {
            let tx = action_tx.clone();
            let id = agent_id;
            tokio::spawn(async move {
                let reader = BufReader::new(stdout).lines();
                let mut lines = reader;
                while let Some(line) = lines.next_line().await.ok().flatten() {
                    let _ = tx.send(Action::AppendDevServerLog { agent_id: id, line });
                }
            });
        }

        if let Some(stderr) = stderr {
            let id = agent_id;
            let tx = action_tx.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr).lines();
                let mut lines = reader;
                while let Some(line) = lines.next_line().await.ok().flatten() {
                    let _ = tx.send(Action::AppendDevServerLog {
                        agent_id: id,
                        line: format!("[stderr] {}", line),
                    });
                }
            });
        }

        let tx = action_tx.clone();
        let id = agent_id;
        tokio::spawn(async move {
            let _ = child.wait().await;
            let _ = tx.send(Action::UpdateDevServerStatus {
                agent_id: id,
                status: DevServerStatus::Stopped,
            });
        });
    }
}

impl Default for DevServer {
    fn default() -> Self {
        Self::new()
    }
}
