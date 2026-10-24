//! Indexing and the hash-guarded read/write pair.
//!
//! The index walks a project root once, reads only `.md` files whose frontmatter carries `id` and
//! `type`, and derives the validation report while building. Reads and writes resolve paths relative
//! to the same root and refuse to escape it. A write carries the hash of the file it was based on, so
//! a concurrent edit returns `conflict` instead of being clobbered.

use super::frontmatter::Frontmatter;
use super::{
    SpecDanglingLink, SpecDoc, SpecDuplicateId, SpecGraph, SpecNode, SpecParentCycle,
    SpecValidation,
};
use crate::ipc::{IpcError, IpcResult};
use crate::paths::display_path;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

/// Directories that never contain project specs. Pruned before their contents are visited, which is
/// what keeps indexing a checkout with a populated `node_modules`/`target` cheap.
pub const SKIPPED_DIRS: &[&str] = &["node_modules", "target", ".git", "dist"];

/// Hard bound on files considered in one index pass, so pointing Grove at a huge tree cannot stall
/// the UI. The first 5,000 markdown files are plenty for a spec graph.
pub const MAX_INDEXED_FILES: usize = 5_000;

/// Hard bound on filesystem entries visited, directories included. Together with the skipped
/// directories this keeps a wrong project root from turning into a full-disk walk.
pub const MAX_VISITED_ENTRIES: usize = 50_000;

/// Files larger than this are not spec files; skipping them keeps a stray dump from being read.
pub const MAX_SPEC_BYTES: u64 = 2 * 1024 * 1024;

/// A parsed, validated view of every spec under a root.
#[derive(Debug, Clone)]
pub struct SpecIndex {
    root: PathBuf,
    nodes: Vec<SpecNode>,
    validation: SpecValidation,
}

impl SpecIndex {
    /// Read the graph under `root`. Missing roots are an I/O error; malformed files are skipped (the
    /// graph is best-effort, exactly like a human skimming the repo).
    pub fn load(root: impl AsRef<Path>) -> IpcResult<Self> {
        let root = std::fs::canonicalize(root.as_ref()).map_err(|error| {
            IpcError::io(format!(
                "cannot index spec root {}: {error}",
                root.as_ref().display()
            ))
        })?;
        if !root.is_dir() {
            return Err(IpcError::invalid(format!(
                "spec root is not a directory: {}",
                display_path(&root)
            )));
        }

        let mut nodes = Vec::new();
        let mut considered = 0usize;
        let mut visited = 0usize;
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| !is_skipped(entry));

