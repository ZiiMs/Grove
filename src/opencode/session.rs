use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub fn get_db_path() -> Result<PathBuf> {
    let output = Command::new("opencode")
        .args(["db", "path"])
        .output()
        .context("Failed to run 'opencode db path'")?;

    if !output.status.success() {
        anyhow::bail!("opencode db path command failed");
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path))
}

pub fn find_session_by_directory(worktree_path: &str) -> Result<Option<String>> {
    let db_path = get_db_path()?;

    if !db_path.exists() {
        tracing::debug!("OpenCode database not found at {:?}", db_path);
        return Ok(None);
    }

    let query = format!(
        "SELECT id FROM session WHERE directory = '{}' ORDER BY time_updated DESC LIMIT 1;",
        worktree_path.replace("'", "''")
    );

    let output = Command::new("opencode")
        .args(["db", &query, "--format", "json"])
        .output()
        .context("Failed to query opencode database")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("OpenCode DB query failed: {}", stderr);
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();

    if trimmed.is_empty() || trimmed == "[]" {
        return Ok(None);
    }

    let sessions: Vec<serde_json::Value> =
        serde_json::from_str(trimmed).context("Failed to parse opencode session query result")?;

    if let Some(first) = sessions.first() {
        if let Some(id) = first.get("id").and_then(|v| v.as_str()) {
            return Ok(Some(id.to_string()));
        }
    }

    Ok(None)
}

pub fn build_command_with_session(base_cmd: &str, session_id: Option<&str>) -> String {
    match session_id {
        Some(id) => format!("{} -s {}", base_cmd, id),
        None => base_cmd.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_command_with_session() {
        assert_eq!(
            build_command_with_session("opencode", Some("ses_123")),
            "opencode -s ses_123"
        );
        assert_eq!(build_command_with_session("opencode", None), "opencode");
    }
}
