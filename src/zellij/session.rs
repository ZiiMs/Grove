use anyhow::{Context, Result};
use std::fs;
use std::process::Command;

const TEMP_DIR: &str = "/tmp";

pub struct ZellijSession {
    pub name: String,
}

impl ZellijSession {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    pub fn create(&self, working_dir: &str, command: &str) -> Result<()> {
        let output = Command::new("zellij")
            .args(["new-session", "-d", "-s", &self.name, "--cwd", working_dir])
            .output()
            .context("Failed to execute zellij")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create zellij session: {}", stderr);
        }

        std::thread::sleep(std::time::Duration::from_millis(100));

        self.send_keys(command)?;

        Ok(())
    }

    pub fn exists(&self) -> bool {
        let output = Command::new("zellij")
            .args(["list-sessions", "--short"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        output.lines().any(|line| {
            let line = line.trim();
            line == self.name || line.starts_with(&format!("{} ", self.name))
        })
    }

    pub fn capture_pane(&self, lines: usize) -> Result<String> {
        let temp_file = self.temp_file_path();

        let output = Command::new("zellij")
            .args(["action", "dump-screen", &temp_file, "--full"])
            .output()
            .context("Failed to dump zellij screen")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to capture pane: {}", stderr);
        }

        let content = fs::read_to_string(&temp_file).context("Failed to read captured content")?;

        let _ = fs::remove_file(&temp_file);

        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(lines);
        Ok(all_lines[start..].join("\n"))
    }

    pub fn capture_pane_full(&self) -> Result<String> {
        let temp_file = self.temp_file_path();

        let output = Command::new("zellij")
            .args(["action", "dump-screen", &temp_file, "--full"])
            .output()
            .context("Failed to dump zellij screen")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to capture pane: {}", stderr);
        }

        let content = fs::read_to_string(&temp_file).context("Failed to read captured content")?;

        let _ = fs::remove_file(&temp_file);

        Ok(content)
    }

    pub fn send_keys_raw(&self, keys: &str) -> Result<()> {
        let output = Command::new("zellij")
            .args(["action", "WriteChars", keys])
            .output()
            .context("Failed to write to zellij")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to write: {}", stderr);
        }
        Ok(())
    }

    pub fn send_keys(&self, keys: &str) -> Result<()> {
        self.send_keys_raw(&format!("{}\n", keys))
    }

    pub fn send_ctrl_c(&self) -> Result<()> {
        self.send_keys_raw("\x03")
    }

    pub fn attach(&self) -> Result<()> {
        let status = Command::new("zellij")
            .args(["attach", &self.name])
            .status()
            .context("Failed to attach to zellij session")?;

        if !status.success() {
            anyhow::bail!("Zellij attach exited with error");
        }
        Ok(())
    }

    pub fn kill(&self) -> Result<()> {
        let output = Command::new("zellij")
            .args(["kill-session", &self.name])
            .output()
            .context("Failed to kill zellij session")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("not found")
                && !stderr.contains("No such session")
                && !stderr.is_empty()
            {
                anyhow::bail!("Failed to kill session: {}", stderr);
            }
        }
        Ok(())
    }

    pub fn pane_current_command(&self) -> Option<String> {
        let content = self.capture_pane(20).ok()?;

        if content.contains("claude") || content.contains("Claude") {
            return Some("node".to_string());
        }
        if content.contains("opencode") || content.contains("OpenCode") {
            return Some("node".to_string());
        }
        if content.contains("codex") || content.contains("Codex") {
            return Some("codex".to_string());
        }
        if content.contains("gemini") || content.contains("Gemini") {
            return Some("gemini".to_string());
        }

        Some("bash".to_string())
    }

    pub fn pane_size(&self) -> Result<(u16, u16)> {
        Ok((120, 40))
    }

    fn temp_file_path(&self) -> String {
        format!("{}/grove-capture-{}.txt", TEMP_DIR, self.name)
    }
}

pub fn is_zellij_available() -> bool {
    Command::new("zellij")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn list_grove_sessions() -> Result<Vec<String>> {
    let output = Command::new("zellij")
        .args(["list-sessions", "--short"])
        .output()
        .context("Failed to list zellij sessions")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter_map(|line| {
            let name = line.split_whitespace().next()?;
            if name.starts_with("grove-") {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect())
}
