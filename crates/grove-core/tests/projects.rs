//! Workspace projects against a real temp directory and a real git repository.

use grove_core::gitops::Git;
use grove_core::ipc::Session;
use grove_core::projects::{list_dirs, AddProjectRequest, ProjectService};
use grove_core::sessions::event::SessionState;
use grove_core::store::Store;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tempfile::TempDir;

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo() -> TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    git(dir.path(), &["init", "--quiet", "--initial-branch=main"]);
    git(dir.path(), &["config", "user.email", "tester@grove.test"]);
    git(dir.path(), &["config", "user.name", "Grove Tester"]);
    std::fs::write(dir.path().join("README.md"), "hello\n").expect("write");
    git(dir.path(), &["add", "--all"]);
    git(dir.path(), &["commit", "--quiet", "--message", "init"]);
    git(
        dir.path(),
        &["remote", "add", "origin", "git@github.com:acme/grove.git"],
    );
    dir
}

fn service() -> (ProjectService, Arc<Store>) {
    let store = Arc::new(Store::open_in_memory().expect("store"));
    (
        ProjectService::new(Arc::clone(&store), Git::system()),
        store,
    )
}

fn request(path: &Path) -> AddProjectRequest {
    AddProjectRequest {
        path: path.to_string_lossy().to_string(),
        ..AddProjectRequest::default()
    }
}

fn session(id: &str, cwd: &Path) -> Session {
    Session {
        id: id.into(),
        provider: "mock".into(),
        cwd: cwd.to_string_lossy().replace('\\', "/"),
        model: None,
        title: "Existing".into(),
        status: SessionState::Idle,
        project_id: None,
        awaiting: false,
        created_at: "2025-01-01T00:00:00.000Z".into(),
        updated_at: "2025-01-01T00:00:00.000Z".into(),
    }
}

#[test]
fn adding_a_repository_stores_its_git_facts() {
    let dir = repo();
    let (projects, _) = service();

    let project = projects.add(request(dir.path())).expect("add");
    assert!(project.is_git);
    assert_eq!(
        project.name,
        dir.path().file_name().unwrap().to_str().unwrap()
    );
    assert_eq!(project.default_branch.as_deref(), Some("main"));
    assert_eq!(project.remote.as_deref(), Some("acme/grove"));

    let listed = projects.list().expect("list");
    assert_eq!(listed, vec![project]);
}

#[test]
fn a_plain_folder_is_added_without_git_facts_and_duplicates_are_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (projects, _) = service();

    let project = projects.add(request(dir.path())).expect("add");
    assert!(!project.is_git);
    assert_eq!(project.default_branch, None);
    assert_eq!(project.remote, None);

    let error = projects
        .add(request(dir.path()))
        .expect_err("the same path is refused");
    assert!(
        error.to_string().contains("already a project")
            || error.to_string().contains("already exists")
    );
}

#[test]
fn an_existing_session_in_the_project_is_linked() {
    let dir = repo();
    let (projects, store) = service();
    store
        .insert_session(&session("session-1", dir.path()), &serde_json::json!({}))
        .expect("insert");

    let project = projects.add(request(dir.path())).expect("add");
    let row = store.session("session-1").expect("read").expect("present");
    assert_eq!(row.project_id.as_deref(), Some(project.id.as_str()));
}

#[test]
fn removing_a_project_keeps_its_sessions_and_the_folder() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (projects, store) = service();
    store
        .insert_session(&session("session-1", dir.path()), &serde_json::json!({}))
        .expect("insert");
    let project = projects.add(request(dir.path())).expect("add");

    projects.remove(&project.id).expect("remove");

    assert!(projects.list().expect("list").is_empty());
    let row = store.session("session-1").expect("read").expect("present");
    assert_eq!(
        row.project_id, None,
        "a removed project leaves its sessions as chats"
    );
    assert!(dir.path().is_dir(), "the working tree is untouched");
}

#[test]
fn removing_an_unknown_project_is_not_found() {
    let (projects, _) = service();
    let error = projects
        .remove("missing")
        .expect_err("unknown projects are refused");
    assert_eq!(error.to_string(), "project not found: missing");
}

#[test]
fn detect_and_list_dirs_only_report_directories() {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir(root.path().join("alpha")).expect("dir");
    std::fs::create_dir(root.path().join(".hidden")).expect("dir");
    std::fs::write(root.path().join("file.txt"), "x").expect("file");

    let listing = list_dirs(Some(root.path().to_str().unwrap())).expect("list");
    assert_eq!(
        listing
            .dirs
            .iter()
            .map(|dir| dir.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha"]
    );

    let (projects, _) = service();
    let detection = projects
        .detect(root.path().to_str().unwrap())
        .expect("detect");
    assert!(!detection.is_git);
}