        for entry in walker {
            visited += 1;
            if visited > MAX_VISITED_ENTRIES {
                tracing::warn!(
                    target: "grove::specgraph",
                    limit = MAX_VISITED_ENTRIES,
                    "spec index hit its traversal bound; remaining entries are ignored"
                );
                break;
            }
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() || !is_markdown(entry.path()) {
                continue;
            }
            considered += 1;
            if considered > MAX_INDEXED_FILES {
                tracing::warn!(
                    target: "grove::specgraph",
                    limit = MAX_INDEXED_FILES,
                    "spec index hit its file bound; remaining files are ignored"
                );
                break;
            }
            if entry
                .metadata()
                .map(|metadata| metadata.len() > MAX_SPEC_BYTES)
                .unwrap_or(false)
            {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok(frontmatter) = Frontmatter::parse(&source) else {
                continue;
            };
            let Some(path) = relative_path(&root, entry.path()) else {
                continue;
            };
            if let Some(node) = node_from_frontmatter(path, &frontmatter) {
                nodes.push(node);
            }
        }

        nodes.sort_by(|left, right| {
            (&left.spec_type, &left.id, &left.path).cmp(&(&right.spec_type, &right.id, &right.path))
        });
        let validation = validate(&nodes);
        Ok(Self {
            root,
            nodes,
            validation,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn nodes(&self) -> &[SpecNode] {
        &self.nodes
    }

    pub fn validation(&self) -> &SpecValidation {
        &self.validation
    }

    pub fn graph(&self) -> SpecGraph {
        SpecGraph {
            root: rooted_display(&self.root),
            nodes: self.nodes.clone(),
            validation: self.validation.clone(),
        }
    }

    /// Tree roots: nodes without a parent, plus orphans whose parent id is not indexed (they would
    /// otherwise be invisible in the tree and only appear in the validation report).
    pub fn roots(&self) -> Vec<SpecNode> {
        let known: BTreeSet<&str> = self.nodes.iter().map(|node| node.id.as_str()).collect();
        self.nodes
            .iter()
            .filter(|node| match node.parent.as_deref() {
                None => true,
                Some(parent) => !known.contains(parent),
            })
            .cloned()
            .collect()
    }

    /// Parent chain, nearest parent first. Cycles stop at the repeated id instead of looping.
    pub fn ancestors(&self, id: &str) -> Vec<SpecNode> {
        let by_id = by_id(&self.nodes);
        let mut chain = Vec::new();
        let mut visited = BTreeSet::new();
        let mut current = by_id.get(id).and_then(|node| node.parent.as_deref());
        while let Some(parent) = current {
            if !visited.insert(parent) {
                break;
            }
            let Some(node) = by_id.get(parent) else {
                break;
            };
            chain.push((*node).clone());
            current = node.parent.as_deref();
        }
        chain
    }

    /// Every descendant, sorted by id, excluding the node itself.
    pub fn subtree(&self, id: &str) -> Vec<SpecNode> {
        let mut found = BTreeMap::new();
        let mut queue = vec![id.to_string()];
        while let Some(parent) = queue.pop() {
            for node in &self.nodes {
                if node.parent.as_deref() == Some(parent.as_str())
                    && node.id != id
                    && !found.contains_key(&node.id)
                {
                    found.insert(node.id.clone(), node.clone());
                    queue.push(node.id.clone());
                }
            }
        }
        found.into_values().collect()
    }

    /// Any node sharing an edge with `id`, in either direction. Parent, children, and every link
    /// field on both sides — the set a click in the reader can navigate to.
    pub fn neighbours(&self, id: &str) -> Vec<SpecNode> {
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            let outgoing = node.id == id;
            let incoming = node.parent.as_deref() == Some(id)
                || node.depends_on.iter().any(|target| target == id)
                || node.references.iter().any(|target| target == id)
                || node.implements.iter().any(|target| target == id)
                || node.covers.iter().any(|target| target == id);
            if !(outgoing || incoming) {
                continue;
            }
            if node.id != id {
                ids.insert(node.id.clone());
            }
            if let Some(parent) = node.parent.as_deref() {
                if parent != id {
                    ids.insert(parent.to_string());
                }
            }
            for target in node
                .depends_on
                .iter()
                .chain(node.references.iter())
                .chain(node.implements.iter())
                .chain(node.covers.iter())
            {
                if target != id {
                    ids.insert(target.clone());
                }
            }
        }
        ids.into_iter()
            .filter(|target| self.node(target).is_some())
            .filter_map(|target| self.node(&target).cloned())
            .collect()
    }

    pub fn node(&self, id: &str) -> Option<&SpecNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn node_by_path(&self, path: &str) -> Option<&SpecNode> {
        let normalised = path.replace('\\', "/");
        self.nodes.iter().find(|node| node.path == normalised)
    }
}

/// Read one spec: parsed frontmatter, body and the hash a subsequent write must present.
pub fn read_spec(root: &Path, path: &str) -> IpcResult<SpecDoc> {
    let (root, full) = resolve_existing(root, path)?;
    let bytes = std::fs::read(&full).map_err(|error| read_error(path, &error))?;
    let source = String::from_utf8(bytes.clone())
        .map_err(|_| IpcError::invalid(format!("spec is not valid UTF-8: {path}")))?;
    let frontmatter = Frontmatter::parse(&source).map_err(|error| {
        IpcError::invalid(format!("{path}: {error}")).with_detail(display_path(&full))
    })?;
    let relative = relative_path(&root, &full).unwrap_or_else(|| path.to_string());
    let node = node_from_frontmatter(relative, &frontmatter).ok_or_else(|| {
        IpcError::invalid(format!(
            "{path}: frontmatter must carry both `id` and `type` to be a spec node"
        ))
    })?;
    Ok(SpecDoc {
        node,
        body: frontmatter.body().to_string(),
        hash: content_hash(&bytes),
    })
}

/// Replace a spec's body, guarded by the hash of the file the edit was based on.
///
/// The frontmatter block is preserved byte-for-byte, so formatting the user never saw in the editor
/// cannot be rewritten by a save. The hash covers the whole file; a successful write changes it, so
/// the next save must re-read first.
pub fn write_spec(root: &Path, path: &str, body: &str, expected_hash: &str) -> IpcResult<SpecDoc> {
    let (root, full) = resolve_existing(root, path)?;
    let bytes = std::fs::read(&full).map_err(|error| read_error(path, &error))?;
    let actual_hash = content_hash(&bytes);
    if actual_hash != expected_hash {
        return Err(IpcError::conflict(format!("spec changed on disk: {path}"))
            .with_detail(format!("expected {expected_hash}, found {actual_hash}")));
    }
    let source = String::from_utf8(bytes)
        .map_err(|_| IpcError::invalid(format!("spec is not valid UTF-8: {path}")))?;
    let frontmatter = Frontmatter::parse(&source).map_err(|error| {
        IpcError::invalid(format!("{path}: {error}")).with_detail(display_path(&full))
    })?;

    let next = format!("{}{}", frontmatter.raw(), body);
    std::fs::write(&full, next.as_bytes()).map_err(|error| IpcError::io(error.to_string()))?;
    read_spec(&root, path)
}

pub(crate) fn content_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Root path for display and contract fixtures: forward slashes, no Windows verbatim prefix.
pub(crate) fn rooted_display(root: &Path) -> String {
    let text = display_path(root);
    if let Some(stripped) = text.strip_prefix("//?/") {
        if let Some(unc) = stripped.strip_prefix("UNC/") {
            return format!("//{unc}");
        }
        return stripped.to_string();
    }
    text
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

fn is_skipped(entry: &DirEntry) -> bool {
    if entry.depth() == 0 || !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    SKIPPED_DIRS.contains(&name.as_str())
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

fn node_from_frontmatter(path: String, frontmatter: &Frontmatter) -> Option<SpecNode> {
    Some(SpecNode {
        id: frontmatter.scalar("id")?.to_string(),
        spec_type: frontmatter.scalar("type")?.to_string(),
        title: frontmatter.scalar("title").map(str::to_string),
        status: frontmatter.scalar("status").map(str::to_string),
        path,
        parent: frontmatter.scalar("parent").map(str::to_string),
        depends_on: frontmatter.list("depends-on"),
        references: frontmatter.list("references"),
        implements: frontmatter.list("implements"),
        covers: frontmatter.list("covers"),
    })
}

fn by_id(nodes: &[SpecNode]) -> HashMap<&str, &SpecNode> {
    let mut map = HashMap::new();
    for node in nodes {
        map.entry(node.id.as_str()).or_insert(node);
    }
    map
}

fn validate(nodes: &[SpecNode]) -> SpecValidation {
    let by_id = by_id(nodes);
    let mut dangling_links = Vec::new();
    for node in nodes {
        let mut check = |field: &str, target: &str| {
            if !by_id.contains_key(target) {
                dangling_links.push(SpecDanglingLink {
                    from: node.id.clone(),
                    path: node.path.clone(),
                    field: field.to_string(),
                    target: target.to_string(),
                });
            }
        };
        if let Some(parent) = node.parent.as_deref() {
            check("parent", parent);
        }
        for target in &node.depends_on {
            check("depends-on", target);
        }
        for target in &node.references {
            check("references", target);
        }
        for target in &node.implements {
            check("implements", target);
        }
        for target in &node.covers {
            check("covers", target);
        }
    }
    dangling_links.sort_by(|left, right| {
        (&left.path, &left.field, &left.target).cmp(&(&right.path, &right.field, &right.target))
    });

    let mut grouped: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for node in nodes {
        grouped
            .entry(node.id.as_str())
            .or_default()
            .push(node.path.as_str());
    }
    let mut duplicate_ids: Vec<SpecDuplicateId> = grouped
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(id, mut paths)| {
            paths.sort_unstable();
            SpecDuplicateId {
                id: id.to_string(),
                paths: paths.into_iter().map(str::to_string).collect(),
            }
        })
        .collect();
    duplicate_ids.sort_by(|left, right| left.id.cmp(&right.id));

    let mut cycles: BTreeSet<Vec<String>> = BTreeSet::new();
    for node in nodes {
        let mut visited = Vec::new();
        let mut current = Some(node.id.as_str());
        while let Some(id) = current {
            if let Some(start) = visited.iter().position(|seen| *seen == id) {
                let cycle = canonical_cycle(&visited[start..]);
                if !cycle.is_empty() {
                    cycles.insert(cycle);
                }
                break;
            }
            visited.push(id);
            current = by_id.get(id).and_then(|node| node.parent.as_deref());
        }
    }
    let parent_cycles = cycles
        .into_iter()
        .map(|ids| SpecParentCycle {
            paths: ids
                .iter()
                .map(|id| {
                    by_id
                        .get(id.as_str())
                        .map(|node| node.path.clone())
                        .unwrap_or_default()
                })
                .collect(),
            ids,
        })
        .collect();

    SpecValidation {
        dangling_links,
        duplicate_ids,
        parent_cycles,
    }
}

/// Rotate a cycle so it starts at the smallest id. The same loop reached from different nodes then
/// produces one report entry, not one per starting point.
fn canonical_cycle(ids: &[&str]) -> Vec<String> {
    let Some((index, _)) = ids
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.cmp(right))
    else {
        return Vec::new();
    };
    ids[index..]
        .iter()
        .chain(ids[..index].iter())
        .map(|id| id.to_string())
        .collect()
}

fn relative_spec_path(path: &str) -> IpcResult<PathBuf> {
    if path.trim().is_empty() {
        return Err(IpcError::invalid("spec path is empty"));
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(IpcError::invalid(format!(
            "spec path must be relative to the project root: {path}"
        )));
    }
    let mut relative = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => {
                return Err(IpcError::invalid(format!(
                    "spec path must stay inside the project root: {path}"
                )))
            }
        }
    }
    if relative.as_os_str().is_empty() || !is_markdown(&relative) {
        return Err(IpcError::invalid(format!(
            "spec path must name a markdown file: {path}"
        )));
    }
    Ok(relative)
}

