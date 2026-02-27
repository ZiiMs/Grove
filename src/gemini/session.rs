use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct ProjectsJson {
    #[serde(rename = "projects")]
    projects: std::collections::HashMap<String, String>,
}

fn get_gemini_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".gemini"))
        .unwrap_or_else(|| PathBuf::from("~/.gemini"))
}

fn get_projects_json_path() -> PathBuf {
    get_gemini_dir().join("projects.json")
}

pub fn find_session_by_directory(worktree_path: &str) -> Result<Option<String>> {
    let projects_path = get_projects_json_path();

    if !projects_path.exists() {
        tracing::debug!("Gemini projects.json not found at {:?}", projects_path);
        return Ok(None);
    }

    let content =
        std::fs::read_to_string(&projects_path).context("Failed to read Gemini projects.json")?;

    let projects: ProjectsJson =
        serde_json::from_str(&content).context("Failed to parse Gemini projects.json")?;

    let project_name = projects.projects.get(worktree_path);

    let project_name = match project_name {
        Some(name) => name,
        None => {
            tracing::debug!("No Gemini project found for path: {}", worktree_path);
            return Ok(None);
        }
    };

    let session_id = find_session_in_directory(worktree_path, project_name)?;

    Ok(session_id)
}

fn find_session_in_directory(worktree_path: &str, _project_name: &str) -> Result<Option<String>> {
    let output = Command::new("gemini")
        .args(["--list-sessions"])
        .current_dir(worktree_path)
        .output()
        .context("Failed to run 'gemini --list-sessions'")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("gemini --list-sessions failed: {}", stderr);
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();

    if trimmed.is_empty() {
        return Ok(None);
    }

    let session_id = parse_session_list(trimmed)?;

    Ok(session_id)
}

fn parse_session_list(output: &str) -> Result<Option<String>> {
    let re = Regex::new(r"^\s*(\d+)\.\s+.+?\s+\[([0-9a-f-]+)\]\s*$")?;

    let mut latest_index: u32 = 0;
    let mut latest_session: Option<String> = None;

    for line in output.lines() {
        if let Some(caps) = re.captures(line) {
            let index: u32 = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
            let session_id = caps.get(2).unwrap().as_str().to_string();

            if index > latest_index {
                latest_index = index;
                latest_session = Some(session_id);
            }
        }
    }

    Ok(latest_session)
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
    fn test_build_resume_command_with_session() {
        assert_eq!(
            build_resume_command("gemini", Some("8cfa2711-514a-4197-ac0e-df46c9fee46f")),
            "gemini --resume 8cfa2711-514a-4197-ac0e-df46c9fee46f"
        );
    }

    #[test]
    fn test_build_resume_command_without_session() {
        assert_eq!(build_resume_command("gemini", None), "gemini");
    }

    #[test]
    fn test_parse_session_list() {
        let output = r#"Available sessions for this project (1):
  1. Generate 200 lorem ipsum paragraphs into a text file. (Just now) [8cfa2711-514a-4197-ac0e-df46c9fee46f]"#;

        let result = parse_session_list(output).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "8cfa2711-514a-4197-ac0e-df46c9fee46f");
    }

    #[test]
    fn test_parse_session_list_multiple() {
        let output = r#"Available sessions for this project (2):
  1. First session (2 hours ago) [aaa111]
  2. Second session (Just now) [bbb222]"#;

        let result = parse_session_list(output).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "bbb222");
    }

    #[test]
    fn test_parse_session_list_empty() {
        let result = parse_session_list("").unwrap();
        assert!(result.is_none());
    }
}
