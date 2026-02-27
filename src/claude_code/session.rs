use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

fn get_history_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".claude").join("history.jsonl"))
        .unwrap_or_else(|| PathBuf::from("~/.claude/history.jsonl"))
}

pub fn find_session_by_directory(worktree_path: &str) -> Result<Option<String>> {
    let history_path = get_history_path();

    if !history_path.exists() {
        tracing::debug!("Claude history not found at {:?}", history_path);
        return Ok(None);
    }

    let file = File::open(&history_path).context("Failed to open Claude history file")?;
    let reader = BufReader::new(file);

    let mut latest: Option<(u64, String)> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.is_empty() {
            continue;
        }

        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if entry.get("project").and_then(|p| p.as_str()) == Some(worktree_path) {
            if let (Some(session_id), Some(timestamp)) = (
                entry.get("sessionId").and_then(|s| s.as_str()),
                entry.get("timestamp").and_then(|t| t.as_u64()),
            ) {
                if latest.as_ref().is_none_or(|(ts, _)| timestamp > *ts) {
                    latest = Some((timestamp, session_id.to_string()));
                }
            }
        }
    }

    Ok(latest.map(|(_, id)| id))
}

pub fn build_resume_command(base_cmd: &str, session_id: Option<&str>) -> String {
    match session_id {
        Some(id) => format!("{} --resume {}", base_cmd, id),
        None => base_cmd.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_resume_command() {
        assert_eq!(
            build_resume_command("claude", Some("abc123")),
            "claude --resume abc123"
        );
        assert_eq!(build_resume_command("claude", None), "claude");
    }
}
