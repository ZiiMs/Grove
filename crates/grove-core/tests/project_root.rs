//! One notion of "the project".
//!
//! A session's `cwd` is the project the user means: the spec graph, the plan mirror's fallback, MCP
//! discovery and the project-local skills target all have to follow it. These tests pin that moving
//! the root moves all of them, and that `GROVE_PROJECT_ROOT` is only the starting point.

use grove_core::paths::{display_path, AppPaths};
use grove_core::GroveCore;

fn spec(id: &str, title: &str) -> String {
    format!(
        "---\nid: {id}\ntype: goal-and-requirements\ntitle: {title}\n---\n\n# {title}\n\nBody.\n"
    )
}

fn project(specs: &[(&str, &str)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("temp dir");
    for (file, body) in specs {
        std::fs::write(directory.path().join(file), body).expect("write a spec");
    }
    directory
}

fn core(directory: &tempfile::TempDir) -> GroveCore {
    GroveCore::new(AppPaths::new(directory.path().join("app-data")))
}

fn ids(core: &GroveCore) -> Vec<String> {
    let mut ids: Vec<String> = core
        .spec_index()
        .expect("the project indexes")
        .nodes
        .into_iter()
        .map(|node| node.id)
        .collect();
    ids.sort();
    ids
}

#[test]
fn setting_the_project_root_moves_what_the_spec_graph_indexes() {
    let first = project(&[("a.md", &spec("alpha", "Alpha"))]);
    let second = project(&[("b.md", &spec("beta", "Beta"))]);
    let core = core(&first);

    core.set_project_root(first.path());
    assert_eq!(ids(&core), ["alpha"]);
    let canonical = std::fs::canonicalize(first.path()).expect("canonical project");
    let expected = display_path(&canonical);
    let expected = expected
        .strip_prefix("//?/")
        .unwrap_or(&expected)
        .to_string();
    assert_eq!(
        core.spec_index().unwrap().root,
        expected,
        "the graph reports the project it indexed, so the panel can show it"
    );

    core.set_project_root(second.path());
    assert_eq!(
        ids(&core),
        ["beta"],
        "the index follows the active session instead of the app's own directory"
    );
}

#[test]
fn the_project_local_skill_target_follows_the_project_root() {
    let first = project(&[]);
    let second = project(&[]);
    let core = core(&first);
    core.set_project_root(second.path());

    let agents = core
        .skill_targets(None)
        .into_iter()
        .find(|target| target.id == "agents")
        .expect("the project-local target always exists");
    assert_eq!(
        agents.path,
        display_path(&second.path().join(".agents/skills")),
        "writing `Project .agents/skills` must land in the repository, not beside the executable"
    );

    // An explicit root still wins: the UI may name a repository it is not otherwise looking at.
    let explicit = core
        .skill_targets(Some(first.path()))
        .into_iter()
        .find(|target| target.id == "agents")
        .expect("the project-local target always exists");
    assert_eq!(
        explicit.path,
        display_path(&first.path().join(".agents/skills"))
    );
}

#[test]
fn mcp_discovery_reads_the_project_root_it_was_pointed_at() {
    let first = project(&[]);
    let second = project(&[(
        ".mcp.json",
        r#"{"mcpServers":{"second-only":{"command":"node","args":["server.mjs"]}}}"#,
    )]);
    let core = core(&first);
    core.set_project_root(first.path());
    assert!(
        core.mcp
            .discover()
            .servers
            .iter()
            .all(|server| server.name != "second-only"),
        "the first project has no MCP config"
    );

    core.set_project_root(second.path());
    assert!(
        core.mcp
            .discover()
            .servers
            .iter()
            .any(|server| server.name == "second-only"),
        "MCP config is read from the project Grove is looking at"
    );
}
