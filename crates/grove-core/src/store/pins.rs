//! Pins: which specs a session was told to carry (see `architecture.md` §Persistence).
//!
//! A pin is a durable fact about a *session*, not about the spec graph: the graph is re-derived from
//! files on every read, and a pinned spec that later disappears simply stops contributing a document.
//! That is why the table stores the spec id (the stable handle across renames of the file) and why
//! deleting a session takes its pins with it through the foreign key.

use super::{Store, StoreError};
use rusqlite::params;

impl Store {
    /// Pin a spec to a session. Idempotent: pinning twice is one row and one `true` answer.
    pub fn add_pin(&self, session_id: &str, spec_id: &str) -> Result<bool, StoreError> {
        if spec_id.trim().is_empty() {
            return Err(StoreError::Invalid("a pin needs a spec id".into()));
        }
        self.with_conn(|conn| {
            let inserted = conn.execute(
                "INSERT INTO pins (session_id, spec_id) VALUES (?1, ?2)
                 ON CONFLICT(session_id, spec_id) DO NOTHING",
                params![session_id, spec_id],
            )?;
            Ok(inserted > 0)
        })
    }

    /// Unpin a spec. Returns whether a pin was actually removed.
    pub fn remove_pin(&self, session_id: &str, spec_id: &str) -> Result<bool, StoreError> {
        self.with_conn(|conn| {
            let removed = conn.execute(
                "DELETE FROM pins WHERE session_id = ?1 AND spec_id = ?2",
                params![session_id, spec_id],
            )?;
            Ok(removed > 0)
        })
    }

    /// Every spec id pinned to the session, in the order the user pinned them.
    pub fn pins(&self, session_id: &str) -> Result<Vec<String>, StoreError> {
        self.with_conn(|conn| {
            let mut statement =
                conn.prepare("SELECT spec_id FROM pins WHERE session_id = ?1 ORDER BY rowid")?;
            let rows = statement.query_map(params![session_id], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::Session;
    use crate::sessions::event::SessionState;

    fn session(id: &str) -> Session {
        Session {
            id: id.into(),
            provider: "mock".into(),
            cwd: "C:/projects/grove".into(),
            model: None,
            title: "Pinned specs".into(),
            status: SessionState::Idle,
            project_id: None,
            awaiting: false,
            archived_at: None,
            created_at: "2025-01-01T00:00:00.000Z".into(),
            updated_at: "2025-01-01T00:00:00.000Z".into(),
        }
    }

    fn store_with_session() -> Store {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_session(&session("session-1"), &serde_json::json!({}))
            .unwrap();
        store
    }

    #[test]
    fn pins_round_trip_in_insertion_order_and_are_idempotent() {
        let store = store_with_session();
        assert!(store.pins("session-1").unwrap().is_empty());

        assert!(store.add_pin("session-1", "grove-architecture").unwrap());
        assert!(store.add_pin("session-1", "grove-goal").unwrap());
        assert!(
            !store.add_pin("session-1", "grove-architecture").unwrap(),
            "pinning the same spec twice changes nothing"
        );

        assert_eq!(
            store.pins("session-1").unwrap(),
            vec!["grove-architecture".to_string(), "grove-goal".to_string()]
        );

        assert!(store.remove_pin("session-1", "grove-architecture").unwrap());
        assert!(
            !store.remove_pin("session-1", "grove-architecture").unwrap(),
            "removing a pin that is gone reports it"
        );
        assert_eq!(
            store.pins("session-1").unwrap(),
            vec!["grove-goal".to_string()]
        );
    }

    #[test]
    fn pins_belong_to_one_session_and_survive_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grove.db");
        {
            let store = Store::open(&path).unwrap();
            store
                .insert_session(&session("session-1"), &serde_json::json!({}))
                .unwrap();
            store
                .insert_session(&session("session-2"), &serde_json::json!({}))
                .unwrap();
            store.add_pin("session-1", "grove-architecture").unwrap();
            store.add_pin("session-2", "grove-goal").unwrap();
        }

        let reopened = Store::open(&path).unwrap();
        assert_eq!(
            reopened.pins("session-1").unwrap(),
            vec!["grove-architecture".to_string()]
        );
        assert_eq!(
            reopened.pins("session-2").unwrap(),
            vec!["grove-goal".to_string()],
            "a restart rewrites no pins"
        );
        assert!(reopened.pins("missing").unwrap().is_empty());
    }

    #[test]
    fn pins_cannot_be_attached_to_a_session_that_does_not_exist() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.add_pin("missing", "grove-architecture").is_err());
    }
}
