//! Workspace projects: the folders the user added, and linking existing sessions to them.
//!
//! A project is a folder plus the git facts we detected when it was added. Worktrees are not stored:
//! git is the source of truth for a checkout, so the UI asks git each time and groups sessions by
//! their `cwd`. Sessions keep a `project_id`, which is what makes a loose chat distinguishable from a
//! session that belongs to a project even when both run in the app directory.

use super::{Store, StoreError};
use crate::ipc::Project;
use rusqlite::{params, OptionalExtension, Row};

const PROJECT_COLUMNS: &str =
    "id, name, path, remote, default_branch, is_git, created_at, updated_at";

fn row_to_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        remote: row.get(3)?,
        default_branch: row.get(4)?,
        is_git: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

impl Store {
    pub fn insert_project(&self, project: &Project) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO projects
                     (id, name, path, remote, default_branch, is_git, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &project.id,
                    &project.name,
                    &project.path,
                    project.remote.as_deref(),
                    project.default_branch.as_deref(),
                    i64::from(project.is_git),
                    &project.created_at,
                    &project.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_projects(&self) -> Result<Vec<Project>, StoreError> {
        self.with_conn(|conn| {
            let sql =
                format!("SELECT {PROJECT_COLUMNS} FROM projects ORDER BY name COLLATE NOCASE");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], row_to_project)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn project(&self, id: &str) -> Result<Option<Project>, StoreError> {
        self.with_conn(|conn| {
            let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1");
            Ok(conn
                .query_row(&sql, params![id], row_to_project)
                .optional()?)
        })
    }

    /// True when a name is already taken. The UI validates before inserting; this keeps the message
    /// user-facing instead of surfacing a raw SQLite constraint error.
    pub fn project_name_taken(&self, name: &str) -> Result<bool, StoreError> {
        self.with_conn(|conn| {
            let found: Option<String> = conn
                .query_row(
                    "SELECT id FROM projects WHERE name = ?1 COLLATE NOCASE",
                    params![name],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(found.is_some())
        })
    }

    pub fn project_path_taken(&self, path: &str) -> Result<bool, StoreError> {
        self.with_conn(|conn| {
            let found: Option<String> = conn
                .query_row(
                    "SELECT id FROM projects WHERE path = ?1 COLLATE NOCASE",
                    params![path],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(found.is_some())
        })
    }

    /// Attach every loose session whose cwd is one of `paths` (or sits under one) to a project.
    ///
    /// This is what makes a newly added project immediately show the conversations that already ran
    /// in it, instead of leaving them in Chats forever.
    pub fn link_sessions_to_project(
        &self,
        project_id: &str,
        paths: &[String],
    ) -> Result<usize, StoreError> {
        self.with_conn(|conn| {
            let mut linked = 0;
            for path in paths {
                let path = path.trim().trim_end_matches(['/', '\\']);
                if path.is_empty() {
                    continue;
                }
                linked += conn.execute(
                    "UPDATE sessions SET project_id = ?1
                     WHERE project_id IS NULL
                       AND (cwd = ?2 OR cwd LIKE ?3 OR cwd LIKE ?4)",
                    params![project_id, path, format!("{path}/%"), format!("{path}\\%")],
                )?;
            }
            Ok(linked)
        })
    }

    /// The project a session's directory belongs to, if any. Used to label a session that predates
    /// `project_id` without writing to every row on startup.
    pub fn project_for_path(&self, path: &str) -> Result<Option<Project>, StoreError> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {PROJECT_COLUMNS} FROM projects
                 WHERE ?1 = path OR ?1 LIKE path || '/%' OR ?1 LIKE path || '\\%'
                 ORDER BY length(path) DESC
                 LIMIT 1"
            );
            Ok(conn
                .query_row(&sql, params![path], row_to_project)
                .optional()?)
        })
    }
}
