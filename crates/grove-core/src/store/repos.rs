//! Session, event and settings queries. Every read reconstructs the same DTOs the wire carries, so the
//! store and the IPC contract can never drift apart silently.

use super::{Store, StoreError};
use crate::ipc::{Session, SessionDetail, SessionSummary};
use crate::sessions::event::{now, EventKind, HarnessEvent, SessionState};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::BTreeMap;

const SESSION_COLUMNS: &str =
    "s.id, s.harness, s.cwd, s.model, s.title, s.status, s.created_at, s.updated_at";

fn row_to_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        harness: row.get(1)?,
        cwd: row.get(2)?,
        model: row.get(3)?,
        title: row.get(4)?,
        status: SessionState::parse(&row.get::<_, String>(5)?),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

impl Store {
    pub fn insert_session(
        &self,
        session: &Session,
        adapter_config: &serde_json::Value,
    ) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO sessions
                     (id, harness, cwd, model, title, status, adapter_config, resume_token, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9)",
                params![
                    &session.id,
                    &session.harness,
                    &session.cwd,
                    session.model.as_deref(),
                    &session.title,
                    session.status.as_str(),
                    adapter_config.to_string(),
                    &session.created_at,
                    &session.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn session(&self, id: &str) -> Result<Option<Session>, StoreError> {
        self.with_conn(|conn| {
            let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions s WHERE s.id = ?1");
            Ok(conn
                .query_row(&sql, params![id], row_to_session)
                .optional()?)
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>, StoreError> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {SESSION_COLUMNS},
                        (SELECT COUNT(*) FROM events e WHERE e.session_id = s.id) AS event_count
                 FROM sessions s
                 ORDER BY s.updated_at DESC, s.created_at DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], |row| {
                let event_count: i64 = row.get(8)?;
                Ok(SessionSummary {
                    session: row_to_session(row)?,
                    event_count: event_count.max(0) as u64,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn session_detail(&self, id: &str) -> Result<Option<SessionDetail>, StoreError> {
        let Some(session) = self.session(id)? else {
            return Ok(None);
        };
        Ok(Some(SessionDetail {
            session,
            events: self.session_events(id)?,
        }))
    }

    /// The transcript in sequence order. Sequence numbers are assigned by SQLite on append, so
    /// "ordered by seq" is the insertion order the pump persisted.
    pub fn session_events(&self, session_id: &str) -> Result<Vec<HarnessEvent>, StoreError> {
        let rows: Vec<(String, String)> = self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT at, payload FROM events WHERE session_id = ?1 ORDER BY seq")?;
            let rows = stmt.query_map(params![session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })?;

        rows.into_iter()
            .map(|(at, payload)| {
                let kind: EventKind = serde_json::from_str(&payload)?;
                Ok(HarnessEvent { at, kind })
            })
            .collect()
    }

    /// Append one event and return its sequence number. `payload` is the full `EventKind` JSON; `kind`
    /// is the tag, duplicated in its own column for cheap queries.
    pub fn append_event(&self, session_id: &str, event: &HarnessEvent) -> Result<i64, StoreError> {
        let payload = serde_json::to_string(&event.kind)?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO events (session_id, at, kind, payload) VALUES (?1, ?2, ?3, ?4)",
                params![session_id, &event.at, event.kind.tag(), payload],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn event_count(&self, session_id: &str) -> Result<u64, StoreError> {
        self.with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM events WHERE session_id = ?1",
                params![session_id],
                |row| row.get::<_, i64>(0),
            )? as u64)
        })
    }

    pub fn update_session_status(&self, id: &str, status: SessionState) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET status = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, status.as_str(), now()],
            )?;
            Ok(())
        })
    }

    pub fn set_resume_token(&self, id: &str, token: Option<&str>) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET resume_token = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, token, now()],
            )?;
            Ok(())
        })
    }

    pub fn resume_token(&self, id: &str) -> Result<Option<String>, StoreError> {
        self.with_conn(|conn| {
            let token: Option<Option<String>> = conn
                .query_row(
                    "SELECT resume_token FROM sessions WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(token.flatten())
        })
    }

    pub fn adapter_config(&self, id: &str) -> Result<serde_json::Value, StoreError> {
        let raw: Option<String> = self.with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT adapter_config FROM sessions WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?)
        })?;
        Ok(raw
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_else(|| serde_json::json!({})))
    }

    /// After a restart nothing is running: sessions are rows, not processes. Turn every live-looking
    /// status into `exited` so the UI offers Resume instead of pretending a process exists.
    pub fn mark_orphaned_sessions_exited(&self) -> Result<usize, StoreError> {
        self.with_conn(|conn| {
            Ok(conn.execute(
                "UPDATE sessions SET status = 'exited'
                 WHERE status IN ('starting', 'ready', 'busy')",
                [],
            )?)
        })
    }

    /// Every settings row as JSON, keyed by its name. Storing one JSON document per key is what lets
    /// an unknown key survive a round-trip: this build does not have to understand it to keep it.
    pub fn settings(&self) -> Result<BTreeMap<String, serde_json::Value>, StoreError> {
        self.with_conn(read_settings)
    }

    /// Read, modify and write every settings row in one transaction. `update` returns the value the
    /// caller wants; the rows it leaves behind are what gets committed, so a patch is all-or-nothing
    /// and a validation failure writes nothing.
    pub fn update_settings<T>(
        &self,
        update: impl FnOnce(&mut BTreeMap<String, serde_json::Value>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut rows = read_settings(conn)?;
            let outcome = update(&mut rows)?;
            {
                let mut statement = tx.prepare(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                )?;
                for (key, value) in &rows {
                    statement.execute(params![key, value.to_string()])?;
                }
            }
            tx.commit()?;
            Ok(outcome)
        })
    }
}

