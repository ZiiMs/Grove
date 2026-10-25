//! Spec graph behaviour, exercised against this repository and against temporary fixture repos.
//!
//! The real files (`goal-and-requirements.md`, `architecture.md`, `docs/sidecar-protocol.md`) are valid
//! spec nodes and are the fixture for the happy path. The error cases — dangling links, duplicate ids,
//! parent cycles — need broken files, so those live in a tempdir.

use grove_core::specgraph::{read_spec, write_spec, SpecChange, SpecIndex, SpecWatcher};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root exists")
}

fn write_spec_file(root: &Path, relative: &str, frontmatter: &str, body: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture directory");
    }
    std::fs::write(path, format!("---\n{frontmatter}\n---\n{body}")).expect("write fixture spec");
}

fn ids(nodes: Vec<grove_core::specgraph::SpecNode>) -> Vec<String> {
    nodes.into_iter().map(|node| node.id).collect()
}

#[test]
fn indexes_this_repository_as_a_spec_graph() {
    let index = SpecIndex::load(repo_root()).expect("index the repository");

    let goal = index
        .node("grove-goal-and-requirements")
        .expect("goal spec is indexed");
    assert_eq!(goal.spec_type, "goal-and-requirements");
    assert_eq!(goal.path, "goal-and-requirements.md");
    assert_eq!(
        goal.title.as_deref(),
        Some("Grove — one desktop home for every AI coding provider")
    );

    let architecture = index
        .node("grove-architecture")
        .expect("architecture spec is indexed");
    assert_eq!(architecture.spec_type, "architecture-design");
    assert_eq!(architecture.path, "architecture.md");
    assert_eq!(
        architecture.parent.as_deref(),
        Some("grove-goal-and-requirements")
    );

    assert_eq!(
        ids(index.roots()),
        vec!["grove-goal-and-requirements".to_string()]
    );
    assert_eq!(
        ids(index.ancestors("grove-architecture")),
        vec!["grove-goal-and-requirements".to_string()]
    );
    assert_eq!(
        ids(index.subtree("grove-goal-and-requirements")),
        vec!["grove-architecture".to_string()]
    );
    assert_eq!(
        ids(index.neighbours("grove-architecture")),
        vec!["grove-goal-and-requirements".to_string()]
    );

    assert!(
        !index.graph().root.contains("//?/"),
        "the graph root is display-ready: {}",
        index.graph().root
    );

    // The index is bounded: dependency and build directories never contribute specs.
    for node in index.nodes() {
        for skipped in ["node_modules/", "target/", ".git/", "dist/"] {
            assert!(
                !node.path.starts_with(skipped),
                "{} must not be indexed from {skipped}",
                node.path
            );
        }
    }

    // The repo's own graph is healthy, so nothing is reported.
    let validation = index.graph().validation;
    assert!(validation.dangling_links.is_empty());
    assert!(validation.duplicate_ids.is_empty());
    assert!(validation.parent_cycles.is_empty());
}

