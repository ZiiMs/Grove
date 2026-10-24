//! The project's spec graph.
//!
//! Specs are the repository's own markdown files. A file becomes a node when its YAML frontmatter
//! carries `id` and `type`; the graph is derived on demand from files, never copied into a database
//! (see `architecture.md` §Persistence). This module owns:
//!
//! - [`frontmatter`] — the small, tested YAML subset spec files use
//! - [`index`] — walking a root, tree/ancestor/subtree/neighbour queries, validation, read/write
//! - [`watch`] — coalesced `notify` events for the shell's `grove://spec-changed` event
//!
//! The wire shapes (`SpecNode`, `SpecGraph`, `SpecDoc`, `SpecValidation`, `SpecChange`) live here and
//! are mirrored in `packages/contract/src/specs.ts`; `contract/fixtures/spec*.json` pins both sides.

pub mod frontmatter;
pub mod index;
pub mod watch;

pub use frontmatter::{Frontmatter, FrontmatterError, FrontmatterValue};
pub use index::{
    read_spec, write_spec, SpecIndex, MAX_INDEXED_FILES, MAX_SPEC_BYTES, SKIPPED_DIRS,
};
pub use watch::{SpecChange, SpecWatcher, DEFAULT_DEBOUNCE};

use serde::{Deserialize, Serialize};

/// One indexed spec file. `path` is relative to the indexed root and always forward-slash, so the
/// wire shape is identical on Windows and POSIX.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecNode {
    pub id: String,
    #[serde(rename = "type")]
    pub spec_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// `depends-on` in the file, `dependsOn` on the wire.
    pub depends_on: Vec<String>,
    pub references: Vec<String>,
    pub implements: Vec<String>,
    pub covers: Vec<String>,
}

/// The whole graph as the Specs panel consumes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecGraph {
    /// Display form of the indexed root (forward slashes, no Windows verbatim prefix).
    pub root: String,
    pub nodes: Vec<SpecNode>,
    pub validation: SpecValidation,
}

/// A spec plus its editable body and the hash a save must present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecDoc {
    pub node: SpecNode,
    pub body: String,
    /// SHA-256 of the whole file (frontmatter + body), hex-encoded.
    pub hash: String,
}

/// Everything wrong with the graph. Empty vectors mean healthy; each finding carries the offending
/// file path(s) so the panel can point at the problem.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecValidation {
    pub dangling_links: Vec<SpecDanglingLink>,
    pub duplicate_ids: Vec<SpecDuplicateId>,
    pub parent_cycles: Vec<SpecParentCycle>,
}

/// A link field naming an id that no indexed node provides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecDanglingLink {
    pub from: String,
    pub path: String,
    /// `parent`, `depends-on`, `references`, `implements` or `covers`.
    pub field: String,
    pub target: String,
}

/// The same id declared by more than one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecDuplicateId {
    pub id: String,
    pub paths: Vec<String>,
}

/// A parent loop, canonicalised to start at the smallest id so it is reported once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecParentCycle {
    pub ids: Vec<String>,
    pub paths: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_fields_are_omitted_from_the_wire() {
        let node = SpecNode {
            id: "a".into(),
            spec_type: "module-design".into(),
            title: None,
            status: None,
            path: "a.md".into(),
            parent: None,
            depends_on: Vec::new(),
            references: Vec::new(),
            implements: Vec::new(),
            covers: Vec::new(),
        };
        let value = serde_json::to_value(node).unwrap();
        assert!(value.get("title").is_none());
        assert!(value.get("parent").is_none());
        assert_eq!(value["type"], "module-design");
    }
}
