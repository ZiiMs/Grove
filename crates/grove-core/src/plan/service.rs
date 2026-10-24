//! [`PlanService`]: the version-guarded record plus its mirror.
//!
//! Every public operation takes one gate mutex, so the read → decide → write sequence of a save or an
//! import cannot interleave with another operation in this process. The record is written with a
//! compare-and-set statement (`expected_version`), which is what makes two writers — human and
//! agent, or two windows — resolve to one winner and one `conflict`.
//!
//! Imports are found by a hash check before every read ([`PlanService::get`], [`PlanService::text`]
//! and the import step of [`PlanService::set`]). That is the watcher: no native notification is
//! required, and an edit made while the app was closed is imported on the next read. An import bumps
//! the version and is announced on the bus the shell forwards as `grove://plan-changed`.

use super::mirror::{self, ImportDecision};
use super::{Plan, PlanUpdate};
use crate::ipc::{EventEnvelope, IpcError, IpcResult};
use crate::paths::display_path;
use crate::store::{PlanRow, Store, StoreError};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::broadcast;

/// The bus the Tauri shell subscribes to and forwards as `grove://plan-changed`.
pub type PlanBus = broadcast::Sender<EventEnvelope<Plan>>;

pub struct PlanService {
    store: Arc<Store>,
    /// Project used when a session row has no usable `cwd` (never for a normal session).
    project_root: PathBuf,
    bus: PlanBus,
    /// Serializes read/import/write sequences; SQLite supplies the cross-process guard.
    gate: Mutex<()>,
}

impl PlanService {
    pub fn new(store: Arc<Store>, project_root: impl Into<PathBuf>) -> Self {
        let (bus, _) = broadcast::channel(64);
        Self {
            store,
            project_root: project_root.into(),
            bus,
            gate: Mutex::new(()),
        }
    }

    /// The external-change bus. The shell forwards each envelope as `grove://plan-changed`.
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope<Plan>> {
        self.bus.subscribe()
    }

    /// The plan for one session. Before answering, the mirror is compared with the record so an
    /// agent's edit is imported (and announced) rather than shown stale.
    pub fn get(&self, session_id: &str) -> IpcResult<Plan> {
        let path = self.mirror_file(session_id)?;
        let _gate = self.lock();
        let row = self.sync(session_id, &path)?;
        Ok(plan_at(session_id, &path, row))
    }

    /// Save the human's edit. `expected_version` must be the version the editor was showing; a stale
    /// save returns [`IpcError::Conflict`] and leaves both the record and the mirror alone.
    pub fn set(&self, session_id: &str, update: PlanUpdate) -> IpcResult<Plan> {
        let path = self.mirror_file(session_id)?;
        let _gate = self.lock();

        // Import an external edit first. It moves the version, so a save based on the pre-edit
        // version is refused below instead of clobbering the agent's work.
        let row = self.sync(session_id, &path)?;
        let current = row.as_ref().map(|row| row.version).unwrap_or(0);
        if update.expected_version != current {
            return Err(conflict(&path, update.expected_version, current));
        }

        let hash = mirror::content_hash(update.markdown.as_bytes());
        // Mirror first: an accepted save is always mirrored, and an unwritable project is a clean
        // rejection instead of a record that silently stops matching its file.
        mirror::write_atomic(&path, &update.markdown)?;

        if !stored(
            self.store
                .write_plan(session_id, &update.markdown, current, &hash),
        )? {
            // Another process moved the record between the check and the write: put the record's
            // content back into the mirror and report the conflict. The record always wins.
            if let Some(latest) = stored(self.store.plan_row(session_id))? {
                mirror::write_atomic(&path, &latest.markdown)?;
                let latest_hash = mirror::content_hash(latest.markdown.as_bytes());
                if latest.mirror_hash.as_deref() != Some(latest_hash.as_str()) {
                    stored(self.store.record_mirror_hash(
                        session_id,
                        latest.version,
                        &latest_hash,
                    ))?;
                }
                return Err(conflict(&path, update.expected_version, latest.version));
            }
            return Err(conflict(&path, update.expected_version, current));
        }

        let written = stored(self.store.plan_row(session_id))?.ok_or_else(|| {
            IpcError::io(format!(
                "plan record for session {session_id} vanished after a write"
            ))
        })?;
        Ok(plan_at(session_id, &path, Some(written)))
    }

