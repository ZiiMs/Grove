use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;

const CODEX_SQLITE_HOME_ENV: &str = "CODEX_SQLITE_HOME";
const STATE_DB_FILENAME: &str = "state";

fn get_state_db_path() -> PathBuf {
    if let Ok(custom_home) = std::env::var(CODEX_SQLITE_HOME_ENV) {
        PathBuf::from(custom_home).join(STATE_DB_FILENAME)
    } else {
        dirs::home_dir()
            .map(|h| h.join(".codex").join(STATE_DB_FILENAME))
            .unwrap_or_else(|| PathBuf::from("~/.codex/state"))
    }
}

fn normalize_path_for_comparison(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.ends_with('/') {
        normalized
    } else {
        format!("{}/", normalized)
    }
}

pub fn find_session_by_directory(worktree_path: &str) -> Result<Option<String>> {
    let db_path = get_state_db_path();

    if !db_path.exists() {
        tracing::debug!("Codex state database not found at {:?}", db_path);
        return Ok(None);
    }

    let conn = Connection::open(&db_path)
        .with_context(|| format!("Failed to open Codex state database at {:?}", db_path))?;

    let normalized_path = normalize_path_for_comparison(worktree_path);

    let query = r#"
        SELECT id FROM threads 
        WHERE cwd = ?1 OR cwd = ?2 OR cwd LIKE ?3
        ORDER BY updated_at DESC 
        LIMIT 1
    "#;

    let path_exact = worktree_path.trim_end_matches('/');
    let path_normalized = normalized_path.trim_end_matches('/');
    let path_like = format!("{}%", path_normalized);

    let result = conn.query_row(
        query,
        rusqlite::params![path_exact, path_normalized, path_like],
        |row| row.get::<_, String>(0),
    );

    match result {
        Ok(session_id) => Ok(Some(session_id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => {
            tracing::warn!("Failed to query Codex state database: {}", e);
            Ok(None)
        }
    }
}

pub fn build_resume_command(base_cmd: &str, session_id: Option<&str>) -> String {
    match session_id {
        Some(id) => format!("{} resume {}", base_cmd, id),
        None => format!("{} resume --last", base_cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_resume_command_with_session() {
        assert_eq!(
            build_resume_command("codex", Some("abc123-def456")),
            "codex resume abc123-def456"
        );
    }

    #[test]
    fn test_build_resume_command_without_session() {
        assert_eq!(build_resume_command("codex", None), "codex resume --last");
    }

    #[test]
    fn test_normalize_path_for_comparison() {
        assert_eq!(
            normalize_path_for_comparison("/home/user/project"),
            "/home/user/project/"
        );
        assert_eq!(
            normalize_path_for_comparison("/home/user/project/"),
            "/home/user/project/"
        );
        assert_eq!(
            normalize_path_for_comparison("C:\\Users\\project"),
            "C:/Users/project/"
        );
    }
}
