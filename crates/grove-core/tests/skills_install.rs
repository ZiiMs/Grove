//! Skill targets and installation: atomic copy, new-vs-overwritten, traversal and symlink refusal,
//! install records, and the offline path served from the blob cache.

pub mod skills_support;

use grove_core::paths::display_path;
use grove_core::skills::{InstallRequest, SkillError};
use skills_support::{config, mount_repo, mount_skill, service, temp_paths, tree, REPO};
use std::path::Path;
use std::time::Duration;
use wiremock::MockServer;

const SKILL_MD: &str = "---\nname: pdf\ndescription: Create PDFs\n---\n# PDF\n";

fn pdf_tree() -> serde_json::Value {
    tree(&[
        ("skills/pdf/SKILL.md", "100644"),
        ("skills/pdf/scripts/render.py", "100644"),
    ])
}

fn request(path: &str, target: &str) -> InstallRequest {
    InstallRequest {
        source: REPO.to_string(),
        path: path.to_string(),
        target: target.to_string(),
        project_root: None,
    }
}

#[tokio::test]
async fn targets_list_the_user_directory_and_the_project_local_agents_dir() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    let mut config = config(&server);
    let home = dir.path().join("home");
    config.home = Some(home.clone());
    std::fs::create_dir_all(home.join(".agents/skills")).unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir_all(project.join(".agents/skills")).unwrap();
    let service = service(paths, config);

    let targets = service.targets(Some(&project));

    let ids: Vec<&str> = targets.iter().map(|target| target.id.as_str()).collect();
    // Grove runs the agent itself, so a skill is installed where Grove reads it: the user's global
    // folder and the project's `.agents/skills`.
    assert_eq!(ids, ["user", "agents"]);
    assert_eq!(targets[0].path, display_path(&home.join(".agents/skills")));
    assert_eq!(
        targets[1].path,
        display_path(&project.join(".agents/skills"))
    );
    let exists: Vec<bool> = targets.iter().map(|target| target.exists).collect();
    assert_eq!(exists, [true, true]);
}

#[tokio::test]
async fn detection_finds_skills_grove_did_not_install() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    let mut config = config(&server);
    let home = dir.path().join("home");
    config.home = Some(home.clone());
    let project = dir.path().join("project");

    let user_pdf = home.join(".agents/skills/pdf");
    std::fs::create_dir_all(&user_pdf).unwrap();
    std::fs::write(user_pdf.join("SKILL.md"), SKILL_MD).unwrap();

    let project_orphan = project.join(".agents/skills/orphan");
    std::fs::create_dir_all(&project_orphan).unwrap();
    std::fs::write(project_orphan.join("SKILL.md"), SKILL_MD).unwrap();

    // A folder without SKILL.md and an in-flight staging folder are not skills.
    std::fs::create_dir_all(project.join(".agents/skills/not-a-skill")).unwrap();
    let staging = project.join(".agents/skills/.grove-install-abc");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("SKILL.md"), SKILL_MD).unwrap();

    let service = service(paths, config);
    let found = service.detected(Some(&project));

    let rows: Vec<(&str, &str)> = found
        .iter()
        .map(|skill| (skill.name.as_str(), skill.target.as_str()))
        .collect();
    assert_eq!(rows, [("pdf", "user"), ("orphan", "agents")]);
    assert_eq!(found[0].path, display_path(&user_pdf));
    assert_eq!(found[1].path, display_path(&project_orphan));

    // A folder without SKILL.md is not removable, even inside a target.
    let not_a_skill = project.join(".agents/skills/not-a-skill");
    let error = service
        .remove_skill(&display_path(&not_a_skill), Some(&project))
        .expect_err("a folder without SKILL.md is refused");
    assert!(matches!(error, SkillError::Invalid(_)));
    assert!(not_a_skill.exists());

    // A detected skill can be removed even though no install record exists for it.
    service
        .remove_skill(&found[0].path, Some(&project))
        .expect("remove the detected skill");
    assert!(!user_pdf.exists(), "the detected folder is gone");
    assert!(service.installs().expect("records").is_empty());
}

#[cfg(windows)]
#[tokio::test]
async fn removal_clears_read_only_files_on_windows() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    let mut config = config(&server);
    let home = dir.path().join("home");
    config.home = Some(home.clone());
    let skill = home.join(".agents/skills/pdf");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), SKILL_MD).unwrap();
    let readonly = skill.join("notes.txt");
    std::fs::write(&readonly, "keep me").unwrap();
    let mut permissions = std::fs::metadata(&readonly).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&readonly, permissions).unwrap();

    let service = service(paths, config);
    service
        .remove_skill(&display_path(&skill), None)
        .expect("a read-only file must not block removal");
    assert!(!skill.exists(), "the whole skill folder is gone");
}