fn read_settings(conn: &Connection) -> Result<BTreeMap<String, serde_json::Value>, StoreError> {
    let mut statement = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut settings = BTreeMap::new();
    for row in rows {
        let (key, raw) = row?;
        settings.insert(key, serde_json::from_str(&raw)?);
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::event::{Block, Role};

    fn session(id: &str) -> Session {
        Session {
            id: id.into(),
            harness: "mock".into(),
            cwd: "C:/projects/grove".into(),
            model: Some("mock-1".into()),
            title: "Refactor the store".into(),
            status: SessionState::Starting,
            created_at: "2025-01-01T00:00:00.000Z".into(),
            updated_at: "2025-01-01T00:00:00.000Z".into(),
        }
    }

    fn message(text: &str) -> HarnessEvent {
        HarnessEvent::at(
            EventKind::Message {
                role: Role::User,
                blocks: vec![Block::Text { text: text.into() }],
            },
            "2025-01-01T00:00:01.000Z",
        )
    }

    #[test]
    fn events_get_increasing_sequence_numbers_and_read_back_in_order() {
        let store = Store::open_in_memory().unwrap();
        let row = session("session-1");
        store.insert_session(&row, &serde_json::json!({})).unwrap();

        assert_eq!(store.event_count("session-1").unwrap(), 0);
        assert_eq!(store.append_event("session-1", &message("one")).unwrap(), 1);
        assert_eq!(store.append_event("session-1", &message("two")).unwrap(), 2);
        assert_eq!(
            store.append_event("session-1", &message("three")).unwrap(),
            3
        );
        assert_eq!(store.event_count("session-1").unwrap(), 3);

        let events = store.session_events("session-1").unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], message("one"));
        assert_eq!(events[2], message("three"));
    }

    #[test]
    fn list_and_detail_expose_the_same_row() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_session(
                &session("session-1"),
                &serde_json::json!({"chunkDelayMs": 1}),
            )
            .unwrap();
        store.append_event("session-1", &message("hi")).unwrap();

        let summaries = store.list_sessions().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].session, session("session-1"));
        assert_eq!(summaries[0].event_count, 1);

        let detail = store.session_detail("session-1").unwrap().unwrap();
        assert_eq!(detail.session, session("session-1"));
        assert_eq!(detail.events, vec![message("hi")]);

        assert!(store.session("missing").unwrap().is_none());
        assert!(store.session_detail("missing").unwrap().is_none());
        assert_eq!(
            store.adapter_config("session-1").unwrap(),
            serde_json::json!({"chunkDelayMs": 1})
        );
    }

    #[test]
    fn status_and_resume_token_round_trip() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_session(&session("session-1"), &serde_json::json!({}))
            .unwrap();
        assert_eq!(store.resume_token("session-1").unwrap(), None);

        store
            .set_resume_token("session-1", Some("token-1"))
            .unwrap();
        assert_eq!(
            store.resume_token("session-1").unwrap().as_deref(),
            Some("token-1")
        );

        store
            .update_session_status("session-1", SessionState::Idle)
            .unwrap();
        assert_eq!(
            store.session("session-1").unwrap().unwrap().status,
            SessionState::Idle
        );
    }

    #[test]
    fn orphaned_sessions_become_exited_but_terminal_ones_stay() {
        let store = Store::open_in_memory().unwrap();
        for (id, status) in [
            ("a", SessionState::Starting),
            ("b", SessionState::Ready),
            ("c", SessionState::Busy),
            ("d", SessionState::Idle),
            ("e", SessionState::Error),
        ] {
            let mut row = session(id);
            row.status = status;
            store.insert_session(&row, &serde_json::json!({})).unwrap();
        }

        assert_eq!(store.mark_orphaned_sessions_exited().unwrap(), 3);
        assert_eq!(
            store.session("a").unwrap().unwrap().status,
            SessionState::Exited
        );
        assert_eq!(
            store.session("c").unwrap().unwrap().status,
            SessionState::Exited
        );
        assert_eq!(
            store.session("d").unwrap().unwrap().status,
            SessionState::Idle
        );
        assert_eq!(
            store.session("e").unwrap().unwrap().status,
            SessionState::Error
        );
    }
}
