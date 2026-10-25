//! The agent-facing half of the plan: `.grove/sessions/<session>/plan.md` in the session's project.
//!
//! The record in SQLite stays authoritative; the mirror exists so an agent can edit the plan with an
//! ordinary file write. Two properties matter here:
//!
//! - **Atomic writes.** A write goes to a temp file in the same directory, is flushed, and is then
//!   renamed over the target. A reader (the agent) never sees half a plan, and a failed write never
//!   truncates the previous one.
//! - **A pure import decision.** [`decide_import`] compares three hashes — the record's content, the
//!   content the record last wrote to the mirror, and the file now — and says what a read should do.
//!   Keeping it a function is what makes the dual-edit rules testable without filesystem timing:
//!   `service.rs` gathers the three hashes under the store lock and calls it.

use crate::ipc::{IpcError, IpcResult};
use crate::paths::display_path;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The directory Grove owns inside a project (gitignored by the repository that hosts it).
pub const GROVE_DIR: &str = ".grove";
pub const SESSIONS_DIR: &str = "sessions";
pub const PLAN_FILE: &str = "plan.md";

/// Prefix of the temp file used for an atomic write. Never a directory entry the UI lists.
const TEMP_PREFIX: &str = ".plan-";

/// Where one session's plan mirror lives: `<project>/.grove/sessions/<id>/plan.md`.
///
/// The session id is a single path component; anything that could escape the sessions directory is
/// rejected rather than sanitised, because the id comes from the wire.
pub fn mirror_path(project_root: &Path, session_id: &str) -> IpcResult<PathBuf> {
    if session_id.is_empty()
        || session_id == "."
        || session_id == ".."
        || session_id.contains(['/', '\\', ':'])
    {
        return Err(IpcError::invalid(format!(
            "session id is not a single path component: {session_id:?}"
        )));
    }
    Ok(project_root
        .join(GROVE_DIR)
        .join(SESSIONS_DIR)
        .join(session_id)
        .join(PLAN_FILE))
}

