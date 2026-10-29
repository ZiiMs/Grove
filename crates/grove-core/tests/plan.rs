//! Plan dual-edit behaviour (ticket 09): a versioned record in SQLite and a mirror file in the
//! session's project that the agent can edit.
//!
//! Every test runs against a tempdir project. The mirror is a real file, because the whole point of
//! the design is that a human and an agent both write real bytes; the import decision itself is
//! tested as a pure function next to `plan::mirror`.

use grove_core::ipc::{ErrorCode, Session};
use grove_core::paths::display_path;
use grove_core::plan::{PlanService, PlanUpdate};
use grove_core::sessions::event::SessionState;
use grove_core::store::Store;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn session_row(id: &str, project: &Path) -> Session {
    Session {
        id: id.into(),
        provider: "mock".into(),
        cwd: display_path(project),
        model: None,
        title: "Plan fixture".into(),
        status: SessionState::Idle,
        project_id: None,
        awaiting: false,
        archived_at: None,
        pinned: false,
        title_source: grove_core::sessions::event::TitleSource::Manual,
        created_at: "2025-01-01T00:00:00.000Z".into(),
        updated_at: "2025-01-01T00:00:00.000Z".into(),
    }
}

fn open_store(db: &Path) -> Arc<Store> {
    Arc::new(Store::open(db).expect("open the durable store"))
}

fn seed_session(store: &Store, id: &str, project: &Path) {
    store
        .insert_session(&session_row(id, project), &serde_json::json!({}))
        .expect("insert the session row");
}

fn mirror_of(project: &Path, session_id: &str) -> PathBuf {
    project
        .join(".grove")
        .join("sessions")
        .join(session_id)
        .join("plan.md")
}

fn directory_entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .expect("list the mirror directory")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn set_writes_the_record_and_an_atomic_mirror_creating_directories() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    seed_session(&store, "session-1", dir.path());
    let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());

    let plan = service
        .set("session-1", PlanUpdate::new("# Plan\n\n- one\n", 0))
        .expect("the first write is accepted");

    assert_eq!(plan.version, 1);
    assert_eq!(plan.markdown, "# Plan\n\n- one\n");
    let mirror = mirror_of(dir.path(), "session-1");
    assert_eq!(plan.mirror_path, display_path(&mirror));
    assert_eq!(
        std::fs::read_to_string(&mirror).unwrap(),
        "# Plan\n\n- one\n",
        "every write is mirrored into the session's project"
    );
    assert_eq!(
        directory_entries(mirror.parent().unwrap()),
        vec!["plan.md".to_string()],
        "the temp file is renamed, never left behind"
    );
    assert!(
        mirror.starts_with(dir.path()),
        "the mirror stays in the project"
    );
}

#[test]
fn a_stale_write_returns_conflict_and_leaves_the_record_alone() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    seed_session(&store, "session-1", dir.path());
    let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());

    service
        .set("session-1", PlanUpdate::new("first\n", 0))
        .unwrap();
    let stale = service
        .set("session-1", PlanUpdate::new("stale\n", 0))
        .unwrap_err();

    assert_eq!(stale.code, ErrorCode::Conflict);
    let current = service.get("session-1").unwrap();
    assert_eq!(current.version, 1);
    assert_eq!(current.markdown, "first\n");
    assert_eq!(
        std::fs::read_to_string(mirror_of(dir.path(), "session-1")).unwrap(),
        "first\n"
    );
}

