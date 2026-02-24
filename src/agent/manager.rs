use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::{Agent, AgentStatus};
use crate::app::config::AiAgent;
use crate::git::Worktree;
use crate::tmux::TmuxSession;

/// Manages the lifecycle of agents.
pub struct AgentManager {
    pub repo_path: String,
    pub worktree_base: PathBuf,
}

impl AgentManager {
    pub fn new(repo_path: &str, worktree_base: PathBuf) -> Self {
        Self {
            repo_path: repo_path.to_string(),
            worktree_base,
        }
    }

    /// Create a new agent with worktree and tmux session.
    pub fn create_agent(
        &self,
        name: &str,
        branch: &str,
        ai_agent: &AiAgent,
        worktree_symlinks: &[String],
    ) -> Result<Agent> {
        tracing::debug!(
            "AgentManager::create_agent - name: {:?}, branch: {:?}, repo_path: {:?}, worktree_base: {:?}",
            name,
            branch,
            self.repo_path,
            self.worktree_base
        );
        let worktree = Worktree::new(&self.repo_path, self.worktree_base.clone());
        let worktree_path = worktree
            .create(branch)
            .context("Failed to create worktree")?;

        worktree
            .create_symlinks(&worktree_path, worktree_symlinks)
            .context("Failed to create worktree symlinks")?;

        let agent = Agent::new(name.to_string(), branch.to_string(), worktree_path.clone());

        let session = TmuxSession::new(&agent.tmux_session);
        session
            .create(&worktree_path, ai_agent.command())
            .context("Failed to create tmux session")?;

        Ok(agent)
    }

    /// Delete an agent, cleaning up worktree and tmux session.
    pub fn delete_agent(&self, agent: &Agent) -> Result<()> {
        // Kill tmux session first
        let session = TmuxSession::new(&agent.tmux_session);
        if session.exists() {
            session.kill().context("Failed to kill tmux session")?;
        }

        // Remove worktree
        if Path::new(&agent.worktree_path).exists() {
            let worktree = Worktree::new(&self.repo_path, self.worktree_base.clone());
            worktree
                .remove(&agent.worktree_path)
                .context("Failed to remove worktree")?;
        }

        Ok(())
    }

    /// Attach to an agent's tmux session.
    /// Auto-recreates the session if it doesn't exist (e.g., after system restart).
    pub fn attach_to_agent(&self, agent: &Agent, ai_agent: &AiAgent) -> Result<()> {
        let session = TmuxSession::new(&agent.tmux_session);

        if !session.exists() {
            session
                .create(&agent.worktree_path, ai_agent.command())
                .context("Failed to create tmux session")?;
        }

        session.attach()
    }

    /// Get the current output from an agent's tmux session.
    pub fn capture_output(&self, agent: &Agent, lines: usize) -> Result<String> {
        let session = TmuxSession::new(&agent.tmux_session);

        if !session.exists() {
            return Ok(String::new());
        }

        session.capture_pane(lines)
    }

    /// Detect the current status of an agent from its output.
    pub fn detect_status(&self, agent: &Agent) -> Result<AgentStatus> {
        let output = self.capture_output(agent, 50)?;
        Ok(super::detector::detect_status(&output))
    }

    /// Send input to an agent's tmux session.
    pub fn send_input(&self, agent: &Agent, input: &str) -> Result<()> {
        let session = TmuxSession::new(&agent.tmux_session);

        if !session.exists() {
            anyhow::bail!("Tmux session does not exist");
        }

        session.send_keys(input)
    }

    /// Check if an agent's tmux session is still alive.
    pub fn is_session_alive(&self, agent: &Agent) -> bool {
        let session = TmuxSession::new(&agent.tmux_session);
        session.exists()
    }

    /// Restart an agent's AI session.
    pub fn restart_agent(&self, agent: &Agent, ai_agent: &AiAgent) -> Result<()> {
        let session = TmuxSession::new(&agent.tmux_session);

        if !session.exists() {
            session.create(&agent.worktree_path, ai_agent.command())?;
        } else {
            let _ = std::process::Command::new("tmux")
                .args(["send-keys", "-t", &agent.tmux_session, "C-c"])
                .output();

            std::thread::sleep(std::time::Duration::from_millis(100));
            session.send_keys(ai_agent.command())?;
        }

        Ok(())
    }

    /// Get info about all currently running grove sessions.
    pub fn list_running_sessions() -> Result<Vec<String>> {
        crate::tmux::list_grove_sessions()
    }

    /// Recover orphaned sessions (sessions without agents).
    pub fn find_orphaned_sessions(&self, known_agents: &[Uuid]) -> Result<Vec<String>> {
        let sessions = Self::list_running_sessions()?;
        let known_session_names: Vec<String> = known_agents
            .iter()
            .map(|id| format!("grove-{}", id.as_simple()))
            .collect();

        Ok(sessions
            .into_iter()
            .filter(|s| !known_session_names.contains(s))
            .collect())
    }
}
