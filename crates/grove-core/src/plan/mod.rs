//! The session TODO plan: one versioned plan per session, edited by the human in the Plan panel and
//! by the agent as a markdown file (see `architecture.md` §Persistence → plan dual-edit).
//!
//! SQLite is the record: `plans(session_id, markdown, version, mirror_hash)`. Every write also
//! mirrors `.grove/sessions/<id>/plan.md` inside the session's project, atomically. The mirror is the
//! agent-facing surface, so a read imports an external edit when the file no longer matches what the
//! record last wrote — and both sides move `version`, so a stale save is a `conflict` the UI resolves
//! by re-reading instead of clobbering.
//!
//! - [`mirror`] — paths, hashing, the atomic write, and the pure import decision
//! - [`service`] — [`PlanService`], the version-guarded record/mirror pair

pub mod mirror;
pub mod service;

pub use mirror::{decide_import, ImportDecision};
pub use service::PlanService;

use serde::{Deserialize, Serialize};

/// One session's plan as the panel and the session-start path consume it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub session_id: String,
    pub markdown: String,
    /// Starts at 0 (no plan yet) and moves on every accepted write, human or agent.
    pub version: u64,
    /// Display form (forward slashes) of the mirror file the agent edits.
    pub mirror_path: String,
}

/// The arguments to a plan save: the new markdown plus the version it was based on.
///
/// `expected_version` is the optimistic-concurrency guard: a save whose version no longer matches
/// returns `IpcError::Conflict` and the UI reloads rather than overwriting the other writer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanUpdate {
    pub markdown: String,
    pub expected_version: u64,
}

impl PlanUpdate {
    pub fn new(markdown: impl Into<String>, expected_version: u64) -> Self {
        Self {
            markdown: markdown.into(),
            expected_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_shape_is_camel_case() {
        let plan = Plan {
            session_id: "session-1".into(),
            markdown: "# Plan\n".into(),
            version: 2,
            mirror_path: "C:/work/grove/.grove/sessions/session-1/plan.md".into(),
        };
        assert_eq!(
            serde_json::to_value(&plan).unwrap(),
            serde_json::json!({
                "sessionId": "session-1",
                "markdown": "# Plan\n",
                "version": 2,
                "mirrorPath": "C:/work/grove/.grove/sessions/session-1/plan.md",
            })
        );

        let update = PlanUpdate::new("# Plan\n", 2);
        assert_eq!(
            serde_json::to_value(&update).unwrap(),
            serde_json::json!({ "markdown": "# Plan\n", "expectedVersion": 2 })
        );
    }
}
