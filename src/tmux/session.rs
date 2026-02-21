use anyhow::{Context, Result};
use std::process::Command;

/// Manages tmux sessions for agents.
pub struct TmuxSession {
    pub name: String,
}

impl TmuxSession {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    /// Create a new tmux session with the given working directory.
    /// Starts the specified command in the session.
    pub fn create(&self, working_dir: &str, command: &str) -> Result<()> {
        let output = Command::new("tmux")
            .args(["new-session", "-d", "-s", &self.name, "-c", working_dir])
            .output()
            .context("Failed to execute tmux")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create tmux session: {}", stderr);
        }

        self.send_keys(command)?;

        Ok(())
    }

    /// Check if the session exists.
    pub fn exists(&self) -> bool {
        Command::new("tmux")
            .args(["has-session", "-t", &self.name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Capture the current pane content.
    pub fn capture_pane(&self, lines: usize) -> Result<String> {
        let output = Command::new("tmux")
            .args([
                "capture-pane",
                "-t",
                &self.name,
                "-p",
                "-e", // Preserve ANSI escape sequences
                "-J", // Join wrapped lines
                "-S",
                &format!("-{}", lines),
            ])
            .output()
            .context("Failed to capture tmux pane")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to capture pane: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Send keys to the session (without pressing Enter).
    pub fn send_keys_raw(&self, keys: &str) -> Result<()> {
        let output = Command::new("tmux")
            .args(["send-keys", "-t", &self.name, "-l", keys])
            .output()
            .context("Failed to send keys to tmux")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to send keys: {}", stderr);
        }

        Ok(())
    }

    /// Send keys to the session and press Enter.
    pub fn send_keys(&self, keys: &str) -> Result<()> {
        // Send the text literally
        self.send_keys_raw(keys)?;

        // Send Enter key (C-m is Ctrl+M which equals Enter)
        let output = Command::new("tmux")
            .args(["send-keys", "-t", &self.name, "C-m"])
            .output()
            .context("Failed to send Enter to tmux")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to send Enter: {}", stderr);
        }

        Ok(())
    }

    /// Attach to the session (spawns as child process, returns when detached).
    pub fn attach(&self) -> Result<()> {
        let status = Command::new("tmux")
            .args(["attach-session", "-t", &self.name])
            .status()
            .context("Failed to attach to tmux session")?;

        if !status.success() {
            anyhow::bail!("Tmux attach exited with error");
        }

        Ok(())
    }

    /// Kill the session.
    pub fn kill(&self) -> Result<()> {
        let output = Command::new("tmux")
            .args(["kill-session", "-t", &self.name])
            .output()
            .context("Failed to kill tmux session")?;

        if !output.status.success() {
            // Session might not exist, which is fine
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("no server running") && !stderr.contains("session not found") {
                anyhow::bail!("Failed to kill session: {}", stderr);
            }
        }

        Ok(())
    }

    /// Get the foreground process name for this session's pane.
    /// Returns `None` if the tmux command fails (graceful fallback).
    pub fn pane_current_command(&self) -> Option<String> {
        let output = Command::new("tmux")
            .args([
                "display-message",
                "-t",
                &self.name,
                "-p",
                "#{pane_current_command}",
            ])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let cmd = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if cmd.is_empty() {
            None
        } else {
            Some(cmd)
        }
    }

    /// Get pane dimensions.
    pub fn pane_size(&self) -> Result<(u16, u16)> {
        let output = Command::new("tmux")
            .args([
                "display-message",
                "-t",
                &self.name,
                "-p",
                "#{pane_width} #{pane_height}",
            ])
            .output()
            .context("Failed to get pane size")?;

        if !output.status.success() {
            anyhow::bail!("Failed to get pane size");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.split_whitespace().collect();

        if parts.len() >= 2 {
            let width = parts[0].parse().unwrap_or(80);
            let height = parts[1].parse().unwrap_or(24);
            Ok((width, height))
        } else {
            Ok((80, 24))
        }
    }
}

/// Check if tmux is available on the system.
pub fn is_tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// List all grove-managed tmux sessions.
pub fn list_grove_sessions() -> Result<Vec<String>> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()
        .context("Failed to list tmux sessions")?;

    if !output.status.success() {
        // No sessions is not an error
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|s| s.starts_with("grove-"))
        .map(String::from)
        .collect())
}