#[test]
fn validates_dangling_links_duplicates_and_parent_cycles() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    write_spec_file(
        root,
        "goal.md",
        "id: goal\ntype: goal-and-requirements\ntitle: Goal\nstatus: active",
        "# Goal\n",
    );
    write_spec_file(
        root,
        "arch.md",
        "id: arch\ntype: architecture-design\nparent: goal\ndepends-on: [goal]\nreferences: [ghost]\ncovers: [\"goal\"]",
        "# Architecture\n",
    );
    write_spec_file(
        root,
        "block.md",
        "id: block\ntype: module-design\nparent: arch\nimplements:\n  - arch\n  - goal\nreferences: [arch]",
        "# Block\n",
    );
    write_spec_file(
        root,
        "nested/deep.md",
        "id: deep\ntype: module-design\nparent: arch",
        "# Deep\n",
    );
    write_spec_file(
        root,
        "dup-a.md",
        "id: dup\ntype: module-design",
        "# Dup A\n",
    );
    write_spec_file(
        root,
        "dup-b.md",
        "id: dup\ntype: module-design\nparent: missing-parent",
        "# Dup B\n",
    );
    write_spec_file(
        root,
        "cycle-a.md",
        "id: cycle-a\ntype: module-design\nparent: cycle-b",
        "# Cycle A\n",
    );
    write_spec_file(
        root,
        "cycle-b.md",
        "id: cycle-b\ntype: module-design\nparent: cycle-a",
        "# Cycle B\n",
    );

    // Files that must not reach the graph.
    std::fs::write(root.join("README.md"), "# Not a spec\n").unwrap();
    std::fs::write(root.join("notes.txt"), "id: ignored\ntype: ignored\n").unwrap();
    for skipped in ["node_modules", "target", ".git", "dist"] {
        write_spec_file(
            root,
            &format!("{skipped}/skip.md"),
            "id: skipped\ntype: module-design",
            "# Skipped\n",
        );
    }

    let index = SpecIndex::load(root).expect("index fixture");
    let node_ids: Vec<&str> = index.nodes().iter().map(|node| node.id.as_str()).collect();
    assert!(
        !node_ids.contains(&"skipped"),
        "skipped directories are pruned"
    );
    assert!(
        !node_ids.contains(&"ignored"),
        "non-markdown files are pruned"
    );
    assert_eq!(
        node_ids.iter().filter(|id| **id == "dup").count(),
        2,
        "duplicate ids are both indexed so the validation can report them"
    );

    let graph = index.graph();
    assert_eq!(graph.validation.duplicate_ids.len(), 1);
    assert_eq!(graph.validation.duplicate_ids[0].id, "dup");
    assert_eq!(
        graph.validation.duplicate_ids[0].paths,
        vec!["dup-a.md".to_string(), "dup-b.md".to_string()]
    );

    assert_eq!(graph.validation.parent_cycles.len(), 1);
    assert_eq!(
        graph.validation.parent_cycles[0].ids,
        vec!["cycle-a".to_string(), "cycle-b".to_string()]
    );
    assert_eq!(
        graph.validation.parent_cycles[0].paths,
        vec!["cycle-a.md".to_string(), "cycle-b.md".to_string()]
    );

    let dangling: Vec<(&str, &str, &str, &str)> = graph
        .validation
        .dangling_links
        .iter()
        .map(|link| {
            (
                link.from.as_str(),
                link.path.as_str(),
                link.field.as_str(),
                link.target.as_str(),
            )
        })
        .collect();
    assert!(
        dangling.contains(&("arch", "arch.md", "references", "ghost")),
        "missing reference is reported with its path: {dangling:?}"
    );
    assert!(
        dangling.contains(&("dup", "dup-b.md", "parent", "missing-parent")),
        "orphan parent is reported with its path: {dangling:?}"
    );

    // An orphan (parent missing) still shows up as a tree root so the UI cannot hide it.
    assert!(ids(index.roots()).contains(&"goal".to_string()));
    assert!(ids(index.roots()).contains(&"dup".to_string()));

    assert_eq!(
        ids(index.ancestors("block")),
        vec!["arch".to_string(), "goal".to_string()]
    );
    assert_eq!(
        ids(index.subtree("goal")),
        vec!["arch".to_string(), "block".to_string(), "deep".to_string()]
    );
    let neighbours = ids(index.neighbours("goal"));
    for expected in ["arch", "block"] {
        assert!(
            neighbours.contains(&expected.to_string()),
            "goal neighbours include {expected}: {neighbours:?}"
        );
    }
    assert!(
        !neighbours.contains(&"deep".to_string()),
        "neighbours are adjacent nodes, not the whole subtree"
    );
}

