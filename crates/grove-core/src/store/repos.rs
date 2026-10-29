//! Session, event and settings queries. Every read reconstructs the same DTOs the wire carries, so the
//! store and the IPC contract can never drift apart silently.

use super::{Store, StoreError};
use crate::ipc::{Session, SessionDetail, SessionSummary, SubagentSummary};
use crate::sessions::event::{now, EventKind, SessionEvent, SessionState};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::BTreeMap;

const SESSION_COLUMNS: &str =
    "s.id, s.provider, s.cwd, s.model, s.title, s.status, s.created_at, s.updated_at, s.project_id, s.awaiting, s.archived_at";

fn row_to_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        provider: row.get(1)?,
        cwd: row.get(2)?,
        model: row.get(3)?,
        title: row.get(4)?,
        status: SessionState::parse(&row.get::<_, String>(5)?),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        project_id: row.get(8)?,
        awaiting: row.get::<_, i64>(9)? != 0,
        archived_at: row.get(10)?,
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
                     (id, provider, cwd, model, title, status, adapter_config, resume_token, created_at, updated_at, project_id, awaiting, archived_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, ?12)",
                params![
                    &session.id,
                    &session.provider,
                    &session.cwd,
                    session.model.as_deref(),
                    &session.title,
                    session.status.as_str(),
                    adapter_config.to_string(),
                    &session.created_at,
                    &session.updated_at,
                    session.project_id.as_deref(),
                    i64::from(session.awaiting),
                    session.archived_at.as_deref(),
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
            // One query for every subagent, grouped in memory: the list is small, and this keeps the
            // session query from growing a join for a field most rows leave empty.
            let mut by_session: BTreeMap<String, Vec<SubagentSummary>> = BTreeMap::new();
            {
                let mut stmt = conn.prepare(
                    "SELECT session_id, call_id, description, status, summary, started_at, finished_at
                     FROM subagents ORDER BY started_at, rowid",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        SubagentSummary {
                            call_id: row.get(1)?,
                            description: row.get(2)?,
                            status: row.get(3)?,
                            summary: row.get(4)?,
                            started_at: row.get(5)?,
                            finished_at: row.get(6)?,
                        },
                    ))
                })?;
                for row in rows {
                    let (session_id, subagent) = row?;
                    by_session.entry(session_id).or_default().push(subagent);
                }
            }
            let sql = format!(
                "SELECT {SESSION_COLUMNS},
                        (SELECT COUNT(*) FROM events e WHERE e.session_id = s.id) AS event_count
                 FROM sessions s
                 ORDER BY s.updated_at DESC, s.created_at DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], |row| {
                let event_count: i64 = row.get(11)?;
                let session = row_to_session(row)?;
                let subagents = by_session.remove(&session.id).unwrap_or_default();
                Ok(SessionSummary {
                    session,
                    event_count: event_count.max(0) as u64,
                    subagents,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Record a delegated worker as it starts. A repeated call id (the same parent tool call replayed
    /// after a resume) resets the row instead of duplicating it.
    pub fn start_subagent(
        &self,
        session_id: &str,
        call_id: &str,
        description: &str,
        started_at: &str,
    ) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO subagents
                     (session_id, call_id, description, status, summary, started_at, finished_at)
                 VALUES (?1, ?2, ?3, 'running', NULL, ?4, NULL)
                 ON CONFLICT(session_id, call_id) DO UPDATE SET
                     description = excluded.description,
                     status = 'running',
                     summary = NULL,
                     started_at = excluded.started_at,
                     finished_at = NULL",
                params![session_id, call_id, description, started_at],
            )?;
            Ok(())
        })
    }

    /// Close the subagent row when the parent's tool result arrives. The status is the tool result's
    /// (`ok`, `error` or `cancelled`); a result for any other tool matches no row.
    pub fn finish_subagent(
        &self,
        session_id: &str,
        call_id: &str,
        status: &str,
        summary: &str,
        finished_at: &str,
    ) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE subagents SET status = ?3, summary = ?4, finished_at = ?5
                 WHERE session_id = ?1 AND call_id = ?2",
                params![session_id, call_id, status, summary, finished_at],
            )?;
            Ok(())
        })
    }

    pub fn session_detail(&self, id: &str) -> Result<Option<SessionDetail>, StoreError> {
        let Some(session) = self.session(id)? else {
            return Ok(None);
        };
        let thinking_level = self
            .adapter_config(id)?
            .get("thinkingLevel")
            .and_then(serde_json::Value::as_str)
            .map(crate::providers::ThinkingLevel::parse)
            .unwrap_or_default();
        Ok(Some(SessionDetail {
            session,
            thinking_level,
            events: self.session_events(id)?,
        }))
    }

    /// The transcript in sequence order. Sequence numbers are assigned by SQLite on append, so
    /// "ordered by seq" is the insertion order the pump persisted.
    pub fn session_events(&self, session_id: &str) -> Result<Vec<SessionEvent>, StoreError> {
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
                Ok(SessionEvent { at, kind })
            })
            .collect()
    }

    /// Append one event and return its sequence number. `payload` is the full `EventKind` JSON; `kind`
    /// is the tag, duplicated in its own column for cheap queries.
    pub fn append_event(&self, session_id: &str, event: &SessionEvent) -> Result<i64, StoreError> {
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

    /// Mirror the one bit the tree and titlebar read without loading a transcript.
    pub fn update_session_awaiting(&self, id: &str, awaiting: bool) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET awaiting = ?2 WHERE id = ?1",
                params![id, i64::from(awaiting)],
            )?;
            Ok(())
        })
    }

    /// Move a session between the active tree and the Archive section. `None` restores it; `Some`
    /// stamps when it was archived. Both touch `updated_at`, because moving is a change.
    pub fn set_session_archived(
        &self,
        id: &str,
        archived_at: Option<&str>,
    ) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET archived_at = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, archived_at, now()],
            )?;
            Ok(())
        })
    }

    /// Remove a session row. `events`, `plans`, `pins` and `subagents` cascade; the caller stops a
    /// live agent first. Returns whether a row was actually deleted.
    pub fn delete_session(&self, id: &str) -> Result<bool, StoreError> {
        self.with_conn(|conn| {
            let removed = conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
            Ok(removed > 0)
        })
    }

    /// Point an existing session at another provider. The model and the resume token belong to the
    /// provider that produced them, so a switch writes both through separate calls.
    pub fn set_session_provider(
        &self,
        id: &str,
        provider: &str,
        model: Option<&str>,
    ) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET provider = ?2, model = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, provider, model, now()],
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

    /// Replace the provider config stored with a session. Used when a preference that lives in the
    /// config (today, the thinking level) changes without changing the provider or model.
    pub fn set_adapter_config(
        &self,
        id: &str,
        config: &serde_json::Value,
    ) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET adapter_config = ?1, updated_at = ?2 WHERE id = ?3",
                params![config.to_string(), now(), id],
            )?;
            Ok(())
        })
    }

    /// Close any worker still marked running when its parent turn stops. A normal turn cannot leave
    /// one running (the parent waits for the worker), so this is a cancel or a failed turn.
    pub fn finish_running_subagents(
        &self,
        session_id: &str,
        status: &str,
        summary: &str,
        finished_at: &str,
    ) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE subagents SET status = ?2, summary = COALESCE(summary, ?3), finished_at = ?4
                 WHERE session_id = ?1 AND status = 'running'",
                params![session_id, status, summary, finished_at],
            )?;
            Ok(())
        })
    }

    /// After a restart nothing is running: sessions are rows, not processes. Turn every live-looking
    /// status into `exited` so the UI offers Resume instead of pretending a process exists. A worker
    /// that was mid-flight has no process either; close it as an error so the tree stops spinning.
    pub fn mark_orphaned_sessions_exited(&self) -> Result<usize, StoreError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE subagents
                 SET status = 'error',
                     summary = COALESCE(summary, 'the app closed before this worker finished'),
                     finished_at = ?1
                 WHERE status = 'running'",
                params![now()],
            )?;
            Ok(conn.execute(
                "UPDATE sessions SET status = 'exited', awaiting = 0
                 WHERE status IN ('starting', 'ready', 'busy', 'idle')",
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
            provider: "mock".into(),
            cwd: "C:/projects/grove".into(),
            model: Some("mock-1".into()),
            title: "Refactor the store".into(),
            status: SessionState::Starting,
            project_id: None,
            awaiting: false,
            archived_at: None,
            created_at: "2025-01-01T00:00:00.000Z".into(),
            updated_at: "2025-01-01T00:00:00.000Z".into(),
        }
    }

    fn message(text: &str) -> SessionEvent {
        SessionEvent::at(
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

        assert_eq!(store.mark_orphaned_sessions_exited().unwrap(), 4);
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
            SessionState::Exited
        );
        assert_eq!(
            store.session("e").unwrap().unwrap().status,
            SessionState::Error
        );
    }

    #[test]
    fn archiving_keeps_the_row_and_transcript_and_restore_clears_the_stamp() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_session(&session("session-1"), &serde_json::json!({}))
            .unwrap();
        store.append_event("session-1", &message("hi")).unwrap();

        store
            .set_session_archived("session-1", Some("2025-01-02T00:00:00.000Z"))
            .unwrap();
        let row = store.session("session-1").unwrap().unwrap();
        assert_eq!(row.archived_at.as_deref(), Some("2025-01-02T00:00:00.000Z"));
        assert_eq!(
            store.session_events("session-1").unwrap(),
            vec![message("hi")],
            "archiving never deletes the transcript"
        );
        assert_eq!(
            store.list_sessions().unwrap().len(),
            1,
            "an archived session is still a row in the list"
        );

        store.set_session_archived("session-1", None).unwrap();
        assert_eq!(
            store.session("session-1").unwrap().unwrap().archived_at,
            None
        );
    }

    #[test]
    fn deleting_a_session_cascades_events_plans_pins_and_subagents() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_session(&session("session-1"), &serde_json::json!({}))
            .unwrap();
        store.append_event("session-1", &message("hi")).unwrap();
        store
            .write_plan("session-1", "# Plan\n", 0, "hash")
            .unwrap();
        store.add_pin("session-1", "grove-architecture").unwrap();
        store
            .start_subagent(
                "session-1",
                "call-1",
                "audit the store",
                "2025-01-01T00:00:00.000Z",
            )
            .unwrap();

        assert!(store.delete_session("session-1").unwrap());
        assert!(store.session("session-1").unwrap().is_none());
        assert!(store.session_events("session-1").unwrap().is_empty());
        assert!(store.plan_row("session-1").unwrap().is_none());
        assert!(store.pins("session-1").unwrap().is_empty());
        let subagents: i64 = store
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM subagents WHERE session_id = ?1",
                    params!["session-1"],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(subagents, 0, "the worker trace goes with its parent");
        assert!(
            !store.delete_session("session-1").unwrap(),
            "deleting a missing row reports that nothing was removed"
        );
    }
}