#[tokio::test]
async fn install_copies_the_skill_folder_atomically_and_reports_new_then_overwritten() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(&server, REPO, "main", pdf_tree()).await;
    mount_skill(
        &server,
        REPO,
        "skills/pdf",
        &[
            ("SKILL.md", SKILL_MD),
            ("scripts/render.py", "print('pdf')\n"),
        ],
    )
    .await;
    let mut config = config(&server);
    let home = dir.path().join("home");
    config.home = Some(home.clone());
    let service = service(paths, config);

    let result = service
        .install(request("skills/pdf", "user"))
        .await
        .expect("install");
    let destination = home.join(".agents/skills/pdf");
    assert_eq!(result.skill, "pdf");
    assert_eq!(result.source, REPO);
    assert_eq!(result.target, "user");
    assert_eq!(result.path, display_path(&destination));
    assert!(!result.overwritten, "first install is a new skill");
    assert!(!result.installed_at.is_empty());
    assert_eq!(
        std::fs::read_to_string(destination.join("SKILL.md")).unwrap(),
        SKILL_MD
    );
    assert_eq!(
        std::fs::read_to_string(destination.join("scripts/render.py")).unwrap(),
        "print('pdf')\n"
    );

    // A second install replaces the whole folder: files that are gone upstream do not linger.
    std::fs::write(destination.join("stale.txt"), "old").unwrap();
    let again = service
        .install(request("skills/pdf", "user"))
        .await
        .expect("reinstall");
    assert!(again.overwritten, "second install overwrites");
    assert!(
        !destination.join("stale.txt").exists(),
        "atomic replace drops stale files"
    );

    let records = service.installs().expect("install records");
    assert_eq!(records.len(), 1, "one record per skill + target + path");
    assert_eq!(records[0].skill, "pdf");
    assert_eq!(records[0].target, "user");
    assert_eq!(records[0].path, display_path(&destination));
}

#[tokio::test]
async fn install_into_the_project_local_agents_directory() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(&server, REPO, "main", pdf_tree()).await;
    mount_skill(
        &server,
        REPO,
        "skills/pdf",
        &[("SKILL.md", SKILL_MD), ("scripts/render.py", "")],
    )
    .await;
    let project = dir.path().join("project");
    let service = service(paths, config(&server));

    let mut install = request("skills/pdf", "agents");
    install.project_root = Some(project.clone());
    let result = service.install(install).await.expect("install");

    assert_eq!(
        result.path,
        display_path(&project.join(".agents/skills/pdf"))
    );
    assert!(project.join(".agents/skills/pdf/SKILL.md").is_file());
}

#[tokio::test]
async fn install_refuses_traversal_absolute_paths_and_unknown_targets() {
    let (_dir, paths) = temp_paths();
    let server = MockServer::start().await;
    let service = service(paths, config(&server));

    for path in ["../evil", "skills/../../evil", "/etc/passwd"] {
        let err = service
            .install(request(path, "user"))
            .await
            .expect_err(&format!("{path} must be refused"));
        assert!(
            matches!(err, SkillError::Invalid(_)),
            "{path} should be invalid, got {err:?}"
        );
    }

    let err = service
        .install(request("skills/pdf", "not-a-provider"))
        .await
        .expect_err("unknown target");
    assert!(matches!(err, SkillError::Invalid(_)));
}

#[tokio::test]
async fn install_refuses_a_symlink_in_the_source_tree() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(
        &server,
        REPO,
        "main",
        tree(&[
            ("skills/bad/SKILL.md", "100644"),
            ("skills/bad/escape", "120000"),
        ]),
    )
    .await;
    mount_skill(&server, REPO, "skills/bad", &[("SKILL.md", SKILL_MD)]).await;
    let mut config = config(&server);
    config.home = Some(dir.path().join("home"));
    let service = service(paths, config);

    let err = service
        .install(request("skills/bad", "user"))
        .await
        .expect_err("symlink in a skill must be refused");
    assert!(
        format!("{err}").contains("symlink"),
        "error should name the symlink: {err}"
    );
    assert!(!dir.path().join("home/.agents/skills/bad").exists());
}

