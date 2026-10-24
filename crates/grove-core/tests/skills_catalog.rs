//! Skill catalog: GitHub tree discovery, manifest cache with TTL + ETag, offline fallback, and the
//! optional token. Everything goes through `wiremock`; no test talks to the network.

mod skills_support;

use grove_core::secrets::{MemorySecretStore, SecretStore, GITHUB_TOKEN};
use grove_core::skills::{SkillCatalogConfig, SkillSource};
use skills_support::{
    assert_tree_excludes, config, mount_raw, mount_repo, service, service_with_secrets, temp_paths,
    tree, REPO,
};
use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config_for_dead_port() -> SkillCatalogConfig {
    SkillCatalogConfig {
        sources: vec![SkillSource::new(REPO)],
        api_base: "http://127.0.0.1:1".to_string(),
        ttl: Duration::ZERO,
        home: None,
    }
}

fn catalog_tree() -> serde_json::Value {
    tree(&[
        ("README.md", "100644"),
        ("skills/pdf/SKILL.md", "100644"),
        ("skills/pdf/scripts/render.py", "100644"),
        // A directory without SKILL.md is not a catalog entry.
        ("skills/notes/README.md", "100644"),
        // Nested skills are entries too, named after their own directory.
        ("skills/nested/deep/SKILL.md", "100644"),
        // Trees are not files.
        ("skills/pdf", "040000"),
    ])
}

#[test]
fn defaults_point_at_the_documented_catalog_repositories() {
    let config = SkillCatalogConfig::default();
    let repos: Vec<&str> = config
        .sources
        .iter()
        .map(|source| source.repo.as_str())
        .collect();
    assert_eq!(repos, ["anthropics/skills", "mattpocock/skills"]);
    assert!(config.api_base.starts_with("https://"));
}

#[tokio::test]
async fn catalog_lists_only_directories_that_contain_a_skill_manifest() {
    let (dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(&server, REPO, "main", catalog_tree()).await;
    let mut config = config(&server);
    config.home = Some(dir.path().join("home"));
    let service = service(paths, config);

    let snapshot = service.catalog(false).await.expect("catalog");

    let paths: Vec<&str> = snapshot
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(paths, ["skills/nested/deep", "skills/pdf"]);
    assert_eq!(snapshot.entries[1].name, "pdf");
    assert_eq!(snapshot.entries[1].source, REPO);
    assert_eq!(snapshot.entries[1].id, "acme/skills::skills/pdf");
    assert!(!snapshot.from_cache);
    assert!(!snapshot.offline);
    assert!(snapshot.fetched_at.is_some());
}

#[tokio::test]
async fn preview_returns_frontmatter_and_body() {
    let (_dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(&server, REPO, "main", catalog_tree()).await;
    mount_raw(
        &server,
        REPO,
        "skills/pdf/SKILL.md",
        "---\nname: pdf\ndescription: Create and inspect PDF files\n---\n# PDF\n\nUse me.\n",
    )
    .await;
    let service = service(paths, config(&server));

    let preview = service.preview(REPO, "skills/pdf").await.expect("preview");

    assert_eq!(preview.entry.path, "skills/pdf");
    assert_eq!(
        preview.frontmatter.get("name").map(String::as_str),
        Some("pdf")
    );
    assert_eq!(
        preview.frontmatter.get("description").map(String::as_str),
        Some("Create and inspect PDF files")
    );
    assert_eq!(preview.body, "# PDF\n\nUse me.\n");
}

#[tokio::test]
async fn preview_refuses_traversal_and_absolute_paths_before_any_request() {
    let (_dir, paths) = temp_paths();
    let server = MockServer::start().await;
    // No mocks mounted: a request would fail, so success of the error path proves validation ran first.
    let service = service(paths, config(&server));

    let traversal = service.preview(REPO, "skills/../../secrets").await;
    assert!(traversal.is_err(), "traversal must be refused");
    assert!(matches!(
        traversal.unwrap_err(),
        grove_core::skills::SkillError::Invalid(_)
    ));

    let absolute = service.preview(REPO, "/etc/passwd").await;
    assert!(absolute.is_err(), "absolute path must be refused");
}

#[tokio::test]
async fn offline_catalog_serves_the_disk_cache() {
    let (_dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(&server, REPO, "main", catalog_tree()).await;
    let mut config = config(&server);
    // Zero TTL: the second call always tries the network.
    config.ttl = Duration::ZERO;
    let online_service = service(paths.clone(), config);

    let online = online_service.catalog(false).await.expect("online catalog");
    assert!(!online.from_cache);
    assert_eq!(online.entries.len(), 2);

    // A second service pointed at a dead port: the network is unavailable, the cache is not.
    drop(server);
    let mut unreachable = config_for_dead_port();
    unreachable.ttl = Duration::ZERO;
    let offline_service = service(paths.clone(), unreachable);

    let offline = offline_service
        .catalog(false)
        .await
        .expect("offline catalog");
    assert!(
        offline.offline,
        "served from cache after the network failed"
    );
    assert!(offline.from_cache);
    assert_eq!(offline.entries, online.entries);

    // The cache lives under the app-data dir, not somewhere ambient.
    assert!(
        paths.cache.join("skills").is_dir(),
        "manifest cache is written under AppPaths.cache"
    );
}

#[tokio::test]
async fn a_fresh_cache_is_served_without_a_request_and_refresh_bypasses_it_with_an_etag() {
    let (_dir, paths) = temp_paths();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "default_branch": "main"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO}/git/trees/main")))
        .and(header("if-none-match", "\"etag-1\""))
        .respond_with(ResponseTemplate::new(304))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO}/git/trees/main")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"etag-1\"")
                .set_body_json(catalog_tree()),
        )
        .with_priority(2)
        .expect(1)
        .mount(&server)
        .await;

    let service = service(paths, config(&server));

    let first = service.catalog(false).await.expect("first catalog");
    assert!(!first.from_cache);

    // Inside the TTL: no request at all.
    let cached = service.catalog(false).await.expect("cached catalog");
    assert!(cached.from_cache);
    assert!(!cached.offline);
    assert_eq!(cached.entries, first.entries);

    // Refresh bypasses the TTL and revalidates with the stored ETag (304 keeps the cache).
    let refreshed = service.catalog(true).await.expect("refreshed catalog");
    assert!(refreshed.from_cache);
    assert!(!refreshed.offline);
    assert_eq!(refreshed.entries, first.entries);

    server.verify().await;
}