#[test]
fn two_concurrent_writers_cannot_both_win() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    seed_session(&store, "session-1", dir.path());
    let service = Arc::new(PlanService::new(
        Arc::clone(&store),
        dir.path().to_path_buf(),
    ));
    service
        .set("session-1", PlanUpdate::new("base\n", 0))
        .unwrap();

    let writers: Vec<_> = (0..8)
        .map(|index| {
            let service = Arc::clone(&service);
            std::thread::spawn(move || {
                service.set("session-1", PlanUpdate::new(format!("writer {index}\n"), 1))
            })
        })
        .collect();
    let results: Vec<_> = writers
        .into_iter()
        .map(|writer| writer.join().expect("writer thread"))
        .collect();

    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "exactly one writer at version 1 may move the plan to version 2"
    );
    for result in results.iter().filter(|result| result.is_err()) {
        assert_eq!(result.as_ref().unwrap_err().code, ErrorCode::Conflict);
    }

    let plan = service.get("session-1").unwrap();
    assert_eq!(plan.version, 2);
    assert!(
        plan.markdown.starts_with("writer "),
        "the winner's content is the record: {}",
        plan.markdown
    );
    assert_eq!(
        std::fs::read_to_string(mirror_of(dir.path(), "session-1")).unwrap(),
        plan.markdown,
        "the mirror holds exactly what the record holds"
    );
}

#[tokio::test]
async fn an_external_edit_is_imported_on_read_and_announced() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    seed_session(&store, "session-1", dir.path());
    let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());
    service
        .set("session-1", PlanUpdate::new("# Plan\n\n- one\n", 0))
        .unwrap();
    let mut events = service.subscribe();

    let agent = "# Plan\n\n- one\n- two (agent)\n";
    std::fs::write(mirror_of(dir.path(), "session-1"), agent).unwrap();

    let plan = service.get("session-1").unwrap();
    assert_eq!(plan.version, 2, "the import bumps the version");
    assert_eq!(plan.markdown, agent);

    let envelope = events.try_recv().expect("the import is announced");
    assert_eq!(envelope.session_id.as_deref(), Some("session-1"));
    assert_eq!(envelope.payload, plan);

    assert_eq!(
        store.plan_row("session-1").unwrap().unwrap().markdown,
        agent,
        "the import is written to the record"
    );
    assert_eq!(
        service.get("session-1").unwrap().version,
        2,
        "a read that finds nothing new does not bump the version"
    );
}

#[test]
fn an_agent_file_is_adopted_when_the_record_has_no_plan_yet() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    seed_session(&store, "session-1", dir.path());
    let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());
    let mirror = mirror_of(dir.path(), "session-1");
    std::fs::create_dir_all(mirror.parent().unwrap()).unwrap();
    std::fs::write(&mirror, "agent first\n").unwrap();

    let plan = service.get("session-1").unwrap();
    assert_eq!(plan.version, 1);
    assert_eq!(plan.markdown, "agent first\n");
}

#[test]
fn a_save_after_an_agent_edit_is_a_conflict_not_a_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    seed_session(&store, "session-1", dir.path());
    let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());
    service
        .set("session-1", PlanUpdate::new("human v1\n", 0))
        .unwrap();

    std::fs::write(mirror_of(dir.path(), "session-1"), "agent v2\n").unwrap();
    let stale = service
        .set("session-1", PlanUpdate::new("human edited\n", 1))
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::Conflict);

    let plan = service.get("session-1").unwrap();
    assert_eq!(plan.version, 2);
    assert_eq!(
        plan.markdown, "agent v2\n",
        "the agent's edit is imported before the human's stale save is rejected"
    );
}

#[test]
fn a_missing_mirror_is_recreated_from_the_record_without_bumping_the_version() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    seed_session(&store, "session-1", dir.path());
    let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());
    service
        .set("session-1", PlanUpdate::new("keep me\n", 0))
        .unwrap();

    std::fs::remove_file(mirror_of(dir.path(), "session-1")).unwrap();
    let plan = service.get("session-1").unwrap();

    assert_eq!(plan.version, 1);
    assert_eq!(
        std::fs::read_to_string(mirror_of(dir.path(), "session-1")).unwrap(),
        "keep me\n",
        "the record is authoritative when the file disappears"
    );
}

