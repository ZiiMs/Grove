//! Shared fixtures for the skill-catalog tests. Not a test target: it lives in a subdirectory so
//! Cargo only compiles it into the tests that declare `mod skills_support;`.

use grove_core::paths::AppPaths;
use grove_core::secrets::MemorySecretStore;
use grove_core::skills::{SkillCatalogConfig, SkillService, SkillSource};
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const REPO: &str = "acme/skills";

pub fn temp_paths() -> (TempDir, AppPaths) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path().join("app-data"));
    paths.ensure().expect("create app data");
    (dir, paths)
}

/// A config pointed at `server` with one repository and a long TTL.
pub fn config(server: &MockServer) -> SkillCatalogConfig {
    SkillCatalogConfig {
        sources: vec![SkillSource::new(REPO)],
        api_base: server.uri(),
        ttl: std::time::Duration::from_secs(900),
        home: None,
    }
}

pub fn service(paths: AppPaths, config: SkillCatalogConfig) -> SkillService {
    service_with_secrets(paths, config, Arc::new(MemorySecretStore::new()))
}

pub fn service_with_secrets(
    paths: AppPaths,
    config: SkillCatalogConfig,
    secrets: Arc<dyn grove_core::secrets::SecretStore>,
) -> SkillService {
    SkillService::with_config(paths, secrets, config)
}

/// Mount the repository metadata and recursive tree endpoints the catalog reads.
pub async fn mount_repo(server: &MockServer, repo: &str, branch: &str, tree: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "default_branch": branch })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}/git/trees/{branch}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree))
        .mount(server)
        .await;
}

/// Mount one file on the contents API, served raw (the caller sets `Accept: application/vnd.github.raw`).
pub async fn mount_raw(server: &MockServer, repo: &str, rel_path: &str, body: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/repos/{repo}/contents/{rel_path}")))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(server)
        .await;
}

/// Mount every file of a skill directory. Keys are paths relative to `dir`.
pub async fn mount_skill(server: &MockServer, repo: &str, dir: &str, files: &[(&str, &str)]) {
    for (name, body) in files {
        mount_raw(server, repo, &format!("{dir}/{name}"), body).await;
    }
}

/// A GitHub tree response. `files` are `(repo-relative path, git mode)`; blob SHAs are derived from
/// the path so a test can recognise them in the blob cache.
pub fn tree(files: &[(&str, &str)]) -> Value {
    let entries: Vec<Value> = files
        .iter()
        .map(|(entry_path, mode)| {
            json!({
                "path": entry_path,
                "mode": mode,
                "type": if *mode == "040000" { "tree" } else { "blob" },
                "sha": blob_sha(entry_path),
            })
        })
        .collect();
    json!({ "sha": "root", "truncated": false, "tree": entries })
}

pub fn blob_sha(path: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(path.as_bytes());
    hex::encode(digest)[..40].to_string()
}

/// Assert no file under `root` contains `needle`.
pub fn assert_tree_excludes(root: &std::path::Path, needle: &str) {
    fn walk(dir: &std::path::Path, needle: &str) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, needle);
                continue;
            }
            let bytes = std::fs::read(&path).expect("read cached file");
            assert!(
                !bytes
                    .windows(needle.len())
                    .any(|window| window == needle.as_bytes()),
                "secret leaked into {}",
                path.display()
            );
        }
    }
    walk(root, needle);
}