#[tokio::test]
async fn github_token_is_read_from_secret_store_but_never_written_to_app_data() {
    const TOKEN: &str = "ghp_secret_token_value_123";
    let (_dir, paths) = temp_paths();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO}")))
        .and(header("authorization", format!("Bearer {TOKEN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "default_branch": "main"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO}/git/trees/main")))
        .and(header("authorization", format!("Bearer {TOKEN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(catalog_tree()))
        .mount(&server)
        .await;

    let store = Arc::new(MemorySecretStore::new());
    store.set(GITHUB_TOKEN, TOKEN).expect("store token");
    assert_eq!(
        store.status(GITHUB_TOKEN),
        grove_core::secrets::SecretStatus::Present
    );
    let service = service_with_secrets(paths.clone(), config(&server), store);

    let snapshot = service.catalog(false).await.expect("catalog with token");
    assert_eq!(snapshot.entries.len(), 2);

    // The token is used but never logged or persisted: the whole app-data tree stays secret-free.
    assert_tree_excludes(&paths.app_data, TOKEN);
    assert!(
        !format!("{service:?}").contains(TOKEN),
        "Debug output must not print the token"
    );
}

#[tokio::test]
async fn configurable_sources_are_read_in_order() {
    let (_dir, paths) = temp_paths();
    let server = MockServer::start().await;
    mount_repo(
        &server,
        "acme/one",
        "main",
        tree(&[("skills/one/SKILL.md", "100644")]),
    )
    .await;
    mount_repo(
        &server,
        "acme/two",
        "trunk",
        tree(&[("two/SKILL.md", "100644")]),
    )
    .await;
    let mut config = config(&server);
    config.sources = vec![
        SkillSource::new("acme/one"),
        SkillSource::new("acme/two").with_branch("trunk"),
    ];
    let service = service(paths, config);

    let snapshot = service.catalog(false).await.expect("catalog");

    let sources: Vec<&str> = snapshot
        .entries
        .iter()
        .map(|entry| entry.source.as_str())
        .collect();
    assert_eq!(sources, ["acme/one", "acme/two"]);
    let paths: Vec<&str> = snapshot
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(paths, ["skills/one", "two"]);
}

#[test]
fn memory_secret_store_redacts_its_debug_output() {
    let store = MemorySecretStore::new();
    store.set(GITHUB_TOKEN, "ghp_do_not_print_me").unwrap();
    assert_eq!(
        store.get(GITHUB_TOKEN).unwrap().as_deref(),
        Some("ghp_do_not_print_me")
    );
    assert!(!format!("{store:?}").contains("ghp_do_not_print_me"));
    assert_eq!(
        store.status(GITHUB_TOKEN),
        grove_core::secrets::SecretStatus::Present
    );
    store.delete(GITHUB_TOKEN).unwrap();
    assert_eq!(store.get(GITHUB_TOKEN).unwrap(), None);
    assert_eq!(
        store.status(GITHUB_TOKEN),
        grove_core::secrets::SecretStatus::Absent
    );
}