#[test]
fn the_plan_survives_a_restart_including_an_edit_made_while_closed() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("grove.db");
    {
        let store = open_store(&db);
        seed_session(&store, "session-1", dir.path());
        let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());
        service
            .set("session-1", PlanUpdate::new("human v1\n", 0))
            .unwrap();
    }

    // Grove is not running; the agent edits the mirror file.
    let mirror = mirror_of(dir.path(), "session-1");
    std::fs::write(&mirror, "human v1\nagent v2\n").unwrap();

    {
        let store = open_store(&db);
        let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());
        let plan = service.get("session-1").unwrap();
        assert_eq!(plan.version, 2);
        assert_eq!(plan.markdown, "human v1\nagent v2\n");
        assert_eq!(std::fs::read_to_string(&mirror).unwrap(), plan.markdown);
    }

    {
        let store = open_store(&db);
        let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());
        let plan = service.get("session-1").unwrap();
        assert_eq!(plan.version, 2, "the import is durable, not repeated");
        assert_eq!(plan.markdown, "human v1\nagent v2\n");
    }
}

#[test]
fn the_session_start_path_reads_the_current_plan_text() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    seed_session(&store, "session-1", dir.path());
    let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());

    assert_eq!(
        service.text("session-1").unwrap(),
        None,
        "an empty plan is nothing to put in a prompt"
    );
    service
        .set("session-1", PlanUpdate::new("# Plan\n\n- ship\n", 0))
        .unwrap();
    assert_eq!(
        service.text("session-1").unwrap().as_deref(),
        Some("# Plan\n\n- ship\n")
    );

    std::fs::write(mirror_of(dir.path(), "session-1"), "agent text\n").unwrap();
    assert_eq!(
        service.text("session-1").unwrap().as_deref(),
        Some("agent text\n"),
        "the accessor sees the imported edit, not a stale cache"
    );
}

#[test]
fn unknown_sessions_are_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());

    assert_eq!(
        service.get("missing").unwrap_err().code,
        ErrorCode::NotFound
    );
    assert_eq!(
        service
            .set("missing", PlanUpdate::new("x", 0))
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    assert_eq!(
        service.text("missing").unwrap_err().code,
        ErrorCode::NotFound
    );
}

#[test]
fn the_mirror_lives_in_the_session_project_not_the_app_directory() {
    let app = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let store = open_store(&app.path().join("grove.db"));
    seed_session(&store, "session-1", project.path());
    let service = PlanService::new(Arc::clone(&store), app.path().to_path_buf());

    let plan = service
        .set("session-1", PlanUpdate::new("in project\n", 0))
        .unwrap();

    assert_eq!(
        plan.mirror_path,
        display_path(&mirror_of(project.path(), "session-1"))
    );
    assert!(mirror_of(project.path(), "session-1").exists());
    assert!(!mirror_of(app.path(), "session-1").exists());
}

#[test]
fn a_record_that_moved_past_the_mirror_wins_and_the_file_is_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_store(&dir.path().join("grove.db"));
    seed_session(&store, "session-1", dir.path());
    let service = PlanService::new(Arc::clone(&store), dir.path().to_path_buf());
    service
        .set("session-1", PlanUpdate::new("one\n", 0))
        .unwrap();
    let mirror = mirror_of(dir.path(), "session-1");

    // Simulate a crash between the record write and the mirror write: the record is at version 2 but
    // its `mirror_hash` still describes version 1's content. The agent then edits the stale file.
    store
        .write_plan(
            "session-1",
            "record moved\n",
            1,
            &grove_core::plan::mirror::content_hash(b"record moved\n"),
        )
        .unwrap();
    store
        .record_mirror_hash(
            "session-1",
            2,
            &grove_core::plan::mirror::content_hash(b"one\n"),
        )
        .unwrap();
    std::fs::write(&mirror, "agent edit based on v1\n").unwrap();

    let plan = service.get("session-1").unwrap();
    assert_eq!(
        plan.version, 2,
        "the agent's edit is not imported: the record moved since the last mirror write"
    );
    assert_eq!(plan.markdown, "record moved\n");
    assert_eq!(
        std::fs::read_to_string(&mirror).unwrap(),
        "record moved\n",
        "the stale mirror is rewritten from the record"
    );
}
