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