#[cfg(windows)]
#[tokio::test]
async fn install_refuses_a_junctioned_destination_that_escapes_the_target() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(&server, REPO, "main", pdf_tree()).await;
    mount_skill(
        &server,
        REPO,
        "skills/pdf",
        &[("SKILL.md", SKILL_MD), ("scripts/render.py", "")],
    )
    .await;

    let home = dir.path().join("home");
    let target_root = home.join(".agents/skills");
    std::fs::create_dir_all(&target_root).unwrap();
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let destination = target_root.join("pdf");
    let command = format!(
        "mklink /J \"{}\" \"{}\"",
        destination.display(),
        outside.display()
    );
    use std::os::windows::process::CommandExt;
    let status = std::process::Command::new("cmd")
        .arg("/C")
        .raw_arg(&command)
        .status()
        .expect("run mklink");
    assert!(status.success(), "junction setup failed; test needs mklink");
    assert!(destination.exists());

    let mut config = config(&server);
    config.home = Some(home);
    let service = service(paths, config);

    let err = service
        .install(request("skills/pdf", "user"))
        .await
        .expect_err("junction escape must be refused");
    assert!(matches!(err, SkillError::Invalid(_)), "got {err:?}");
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        0,
        "nothing may be written through the junction"
    );
}

#[tokio::test]
async fn offline_install_reuses_the_cached_blobs() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(&server, REPO, "main", pdf_tree()).await;
    mount_skill(
        &server,
        REPO,
        "skills/pdf",
        &[
            ("SKILL.md", SKILL_MD),
            ("scripts/render.py", "print('pdf')\n"),
        ],
    )
    .await;
    let mut config = config(&server);
    config.ttl = Duration::ZERO;
    let home = dir.path().join("home");
    config.home = Some(home.clone());
    let service = service(paths.clone(), config);

    service
        .install(request("skills/pdf", "user"))
        .await
        .expect("online install");
    std::fs::remove_dir_all(home.join(".agents/skills/pdf")).unwrap();
    assert!(!Path::new(&home.join(".agents/skills/pdf")).exists());

    drop(server);

    let offline = service
        .install(request("skills/pdf", "user"))
        .await
        .expect("offline install");
    assert!(!offline.overwritten);
    assert_eq!(
        std::fs::read_to_string(home.join(".agents/skills/pdf/scripts/render.py")).unwrap(),
        "print('pdf')\n"
    );
    // The blob cache is under the app-data dir; nothing is fetched from the internet in tests.
    assert!(paths.cache.join("skills").is_dir());
}

#[tokio::test]
async fn removing_a_deleted_recorded_skill_only_clears_the_record() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(&server, REPO, "main", pdf_tree()).await;
    mount_skill(
        &server,
        REPO,
        "skills/pdf",
        &[("SKILL.md", SKILL_MD), ("scripts/render.py", "")],
    )
    .await;
    let mut config = config(&server);
    let home = dir.path().join("home");
    config.home = Some(home.clone());
    let service = service(paths, config);

    let result = service
        .install(request("skills/pdf", "user"))
        .await
        .expect("install");
    let destination = home.join(".agents/skills/pdf");
    assert_eq!(service.installs().expect("records").len(), 1);

    // Simulate an external delete or a removed worktree: the record outlives the folder.
    std::fs::remove_dir_all(&destination).unwrap();
    service
        .remove_skill(&result.path, None)
        .expect("a stale record is cleared without touching the filesystem");
    assert!(service.installs().expect("records").is_empty());
    assert!(!destination.exists());

    // An unrecorded path that is also missing remains a not-found error.
    let missing = display_path(&home.join(".agents/skills/missing"));
    let error = service
        .remove_skill(&missing, None)
        .expect_err("an unrecorded missing path is not a stale record");
    assert!(matches!(error, SkillError::NotFound(_)));
}

#[tokio::test]
async fn removing_an_installed_skill_deletes_its_folder_and_record() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(&server, REPO, "main", pdf_tree()).await;
    mount_skill(
        &server,
        REPO,
        "skills/pdf",
        &[
            ("SKILL.md", SKILL_MD),
            ("scripts/render.py", "print('pdf')\n"),
        ],
    )
    .await;
    let mut config = config(&server);
    let home = dir.path().join("home");
    config.home = Some(home.clone());
    let service = service(paths, config);

    let result = service
        .install(request("skills/pdf", "user"))
        .await
        .expect("install");
    let destination = home.join(".agents/skills/pdf");
    assert!(destination.is_dir());

    service
        .remove_skill(&result.path, None)
        .expect("remove the installed skill");
    assert!(!destination.exists(), "the folder is gone");
    assert!(service.installs().expect("records").is_empty());

    let error = service
        .remove_skill("C:/Windows/system32", None)
        .expect_err("an unrecorded path is refused");
    assert!(matches!(
        error,
        SkillError::NotFound(_) | SkillError::Invalid(_)
    ));
}