#[test]
fn read_returns_frontmatter_body_and_hash_and_writes_are_hash_guarded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let original =
        "---\nid: editable\ntype: module-design\ntitle: Editable\nstatus: draft\n---\n# Original\n";
    std::fs::write(root.join("editable.md"), original).unwrap();

    let doc = read_spec(root, "editable.md").expect("read spec");
    assert_eq!(doc.node.id, "editable");
    assert_eq!(doc.node.title.as_deref(), Some("Editable"));
    assert_eq!(doc.body, "# Original\n");
    assert_eq!(
        doc.hash,
        hex::encode(Sha256::digest(original.as_bytes())),
        "the hash is SHA-256 over the whole file, frontmatter included"
    );

    let expected_hash = doc.hash.clone();
    let saved =
        write_spec(root, "editable.md", "# Rewritten\n", &expected_hash).expect("first write wins");
    assert_eq!(saved.body, "# Rewritten\n");
    assert_ne!(saved.hash, expected_hash);
    let on_disk = std::fs::read_to_string(root.join("editable.md")).unwrap();
    assert_eq!(
        on_disk,
        "---\nid: editable\ntype: module-design\ntitle: Editable\nstatus: draft\n---\n# Rewritten\n"
    );

    let conflict = write_spec(root, "editable.md", "# Clobber\n", &expected_hash)
        .expect_err("stale hash is rejected");
    assert_eq!(conflict.code, grove_core::ipc::ErrorCode::Conflict);
    assert_eq!(
        std::fs::read_to_string(root.join("editable.md")).unwrap(),
        on_disk,
        "a rejected write leaves the file untouched"
    );

    let reread = read_spec(root, "editable.md").expect("re-read");
    let saved_again = write_spec(root, "editable.md", "# Second edit\n", &reread.hash)
        .expect("re-read hash is accepted");
    assert_eq!(saved_again.body, "# Second edit\n");
}

#[test]
fn read_and_write_reject_traversal_and_non_spec_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::write(root.join("plain.md"), "# no frontmatter\n").unwrap();

    let traversal = read_spec(root, "../outside.md").expect_err("traversal is refused");
    assert_eq!(traversal.code, grove_core::ipc::ErrorCode::Invalid);

    let missing = read_spec(root, "missing.md").expect_err("missing spec");
    assert_eq!(missing.code, grove_core::ipc::ErrorCode::NotFound);

    let no_frontmatter = read_spec(root, "plain.md").expect_err("not a spec node");
    assert_eq!(no_frontmatter.code, grove_core::ipc::ErrorCode::Invalid);
}

#[test]
fn watcher_handler_filters_events_without_filesystem_timing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().unwrap();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<SpecChange>::new()));
    let sink = {
        let seen = std::sync::Arc::clone(&seen);
        move |change: SpecChange| seen.lock().unwrap().push(change)
    };
    let watcher = SpecWatcher::new(&root, sink);

    let event = notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
        .add_path(root.join("architecture.md"))
        .add_path(root.join("node_modules/ignored.md"))
        .add_path(root.join("notes.txt"));
    watcher.handle_event(&event);

    let changes = seen.lock().unwrap().clone();
    assert_eq!(
        changes,
        vec![SpecChange {
            paths: vec!["architecture.md".to_string()]
        }]
    );
}

#[test]
fn watcher_notifies_on_a_spec_change_on_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (tx, rx) = std::sync::mpsc::channel::<SpecChange>();
    let sink = move |change: SpecChange| {
        let _ = tx.send(change);
    };
    let _watcher = SpecWatcher::start_with_debounce(dir.path(), sink, Duration::from_millis(100))
        .expect("start watcher");

    // Give the native backend a moment to install its watch before writing.
    std::thread::sleep(Duration::from_millis(300));
    write_spec_file(
        dir.path(),
        "watched.md",
        "id: watched\ntype: module-design",
        "# Watched\n",
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut changed = false;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(change) if change.paths.iter().any(|path| path == "watched.md") => {
                changed = true;
                break;
            }
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(changed, "watcher reported a change within 15s");
}
