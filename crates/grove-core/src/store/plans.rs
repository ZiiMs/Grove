//! Plan rows: the SQLite record for one session's TODO plan.
//!
//! The mirror file in the project is derived from these rows, never the other way round. Every write
//! is one conditional statement, so the version check and the content change can never be split by
//! another writer: `expected_version` 0 means "there is no row yet" and inserts version 1; otherwise
//! the row only moves when its version still matches.
//!
//! `mirror_hash` is the SHA-256 of the content the row last wrote to `.grove/sessions/<id>/plan.md`.
//! It is what makes an external edit distinguishable from Grove's own mirror write.

use super::{Store, StoreError};
use crate::sessions::event::now;
use rusqlite::{params, OptionalExtension};

/// The record for one session's plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRow {
    pub markdown: String,
    pub version: u64,
    /// `None` only for a row whose mirror write never completed.
    pub mirror_hash: Option<String>,
    pub updated_at: String,
}

impl Store {
    pub fn plan_row(&self, session_id: &str) -> Result<Option<PlanRow>, StoreError> {
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT markdown, version, mirror_hash, updated_at
                     FROM plans WHERE session_id = ?1",
                    params![session_id],
                    |row| {
                        Ok(PlanRow {
                            markdown: row.get(0)?,
                            version: row.get::<_, i64>(1)?.max(0) as u64,
                            mirror_hash: row.get(2)?,
                            updated_at: row.get(3)?,
                        })
                    },
                )
                .optional()?;
            Ok(row)
        })
    }

    /// Compare-and-set: write `markdown` only when the row is still at `expected_version` (0 = no row
    /// yet). Returns whether the write was applied; `false` means another writer got there first and
    /// the caller must report a conflict.
    pub fn write_plan(
        &self,
        session_id: &str,
        markdown: &str,
        expected_version: u64,
        mirror_hash: &str,
    ) -> Result<bool, StoreError> {
        let updated_at = now();
        self.with_conn(|conn| {
            let changed = if expected_version == 0 {
                conn.execute(
                    "INSERT INTO plans (session_id, markdown, version, mirror_hash, updated_at)
                     VALUES (?1, ?2, 1, ?3, ?4)
                     ON CONFLICT(session_id) DO NOTHING",
                    params![session_id, markdown, mirror_hash, updated_at],
                )?
            } else {
                conn.execute(
                    "UPDATE plans
                     SET markdown = ?2, version = version + 1, mirror_hash = ?3, updated_at = ?4
                     WHERE session_id = ?1 AND version = ?5",
                    params![
                        session_id,
                        markdown,
                        mirror_hash,
                        updated_at,
                        expected_version as i64
                    ],
                )?
            };
            Ok(changed > 0)
        })
    }

    /// Record the hash of a mirror that was rewritten from the row (a missing file), without moving
    /// the version or the content. Guarded by `version` so a concurrent bump is never mislabelled.
    pub fn record_mirror_hash(
        &self,
        session_id: &str,
        version: u64,
        mirror_hash: &str,
    ) -> Result<bool, StoreError> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                "UPDATE plans SET mirror_hash = ?3
                 WHERE session_id = ?1 AND version = ?2",
                params![session_id, version as i64, mirror_hash],
            )?;
            Ok(changed > 0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::Session;
    use crate::sessions::event::SessionState;
    use crate::sessions::event::TitleSource;

    fn store_with_session() -> Store {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_session(
                &Session {
                    id: "session-1".into(),
                    provider: "mock".into(),
                    cwd: "C:/projects/grove".into(),
                    model: None,
                    title: "Plan".into(),
                    status: SessionState::Idle,
                    project_id: None,
                    awaiting: false,
                    archived_at: None,
                    title_source: TitleSource::Manual,
                    created_at: "2025-01-01T00:00:00.000Z".into(),
                    updated_at: "2025-01-01T00:00:00.000Z".into(),
                },
                &serde_json::json!({}),
            )
            .unwrap();
        store
    }

    #[test]
    fn the_first_write_inserts_version_one_and_the_next_write_moves_it() {
        let store = store_with_session();
        assert_eq!(store.plan_row("session-1").unwrap(), None);

        assert!(store
            .write_plan("session-1", "one\n", 0, "hash-one")
            .unwrap());
        let row = store.plan_row("session-1").unwrap().unwrap();
        assert_eq!(row.markdown, "one\n");
        assert_eq!(row.version, 1);
        assert_eq!(row.mirror_hash.as_deref(), Some("hash-one"));

        assert!(store
            .write_plan("session-1", "two\n", 1, "hash-two")
            .unwrap());
        let row = store.plan_row("session-1").unwrap().unwrap();
        assert_eq!(row.markdown, "two\n");
        assert_eq!(row.version, 2);
    }

    #[test]
    fn a_stale_version_is_refused_and_changes_nothing() {
        let store = store_with_session();
        store
            .write_plan("session-1", "one\n", 0, "hash-one")
            .unwrap();

        assert!(!store
            .write_plan("session-1", "stale\n", 0, "hash-stale")
            .unwrap());
        assert!(!store
            .write_plan("session-1", "stale\n", 7, "hash-stale")
            .unwrap());

        let row = store.plan_row("session-1").unwrap().unwrap();
        assert_eq!(row.markdown, "one\n");
        assert_eq!(row.version, 1);
        assert_eq!(row.mirror_hash.as_deref(), Some("hash-one"));
    }

    #[test]
    fn the_mirror_hash_can_be_repaired_without_moving_the_version() {
        let store = store_with_session();
        store
            .write_plan("session-1", "one\n", 0, "hash-one")
            .unwrap();

        assert!(!store.record_mirror_hash("session-1", 9, "wrong").unwrap());
        assert!(store
            .record_mirror_hash("session-1", 1, "repaired")
            .unwrap());
        let row = store.plan_row("session-1").unwrap().unwrap();
        assert_eq!(row.version, 1);
        assert_eq!(row.mirror_hash.as_deref(), Some("repaired"));
    }

    #[test]
    fn a_plan_needs_its_session() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.write_plan("missing", "x", 0, "hash").is_err());
    }
}
