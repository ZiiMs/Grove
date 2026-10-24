use grove_core::gitops::{
    DiffTarget, GitBranch, GitDiff, GitStatus, GitStatusFile, GitWorktree, PrResult,
};
use grove_core::ipc::{AppInfo, ErrorCode, EventEnvelope, IpcError};

/// Each fixture is canonical JSON produced by the Rust side. A change to a DTO without a matching
/// fixture change fails here, and the TS side fails too — that is the contract test.
fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract/fixtures")
        .join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read fixture {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("fixture is valid JSON")
}

#[test]
fn app_info_matches_fixture() {
    let sample = AppInfo {
        name: "Grove".into(),
        version: "0.1.0".into(),
        app_data_dir: "C:/Users/demo/AppData/Roaming/Grove".into(),
        platform: "windows".into(),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("app-info.json")
    );
}

#[test]
fn event_envelope_matches_fixture() {
    let sample = EventEnvelope::session(
        "session-1",
        serde_json::json!({ "kind": "status", "state": "idle" }),
    );
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("event-envelope.json")
    );
}

#[test]
fn ipc_error_matches_fixture() {
    let sample = IpcError {
        code: ErrorCode::Conflict,
        message: "plan changed on disk".into(),
        detail: Some("expected version 3".into()),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("ipc-error.json")
    );
}

fn sample_spec_node() -> grove_core::specgraph::SpecNode {
    grove_core::specgraph::SpecNode {
        id: "grove-architecture".into(),
        spec_type: "architecture-design".into(),
        title: Some("Grove — architecture".into()),
        status: Some("active".into()),
        path: "architecture.md".into(),
        parent: Some("grove-goal-and-requirements".into()),
        depends_on: vec!["grove-goal-and-requirements".into()],
        references: vec!["grove-ipc-contract".into()],
        implements: vec!["grove-goal-and-requirements".into()],
        covers: vec!["specgraph".into()],
    }
}

fn sample_spec_validation() -> grove_core::specgraph::SpecValidation {
    grove_core::specgraph::SpecValidation {
        dangling_links: vec![grove_core::specgraph::SpecDanglingLink {
            from: "grove-architecture".into(),
            path: "architecture.md".into(),
            field: "references".into(),
            target: "grove-missing-spec".into(),
        }],
        duplicate_ids: vec![grove_core::specgraph::SpecDuplicateId {
            id: "grove-spec".into(),
            paths: vec!["specs/one.md".into(), "specs/two.md".into()],
        }],
        parent_cycles: vec![grove_core::specgraph::SpecParentCycle {
            ids: vec!["cycle-a".into(), "cycle-b".into()],
            paths: vec!["specs/cycle-a.md".into(), "specs/cycle-b.md".into()],
        }],
    }
}

#[test]
fn spec_node_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_spec_node()).unwrap(),
        fixture("spec-node.json")
    );
}

#[test]
fn spec_validation_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_spec_validation()).unwrap(),
        fixture("spec-validation.json")
    );
}

#[test]
fn spec_graph_matches_fixture() {
    let sample = grove_core::specgraph::SpecGraph {
        root: "C:/demo/grove".into(),
        nodes: vec![sample_spec_node()],
        validation: grove_core::specgraph::SpecValidation::default(),
    };
    assert_eq!(
        serde_json::to_value(sample).unwrap(),
        fixture("spec-graph.json")
    );
}

#[test]
fn spec_doc_matches_fixture() {
    let sample = grove_core::specgraph::SpecDoc {
        node: sample_spec_node(),
        body: "# Grove — architecture\n\nSettles the open questions.\n".into(),
        hash: "9f2c4e1a8b7d6c5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e".into(),
    };
    assert_eq!(
        serde_json::to_value(sample).unwrap(),
        fixture("spec-doc.json")
    );
}

#[test]
fn spec_change_matches_fixture() {
    let sample = grove_core::specgraph::SpecChange {
        paths: vec!["architecture.md".into(), "goal-and-requirements.md".into()],
    };
    assert_eq!(
        serde_json::to_value(sample).unwrap(),
        fixture("spec-change.json")
    );
}
fn sample_git_status() -> GitStatus {
    GitStatus {
        repo_root: "C:/work/grove".into(),
        branch: Some("ticket/04-git-panel".into()),
        head: Some("9f2c1b7d4e5a6c8b0d1e2f3a4b5c6d7e8f901234".into()),
        clean: false,
        truncated: false,
        files: vec![
            GitStatusFile {
                path: "src/main.ts".into(),
                from: None,
                index: "M".into(),
                worktree: " ".into(),
                staged: true,
                untracked: false,
            },
            GitStatusFile {
                path: "docs/notes.md".into(),
                from: None,
                index: " ".into(),
                worktree: "M".into(),
                staged: false,
                untracked: false,
            },
            GitStatusFile {
                path: "src/renamed.ts".into(),
                from: Some("src/old.ts".into()),
                index: "R".into(),
                worktree: " ".into(),
                staged: true,
                untracked: false,
            },
            GitStatusFile {
                path: "scratch.txt".into(),
                from: None,
                index: "?".into(),
                worktree: "?".into(),
                staged: false,
                untracked: true,
            },
        ],
    }
}

#[test]
fn git_status_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_git_status()).unwrap(),
        fixture("git-status.json")
    );
}

#[test]
fn git_branch_matches_fixture() {
    let sample = GitBranch {
        name: "ticket/04-git-panel".into(),
        remote: None,
        current: true,
        upstream: Some("origin/ticket/04-git-panel".into()),
        ahead: 2,
        behind: 1,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-branch.json")
    );
}

#[test]
fn git_worktree_matches_fixture() {
    let sample = GitWorktree {
        path: "C:/work/grove".into(),
        head: Some("9f2c1b7d4e5a6c8b0d1e2f3a4b5c6d7e8f901234".into()),
        branch: Some("ticket/04-git-panel".into()),
        bare: false,
        detached: false,
        current: true,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-worktree.json")
    );
}

#[test]
fn git_diff_matches_fixture() {
    let sample = GitDiff {
        repo_root: "C:/work/grove".into(),
        target: DiffTarget::WorkingTree,
        path: Some("src/main.ts".into()),
        base: None,
        branch: None,
        patch: concat!(
            "diff --git a/src/main.ts b/src/main.ts\n",
            "index 1a2b3c4..5d6e7f8 100644\n",
            "--- a/src/main.ts\n",
            "+++ b/src/main.ts\n",
            "@@ -1,2 +1,2 @@\n",
            "-const answer = 41;\n",
            "+const answer = 42;\n",
            " console.log(answer);\n",
        )
        .into(),
        truncated: false,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-diff.json")
    );
}

#[test]
fn git_pr_matches_fixture() {
    let sample = PrResult {
        url: "https://github.com/acme/grove/pull/42".into(),
        draft: true,
        base: Some("main".into()),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-pr.json")
    );
}

#[test]
fn git_changed_event_matches_fixture() {
    let sample = EventEnvelope::global(sample_git_status());
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-changed.json")
    );
}
