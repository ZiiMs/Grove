//! Skill targets and installation: atomic copy, new-vs-overwritten, traversal and symlink refusal,
//! install records, and the offline path served from the blob cache.

mod skills_support;

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
async fn targets_list_every_harness_directory_and_the_project_local_agents_dir() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    let mut config = config(&server);
    let home = dir.path().join("home");
    config.home = Some(home.clone());
    std::fs::create_dir_all(home.join(".claude/skills")).unwrap();
    std::fs::create_dir_all(home.join(".codex/skills")).unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir_all(project.join(".agents/skills")).unwrap();
    let service = service(paths, config);

    let targets = service.targets(Some(&project));

    let ids: Vec<&str> = targets.iter().map(|target| target.id.as_str()).collect();
    assert_eq!(ids, ["claude", "codex", "pi", "opencode", "agents"]);
    assert_eq!(targets[0].path, display_path(&home.join(".claude/skills")));
    assert_eq!(targets[1].path, display_path(&home.join(".codex/skills")));
    assert_eq!(targets[2].path, display_path(&home.join(".pi/skills")));
    assert_eq!(
        targets[3].path,
        display_path(&home.join(".config/opencode/skills"))
    );
    assert_eq!(
        targets[4].path,
        display_path(&project.join(".agents/skills"))
    );
    let exists: Vec<bool> = targets.iter().map(|target| target.exists).collect();
    assert_eq!(exists, [true, true, false, false, true]);
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
        .install(request("skills/pdf", "claude"))
        .await
        .expect("install");
    let destination = home.join(".claude/skills/pdf");
    assert_eq!(result.skill, "pdf");
    assert_eq!(result.source, REPO);
    assert_eq!(result.target, "claude");
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
        .install(request("skills/pdf", "claude"))
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
    assert_eq!(records[0].target, "claude");
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
            .install(request(path, "claude"))
            .await
            .expect_err(&format!("{path} must be refused"));
        assert!(
            matches!(err, SkillError::Invalid(_)),
            "{path} should be invalid, got {err:?}"
        );
    }

    let err = service
        .install(request("skills/pdf", "not-a-harness"))
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
        .install(request("skills/bad", "claude"))
        .await
        .expect_err("symlink in a skill must be refused");
    assert!(
        format!("{err}").contains("symlink"),
        "error should name the symlink: {err}"
    );
    assert!(!dir.path().join("home/.claude/skills/bad").exists());
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
    let target_root = home.join(".claude/skills");
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
        .install(request("skills/pdf", "claude"))
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
        .install(request("skills/pdf", "claude"))
        .await
        .expect("online install");
    std::fs::remove_dir_all(home.join(".claude/skills/pdf")).unwrap();
    assert!(!Path::new(&home.join(".claude/skills/pdf")).exists());

    drop(server);

    let offline = service
        .install(request("skills/pdf", "claude"))
        .await
        .expect("offline install");
    assert!(!offline.overwritten);
    assert_eq!(
        std::fs::read_to_string(home.join(".claude/skills/pdf/scripts/render.py")).unwrap(),
        "print('pdf')\n"
    );
    // The blob cache is under the app-data dir; nothing is fetched from the internet in tests.
    assert!(paths.cache.join("skills").is_dir());
}
