//! A session created inside a project runs in that project's checkout; a loose chat keeps the old
//! cwd rule. This is the join between the workspace tree and the session runtime.

use grove_core::ipc::CreateSessionRequest;
use grove_core::paths::AppPaths;
use grove_core::projects::AddProjectRequest;
use grove_core::sessions::manager::SessionError;
use grove_core::GroveCore;

fn core() -> (GroveCore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let core = GroveCore::new(AppPaths::new(dir.path().join("app-data")))
        .with_project_root(dir.path().join("project"));
    (core, dir)
}

fn add_project(core: &GroveCore, dir: &tempfile::TempDir) -> grove_core::ipc::Project {
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).expect("project folder");
    core.projects
        .add(AddProjectRequest {
            path: folder.to_string_lossy().to_string(),
            ..AddProjectRequest::default()
        })
        .expect("add project")
}

#[tokio::test]
async fn a_session_in_a_project_gets_the_project_path_and_id() {
    let (core, dir) = core();
    let project = add_project(&core, &dir);

    let detail = core
        .sessions
        .create(CreateSessionRequest {
            provider: "mock".into(),
            project_id: Some(project.id.clone()),
            title: Some("Inside the project".into()),
            ..CreateSessionRequest::default()
        })
        .await
        .expect("create in project");

    assert_eq!(detail.session.cwd, project.path);
    assert_eq!(
        detail.session.project_id.as_deref(),
        Some(project.id.as_str())
    );
    assert_eq!(detail.session.title, "Inside the project");

    core.sessions.close(&detail.session.id).await.ok();
}

#[tokio::test]
async fn a_session_runs_in_the_named_worktree() {
    let (core, dir) = core();
    let project = add_project(&core, &dir);
    let worktree = dir.path().join("project-wt");
    std::fs::create_dir_all(&worktree).expect("worktree folder");

    let detail = core
        .sessions
        .create(CreateSessionRequest {
            provider: "mock".into(),
            project_id: Some(project.id.clone()),
            worktree_path: Some(worktree.to_string_lossy().to_string()),
            ..CreateSessionRequest::default()
        })
        .await
        .expect("create in worktree");

    assert_eq!(
        detail.session.cwd,
        grove_core::paths::display_path(&worktree)
    );
    core.sessions.close(&detail.session.id).await.ok();
}

#[tokio::test]
async fn an_unknown_project_is_rejected_before_a_session_row_exists() {
    let (core, _dir) = core();
    let error = core
        .sessions
        .create(CreateSessionRequest {
            provider: "mock".into(),
            project_id: Some("missing".into()),
            ..CreateSessionRequest::default()
        })
        .await
        .expect_err("unknown project");
    assert!(matches!(error, SessionError::Invalid(_)));
    assert!(core.sessions.list().expect("list").is_empty());
}

#[tokio::test]
async fn a_loose_chat_has_no_project() {
    let (core, dir) = core();
    let cwd = dir.path().join("loose");
    std::fs::create_dir_all(&cwd).expect("loose folder");

    let detail = core
        .sessions
        .create(CreateSessionRequest {
            provider: "mock".into(),
            cwd: Some(cwd.to_string_lossy().to_string()),
            ..CreateSessionRequest::default()
        })
        .await
        .expect("create loose");

    assert_eq!(detail.session.project_id, None);
    assert_eq!(detail.session.cwd, grove_core::paths::display_path(&cwd));
    core.sessions.close(&detail.session.id).await.ok();
}