/// SHA-256 of the bytes, hex-encoded. The same hash the spec graph uses for its hash-guarded writes.
pub fn content_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Read the mirror. `Ok(None)` means the file does not exist yet, which is the normal first-read
/// state, not an error.
pub fn read(path: &Path) -> IpcResult<Option<String>> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| {
            IpcError::io(format!(
                "plan mirror is not valid UTF-8: {}",
                display_path(path)
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(IpcError::io(format!(
            "cannot read plan mirror {}: {error}",
            display_path(path)
        ))),
    }
}

/// Write the mirror atomically: create the session directory if needed, write a temp file beside the
/// target, flush it, then rename over `path`.
pub fn write_atomic(path: &Path, markdown: &str) -> IpcResult<()> {
    let directory = path.parent().ok_or_else(|| {
        IpcError::io(format!(
            "plan mirror has no parent directory: {}",
            display_path(path)
        ))
    })?;
    std::fs::create_dir_all(directory)?;

    let temp = directory.join(format!("{TEMP_PREFIX}{}.tmp", uuid::Uuid::new_v4()));
    let written = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(markdown.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();

    written.map_err(|error| {
        // A failed write must not leave a scratch file behind for the agent to trip over.
        let _ = std::fs::remove_file(&temp);
        IpcError::io(format!(
            "cannot write plan mirror {}: {error}",
            display_path(path)
        ))
    })
}

/// What a read of the mirror should do, given the record and the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportDecision {
    /// No record and no file: the plan is empty and nothing is written.
    Empty,
    /// The file holds exactly what the record holds (or what it last wrote): nothing to do.
    InSync,
    /// The agent changed the file and the record has not moved since the last mirror write: import
    /// it, bump the version, and announce the change.
    Import,
    /// The record moved (or the file vanished): rewrite the mirror from the record. The record wins.
    RestoreMirror,
}

/// The dual-edit rule, as a table over three hashes:
///
/// - `record_hash` — the markdown the record holds, `None` when there is no row yet.
/// - `mirrored_hash` — the content the record last wrote to the mirror, `None` when it never wrote.
/// - `file_hash` — the file now, `None` when it is missing.
///
/// An external edit is imported only when the file differs from both the record and the last mirror
/// write: the record has not moved since the mirror was written, so the difference is the agent's. If
/// the record moved, the mirror is stale and is rewritten from the record instead.
pub fn decide_import(
    record_hash: Option<&str>,
    mirrored_hash: Option<&str>,
    file_hash: Option<&str>,
) -> ImportDecision {
    match (record_hash, file_hash) {
        // No record, no file: nothing has ever been written.
        (None, None) => ImportDecision::Empty,
        // No record but a file: the agent wrote the first plan. Adopt it at version 1.
        (None, Some(_)) => ImportDecision::Import,
        // The record exists and the file is gone: recreate it, do not bump the version.
        (Some(_), None) => ImportDecision::RestoreMirror,
        (Some(record), Some(file)) if record == file => ImportDecision::InSync,
        // The file differs from the record. Import only when the record still holds what it last
        // mirrored; otherwise the record has moved and the file is stale.
        (Some(record), Some(_)) if mirrored_hash == Some(record) => ImportDecision::Import,
        (Some(_), Some(_)) => ImportDecision::RestoreMirror,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &str = "record";
    const MIRRORED: &str = "mirrored";
    const FILE: &str = "file";

    #[test]
    fn import_decision_table() {
        use ImportDecision::*;

        // (record, mirrored, file) -> decision, with the reason the design picks it.
        let cases = [
            // Nothing on either side: a new session has an empty plan.
            ((None, None, None), Empty),
            // The agent wrote a plan before Grove ever did: adopt it.
            ((None, None, Some(FILE)), Import),
            // The record exists, the file vanished: recreate from the record, no version bump.
            ((Some(RECORD), None, None), RestoreMirror),
            ((Some(RECORD), Some(MIRRORED), None), RestoreMirror),
            // The file is exactly the record's content.
            ((Some(RECORD), None, Some(RECORD)), InSync),
            ((Some(RECORD), Some(RECORD), Some(RECORD)), InSync),
            // The row has not moved since the mirror write, and the file changed: the agent's edit.
            ((Some(RECORD), Some(RECORD), Some(FILE)), Import),
            // The row has moved since the mirror write: the file is stale, the record wins.
            ((Some(RECORD), Some(MIRRORED), Some(FILE)), RestoreMirror),
            // A row that was never mirrored has no mirror write to compare against: record wins.
            ((Some(RECORD), None, Some(FILE)), RestoreMirror),
        ];

        for ((record, mirrored, file), expected) in cases {
            assert_eq!(
                decide_import(record, mirrored, file),
                expected,
                "record={record:?} mirrored={mirrored:?} file={file:?}"
            );
        }
    }

    #[test]
    fn hashes_are_stable_and_differ_on_any_change() {
        assert_eq!(content_hash(b""), content_hash(b""));
        assert_ne!(content_hash(b"a"), content_hash(b"a\n"));
        assert_eq!(
            content_hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn mirror_path_joins_the_expected_components() {
        let path = mirror_path(Path::new("C:/work/grove"), "session-1").unwrap();
        assert_eq!(
            display_path(&path),
            "C:/work/grove/.grove/sessions/session-1/plan.md"
        );
    }

    #[test]
    fn mirror_path_rejects_ids_that_are_not_a_single_component() {
        for id in ["", ".", "..", "a/b", "a\\b", "C:plan"] {
            assert!(
                mirror_path(Path::new("C:/work/grove"), id).is_err(),
                "id {id:?} must be rejected"
            );
        }
    }

    #[test]
    fn write_atomic_creates_directories_and_replaces_content_without_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = mirror_path(dir.path(), "session-1").unwrap();

        write_atomic(&path, "first\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\n");

        write_atomic(&path, "second\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second\n");

        let names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["plan.md".to_string()]);
    }

    #[test]
    fn a_failed_write_leaves_the_target_and_the_directory_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = mirror_path(dir.path(), "session-1").unwrap();
        // A directory where the file belongs makes the rename fail after the temp file is written.
        std::fs::create_dir_all(&path).unwrap();

        assert!(write_atomic(&path, "never\n").is_err());
        assert!(path.is_dir(), "the target is still the directory it was");
        let names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["plan.md".to_string()],
            "no temp file is left behind"
        );
    }

    #[test]
    fn reading_a_missing_mirror_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = mirror_path(dir.path(), "session-1").unwrap();
        assert_eq!(read(&path).unwrap(), None);
    }
}
