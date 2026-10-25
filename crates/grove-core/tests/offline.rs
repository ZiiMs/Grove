//! The offline promise, with the network made hostile rather than merely absent.
//!
//! `skills_catalog::offline_catalog_serves_the_disk_cache` proves the cache fallback against a dead
//! port. This file adds the environment a real air-gapped (or badly proxied) machine has: `HTTP_PROXY`
//! and friends point at a black hole and `NO_PROXY` is empty, so a client that honours proxy settings
//! cannot reach even loopback. A session never needs the network, and the OpenCode adapter's loopback
//! client sets `no_proxy()`; the catalog still answers from the cache it wrote earlier.
//!
//! This lives in its own test binary because environment variables are process-global: no other test
//! target should inherit a black-hole proxy.

pub mod adapter_support;
pub mod skills_support;

use adapter_support::{acp_fixture, claude_fixture, codex_fixture, opencode_fixture, pi_fixture};
use grove_core::ipc::CreateSessionRequest;
use grove_core::paths::AppPaths;
use grove_core::sessions::event::{EventKind, HarnessEvent, Role, SessionState};
use grove_core::sessions::manager::SessionManager;
use grove_core::skills::{SkillCatalogConfig, SkillSource};
use serde_json::Value;
use skills_support::{mount_repo, tree, REPO};
use std::time::Duration;
use tempfile::TempDir;
use wiremock::MockServer;

const BLACK_HOLE: &str = "http://127.0.0.1:9";
const WAIT: Duration = Duration::from_secs(30);

/// Point every proxy variable a Rust/Node client might read at a port nothing listens on.
fn use_a_black_hole_proxy() {
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        std::env::set_var(key, BLACK_HOLE);
    }
    // Explicitly empty: no host is exempt, not even loopback.
    std::env::set_var("NO_PROXY", "");
    std::env::set_var("no_proxy", "");
}

fn idle_count(events: &[HarnessEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                &event.kind,
                EventKind::Status {
                    state: SessionState::Idle
                }
            )
        })
        .count()
}

fn assistant_messages(events: &[HarnessEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                &event.kind,
                EventKind::Message {
                    role: Role::Assistant,
                    ..
                }
            )
        })
        .count()
}

async fn wait_for_idle(manager: &SessionManager, harness: &str, id: &str) -> Vec<HarnessEvent> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let events = manager
            .get(id)
            .unwrap_or_else(|error| panic!("[{harness}] session {id} is readable: {error}"))
            .events;
        if idle_count(&events) >= 1 && assistant_messages(&events) >= 1 {
            return events;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "[{harness}] the offline turn never finished; last events: {:?}",
            events
                .iter()
                .map(|event| event.kind.tag())
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// One manager, one turn per registered harness, all with the proxy environment above.
async fn sessions_run_offline(app_paths: &AppPaths) {
    let state = tempfile::tempdir().expect("the OpenCode fixture's session store");
    let cases: Vec<(&str, Value, Option<TempDir>)> = vec![
        ("mock", serde_json::json!({ "chunkDelayMs": 1 }), None),
        ("claude", claude_fixture::config("tools", &[]), None),
        ("acp", acp_fixture::config("happy", &[]), None),
        ("pi", pi_fixture::config("tools", &[]), None),
        (
            "opencode",
            opencode_fixture::config("happy", state.path(), &["--delay-ms", "2"]),
            Some(state),
        ),
        ("codex", codex_fixture::config("happy", &[]), None),
    ];

    for (harness, config, _state) in cases {
        let manager = SessionManager::in_memory(app_paths).expect("the runtime opens");
        let detail = manager
            .create(CreateSessionRequest {
                harness: harness.to_string(),
                config: Some(config),
                ..Default::default()
            })
            .await
            .unwrap_or_else(|error| panic!("[{harness}] create offline: {error}"));
        let id = detail.session.id;

        manager
            .send(&id, "turn with no network")
            .await
            .unwrap_or_else(|error| panic!("[{harness}] send offline: {error}"));
        let events = wait_for_idle(&manager, harness, &id).await;
        assert!(
            assistant_messages(&events) >= 1,
            "[{harness}] the fake harness answered without the network: {:?}",
            events
                .iter()
                .map(|event| event.kind.tag())
                .collect::<Vec<_>>()
        );
        assert!(
            !events.iter().any(|event| matches!(
                &event.kind,
                EventKind::Error { message, .. } if message.to_lowercase().contains("proxy")
            )),
            "[{harness}] a proxy must not enter the session path"
        );

        manager
            .close(&id)
            .await
            .unwrap_or_else(|error| panic!("[{harness}] close offline: {error}"));
    }
}

/// The catalog has to answer from disk after the network became unreachable.
async fn catalog_serves_its_cache_offline() {
    // Warm the cache while the GitHub stand-in is still reachable.
    let (_dir, paths) = skills_support::temp_paths();
    let server = MockServer::start().await;
    mount_repo(
        &server,
        REPO,
        "main",
        tree(&[
            ("skills/pdf/SKILL.md", "100644"),
            ("skills/pdf/scripts/render.py", "100644"),
        ]),
    )
    .await;
    let online_service = skills_support::service(paths.clone(), skills_support::config(&server));
    let online = online_service
        .catalog(false)
        .await
        .expect("the catalog warms its cache online");
    assert!(!online.from_cache);
    drop(server);

    use_a_black_hole_proxy();

    // A fresh service (so its HTTP client was built under the hostile environment) with a zero TTL
    // has to try the network and then fall back to the cache.
    let offline_service = skills_support::service(
        paths,
        SkillCatalogConfig {
            sources: vec![SkillSource::new(REPO)],
            api_base: BLACK_HOLE.to_string(),
            ttl: Duration::ZERO,
            home: None,
        },
    );
    let offline = offline_service
        .catalog(false)
        .await
        .expect("the cached catalog answers with no reachable network");
    assert!(offline.offline, "the result says it came from the cache");
    assert!(offline.from_cache);
    assert_eq!(
        offline.entries, online.entries,
        "the offline catalog is exactly the cached one"
    );
}

#[tokio::test]
async fn sessions_and_the_cached_catalog_work_with_no_reachable_network() {
    catalog_serves_its_cache_offline().await;

    // The catalog phase already set the proxy environment; this asserts it is the hostile one.
    assert_eq!(std::env::var("HTTP_PROXY").as_deref(), Ok(BLACK_HOLE));
    assert_eq!(std::env::var("NO_PROXY").as_deref(), Ok(""));

    let dir = tempfile::tempdir().expect("app-data tempdir");
    let app_paths = AppPaths::new(dir.path());
    sessions_run_offline(&app_paths).await;
}
