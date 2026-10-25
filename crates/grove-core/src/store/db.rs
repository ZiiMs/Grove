//! Opening the database and applying migrations.
//!
//! Migrations are an ordered `&[&str]`: index + 1 is the version, each one runs exactly once inside a
//! transaction, and the applied set is recorded in `schema_migrations(version)`. Later domains append
//! a migration rather than editing an old one, so an existing install upgrades in place.

use crate::sessions::event::now;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

pub const MIGRATIONS: &[&str] = &[
    // 1 — sessions and their normalized event log (ticket 02).
    r#"
CREATE TABLE sessions (
    id             TEXT PRIMARY KEY,
    harness         TEXT NOT NULL,
    cwd            TEXT NOT NULL,
    model          TEXT,
    title          TEXT NOT NULL,
    status         TEXT NOT NULL,
    adapter_config TEXT NOT NULL DEFAULT '{}',
    resume_token   TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

CREATE TABLE events (
    seq        INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    at         TEXT NOT NULL,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL
);

CREATE INDEX events_by_session ON events(session_id, seq);
"#,
    // 2 — the session TODO plan (ticket 09). `mirror_hash` is the content the row last wrote to
    // `.grove/sessions/<id>/plan.md`, which is how an agent's external edit is told apart from
    // Grove's own mirror write. See `architecture.md` §Persistence → plan dual-edit.
    r#"
CREATE TABLE plans (
    session_id  TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    markdown    TEXT NOT NULL,
    version     INTEGER NOT NULL,
    mirror_hash TEXT,
    updated_at  TEXT NOT NULL
);
"#,
    // 3 — typed settings documents (ticket 07). Values are JSON; secrets never live here.
    r#"
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#,
    // 4 — specs pinned to a session (ticket 14). The specs themselves stay files: the table holds
    // the stable spec id, so a pin survives a restart and a deleted spec simply stops contributing a
    // context document. Rows keep insertion order through `rowid`, which is the order the user pinned
    // them in. See `architecture.md` §Persistence → pins.
    r#"
CREATE TABLE pins (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    spec_id    TEXT NOT NULL,
    PRIMARY KEY (session_id, spec_id)
);
"#,
    // 5 — workspace projects and the two session facts the new shell reads directly: which project a
    // conversation belongs to (loose chats keep `NULL`), and whether a permission request is waiting.
    // `awaiting` is mirrored by the pump, so the tree and titlebar need no transcript read.
    r#"
CREATE TABLE projects (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL UNIQUE,
    path           TEXT NOT NULL UNIQUE,
    remote         TEXT,
    default_branch TEXT,
    is_git         INTEGER NOT NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

ALTER TABLE sessions ADD COLUMN project_id TEXT REFERENCES projects(id) ON DELETE SET NULL;
ALTER TABLE sessions ADD COLUMN awaiting INTEGER NOT NULL DEFAULT 0;

CREATE INDEX sessions_by_project ON sessions(project_id);
"#,
    // 6 — providers replace providers. A session now names the vendor it talks to and Grove runs the
    // agent itself, so the column is renamed rather than reinterpreted: a leftover `claude` row is a
    // provider this build does not know, which the session picker already handles.
    r#"
ALTER TABLE sessions RENAME COLUMN harness TO provider;
"#,
];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// A caller-supplied value failed validation. Domains map this to the wire's `invalid` code.
    #[error("invalid: {0}")]
    Invalid(String),
}

/// The database. One connection behind a mutex: writes are tiny (one event at a time) and every
/// caller is either a command or a session pump, so contention is not worth a pool.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open (or create) the database file, create its parent directory, and migrate.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::prepare(Connection::open(path)?)
    }

    /// In-memory store, for previews and tests that do not care about the file.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(conn: Connection) -> Result<Self, StoreError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                 version    INTEGER PRIMARY KEY,
                 applied_at TEXT NOT NULL
             );",
        )?;

        let applied: Vec<i64> = {
            let mut stmt =
                conn.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        for (index, sql) in MIGRATIONS.iter().enumerate() {
            let version = index as i64 + 1;
            if applied.contains(&version) {
                continue;
            }
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                params![version, now()],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    /// The versions applied, oldest first. Read by tests and by any future "upgrade" UI.
    pub fn applied_migrations(&self) -> Result<Vec<i64>, StoreError> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    fn connection(&self) -> MutexGuard<'_, Connection> {
        // A poisoned mutex means a caller panicked mid-write; the SQLite handle itself is still
        // usable, so recover rather than taking the whole session runtime down with it.
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn with_conn<T>(
        &self,
        run: impl FnOnce(&Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let conn = self.connection();
        run(&conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_path_migrates_once_and_reopening_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("grove.db");

        {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.applied_migrations().unwrap(), vec![1, 2, 3, 4, 5, 6]);
        }
        assert!(path.exists(), "the database file is created on open");

        let reopened = Store::open(&path).unwrap();
        assert_eq!(
            reopened.applied_migrations().unwrap(),
            vec![1, 2, 3, 4, 5, 6],
            "reopening does not replay migrations"
        );
    }

    #[test]
    fn in_memory_store_migrates_too() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.applied_migrations().unwrap(), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn frozen_migrations_are_never_edited() {
        // The shipped DDL is history: a database in the field was built from exactly these bytes.
        // Editing one of them changes what a new install gets without ever changing what an existing
        // install has, which is how a rename quietly became a no-op and the app died on startup.
        // Adding migration 7 does not touch this assertion; rewriting 1..5 does.
        let frozen = include_str!("../../tests/fixtures/schema_v5.sql");
        let current: Vec<&str> = MIGRATIONS[..5].to_vec();
        assert_eq!(
            frozen.replace("\r\n", "\n"),
            current.join("\n-- ---- 8< ----\n"),
            "migrations 1..=5 are frozen; append a new one instead of editing them"
        );
    }

    #[test]
    fn migration_six_turns_a_legacy_harness_row_into_a_provider_row() {
        // The rename is the one migration that rewrites an existing column, so it is pinned by effect
        // on a database built from the frozen history, not from the current array: an install from
        // the harness era must keep its conversations, under the new column name.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grove.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
            )
            .unwrap();
            conn.execute_batch(include_str!("../../tests/fixtures/schema_v5.sql"))
                .unwrap();
            for version in 1..=5 {
                conn.execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                    rusqlite::params![version, "2026-10-24T00:00:00Z"],
                )
                .unwrap();
            }
            conn.execute(
                "INSERT INTO sessions (id, harness, cwd, title, status, created_at, updated_at)
                 VALUES ('legacy', 'claude', 'C:/work', 'From the harness era', 'idle',
                         '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z')",
                [],
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();
        assert_eq!(store.applied_migrations().unwrap(), vec![1, 2, 3, 4, 5, 6]);
        let session = store.session("legacy").unwrap().expect("the row survives");
        assert_eq!(
            session.provider, "claude",
            "the value moves with the column"
        );
    }

    #[test]
    fn an_existing_install_gains_only_the_new_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grove.db");

        // Stand in for a database created by an older build: apply migrations 1..=3 by hand, so the
        // reopen has to append version 4 without replaying (and failing on) the earlier ones.
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
            )
            .unwrap();
            for (index, sql) in MIGRATIONS[..3].iter().enumerate() {
                conn.execute_batch(sql).unwrap();
                conn.execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                    params![index as i64 + 1, now()],
                )
                .unwrap();
            }
        }

        let store = Store::open(&path).unwrap();
        assert_eq!(store.applied_migrations().unwrap(), vec![1, 2, 3, 4, 5, 6]);
        assert!(store.pins("session-1").unwrap().is_empty(), "pins exists");
    }
}