fn resolve_existing(root: &Path, path: &str) -> IpcResult<(PathBuf, PathBuf)> {
    let root = std::fs::canonicalize(root).map_err(|error| {
        IpcError::io(format!(
            "cannot open spec root {}: {error}",
            display_path(root)
        ))
    })?;
    let relative = relative_spec_path(path)?;
    let full = root.join(&relative);
    let canonical = std::fs::canonicalize(&full).map_err(|error| read_error(path, &error))?;
    if !canonical.starts_with(&root) {
        return Err(IpcError::invalid(format!(
            "spec path escapes the project root: {path}"
        )));
    }
    if !canonical.is_file() {
        return Err(IpcError::not_found(format!("spec not found: {path}")));
    }
    Ok((root, canonical))
}

fn read_error(path: &str, error: &std::io::Error) -> IpcError {
    if error.kind() == std::io::ErrorKind::NotFound {
        IpcError::not_found(format!("spec not found: {path}"))
    } else {
        IpcError::io(format!("cannot read spec {path}: {error}"))
    }
    .with_detail(format!("code={:?}", error.kind()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ErrorCode;

    fn node(id: &str, path: &str, parent: Option<&str>) -> SpecNode {
        SpecNode {
            id: id.to_string(),
            spec_type: "module-design".to_string(),
            title: None,
            status: None,
            path: path.to_string(),
            parent: parent.map(str::to_string),
            depends_on: Vec::new(),
            references: Vec::new(),
            implements: Vec::new(),
            covers: Vec::new(),
        }
    }

    #[test]
    fn cycles_are_rotated_to_a_canonical_start() {
        assert_eq!(
            canonical_cycle(&["b", "c", "a"]),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn validation_reports_each_cycle_once() {
        let nodes = vec![node("a", "a.md", Some("b")), node("b", "b.md", Some("a"))];
        let validation = validate(&nodes);
        assert_eq!(validation.parent_cycles.len(), 1);
        assert_eq!(
            validation.parent_cycles[0].ids,
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn validation_ignores_healthy_links() {
        let nodes = vec![
            node("root", "root.md", None),
            node("child", "child.md", Some("root")),
        ];
        let validation = validate(&nodes);
        assert!(validation.dangling_links.is_empty());
        assert!(validation.parent_cycles.is_empty());
        assert!(validation.duplicate_ids.is_empty());
    }

    #[test]
    fn rooted_display_strips_windows_verbatim_prefix() {
        let display = rooted_display(Path::new(r"\\?\C:\demo\grove"));
        assert_eq!(display, "C:/demo/grove");
    }

    #[test]
    fn traversal_is_rejected_before_the_filesystem_is_touched() {
        let error = relative_spec_path("../escape.md").unwrap_err();
        assert_eq!(error.code, ErrorCode::Invalid);
        assert!(relative_spec_path("notes.txt").is_err());
    }
}