    /// The plan text for the session-start path (the prompt context ticket 14 wires up).
    ///
    /// `None` means there is no plan worth putting in a prompt. Reading still imports an external
    /// edit, so a session that starts after the agent rewrote the file starts from the new plan.
    pub fn text(&self, session_id: &str) -> IpcResult<Option<String>> {
        let plan = self.get(session_id)?;
        Ok(if plan.markdown.trim().is_empty() {
            None
        } else {
            Some(plan.markdown)
        })
    }

    /// The display path of the mirror, for the panel and for diagnostics.
    pub fn mirror_path(&self, session_id: &str) -> IpcResult<String> {
        Ok(display_path(&self.mirror_file(session_id)?))
    }

    /// Where the session's mirror lives: inside the session's own project, which is where the agent
    /// runs. The configured project root is only a fallback for a row with no `cwd`.
    fn mirror_file(&self, session_id: &str) -> IpcResult<PathBuf> {
        let session = stored(self.store.session(session_id))?
            .ok_or_else(|| IpcError::not_found(format!("no session: {session_id}")))?;
        let project = PathBuf::from(session.cwd.trim());
        let project = if project.as_os_str().is_empty() {
            self.project_root.clone()
        } else {
            project
        };
        mirror::mirror_path(&project, session_id)
    }

    /// Bring the mirror and the record back in step. Must be called with the gate held.
    ///
    /// Returns the record as it stands afterwards (`None` when no plan has ever been written).
    fn sync(&self, session_id: &str, path: &Path) -> IpcResult<Option<PlanRow>> {
        let row = stored(self.store.plan_row(session_id))?;
        let file = mirror::read(path)?;
        let file_hash = file
            .as_deref()
            .map(|markdown| mirror::content_hash(markdown.as_bytes()));
        let record_hash = row
            .as_ref()
            .map(|row| mirror::content_hash(row.markdown.as_bytes()));
        let decision = mirror::decide_import(
            record_hash.as_deref(),
            row.as_ref().and_then(|row| row.mirror_hash.as_deref()),
            file_hash.as_deref(),
        );

        match decision {
            ImportDecision::Empty | ImportDecision::InSync => Ok(row),
            ImportDecision::Import => {
                let markdown = file.unwrap_or_default();
                let hash = file_hash.unwrap_or_default();
                let expected = row.as_ref().map(|row| row.version).unwrap_or(0);
                if !stored(
                    self.store
                        .write_plan(session_id, &markdown, expected, &hash),
                )? {
                    // Another process moved the record; the next read re-evaluates against it.
                    return stored(self.store.plan_row(session_id));
                }
                let updated = stored(self.store.plan_row(session_id))?;
                if let Some(plan) = updated
                    .as_ref()
                    .map(|row| plan_at(session_id, path, Some(row.clone())))
                {
                    // No receivers is normal (headless tests, a closed window).
                    let _ = self.bus.send(EventEnvelope::session(session_id, plan));
                }
                Ok(updated)
            }
            ImportDecision::RestoreMirror => {
                let Some(row) = row else { return Ok(None) };
                mirror::write_atomic(path, &row.markdown)?;
                let hash = mirror::content_hash(row.markdown.as_bytes());
                if row.mirror_hash.as_deref() != Some(hash.as_str()) {
                    stored(
                        self.store
                            .record_mirror_hash(session_id, row.version, &hash),
                    )?;
                }
                stored(self.store.plan_row(session_id))
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        // A poisoned gate means a caller panicked mid-operation; the store handle is still usable and
        // the SQLite compare-and-set still protects the record, so recover instead of wedging plans.
        self.gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn plan_at(session_id: &str, path: &Path, row: Option<PlanRow>) -> Plan {
    match row {
        Some(row) => Plan {
            session_id: session_id.to_string(),
            markdown: row.markdown,
            version: row.version,
            mirror_path: display_path(path),
        },
        None => Plan {
            session_id: session_id.to_string(),
            markdown: String::new(),
            version: 0,
            mirror_path: display_path(path),
        },
    }
}

fn conflict(path: &Path, expected: u64, found: u64) -> IpcError {
    IpcError::conflict(format!(
        "plan changed: expected version {expected}, found {found}"
    ))
    .with_detail(format!("reload the plan and retry: {}", display_path(path)))
}

/// Store failures are I/O from the command surface's point of view.
fn stored<T>(result: Result<T, StoreError>) -> IpcResult<T> {
    result.map_err(|error| IpcError::io(error.to_string()))
}
