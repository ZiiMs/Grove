//! Explicit MCP probes: the stdio JSON-RPC handshake, the HTTP `tools/list` POST, timeouts, and the
//! guarantee that a probe never leaves a child process behind.
//!
//! The stdio server is `fixtures/mcp/fake_stdio_server.mjs` (Node is already a build dependency); HTTP
//! goes through `wiremock`. No test talks to the network.

mod mcp_support;

use grove_core::mcp::{McpError, ProbeState};
use mcp_support::{wait_for_file, write_fake_server, Sandbox};
use std::time::{Duration, Instant};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn stdio_probe_handshakes_and_lists_the_tools() {
    let sandbox = Sandbox::new();
    let marker = sandbox.path("started.marker");
    let touch = format!("--touch={}", marker.display());
    write_fake_server(&sandbox, "filesystem", "ok", &[&touch]);

    let probe = sandbox
        .service()
        .probe("filesystem", ".mcp.json")
        .await
        .expect("probe runs");

    assert_eq!(probe.state, ProbeState::Connected, "{probe:?}");
    assert_eq!(probe.name, "filesystem");
    assert_eq!(probe.source, ".mcp.json");
    assert_eq!(
        probe
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        ["read_file", "list_directory"]
    );
    assert_eq!(
        probe.tools[0].description.as_deref(),
        Some("Read a file from the project")
    );
    assert_eq!(probe.tools[1].description, None);
    assert_eq!(probe.error, None);
    assert_eq!(probe.stderr_tail, None);
    assert!(marker.exists(), "the server really ran");
}

#[tokio::test]
async fn a_stdio_server_that_never_answers_times_out_and_is_killed() {
    let sandbox = Sandbox::new();
    let heartbeat = sandbox.path("heartbeat");
    let beat = format!("--heartbeat={}", heartbeat.display());
    write_fake_server(&sandbox, "sleeper", "sleep", &[&beat]);

    let mut config = sandbox.config();
    config.timeout = Duration::from_millis(800);
    let service = grove_core::mcp::McpService::new(config);

    let started = Instant::now();
    let probe = service
        .probe("sleeper", ".mcp.json")
        .await
        .expect("probe runs");
    let elapsed = started.elapsed();

    assert_eq!(probe.state, ProbeState::Error, "{probe:?}");
    assert!(
        probe
            .error
            .as_deref()
            .is_some_and(|error| error.contains("timed out")),
        "the error explains the timeout: {probe:?}"
    );
    assert!(
        probe
            .stderr_tail
            .as_deref()
            .is_some_and(|tail| tail.contains("sleeping")),
        "the stderr tail explains what the server was doing: {probe:?}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "the probe honours its timeout instead of hanging: {elapsed:?}"
    );
    assert!(
        heartbeat.exists(),
        "the sleeping server was really running, and wrote its heartbeat"
    );

    // No orphans: the fake server rewrites its heartbeat every 100 ms, so a process that survived the
    // probe would keep touching the file. Dropping the probe future (the timeout path) kills it.
    tokio::time::sleep(Duration::from_millis(250)).await;
    let before = std::fs::metadata(&heartbeat)
        .expect("heartbeat file")
        .modified()
        .expect("mtime");
    tokio::time::sleep(Duration::from_millis(600)).await;
    let after = std::fs::metadata(&heartbeat)
        .expect("heartbeat file")
        .modified()
        .expect("mtime");
    assert_eq!(
        before, after,
        "the probe must kill its child on timeout, leaving no orphan behind"
    );
}

#[tokio::test]
async fn a_crashing_stdio_server_reports_its_stderr_tail() {
    let sandbox = Sandbox::new();
    write_fake_server(&sandbox, "broken", "crash", &[]);

    let probe = sandbox
        .service()
        .probe("broken", ".mcp.json")
        .await
        .expect("probe reports a state instead of failing");

    assert_eq!(probe.state, ProbeState::Error, "{probe:?}");
    assert!(probe.error.is_some(), "{probe:?}");
    assert!(
        probe
            .stderr_tail
            .as_deref()
            .is_some_and(|tail| tail.contains("refusing to start")),
        "a short stderr tail is the useful part of a crash: {probe:?}"
    );
    assert!(probe.tools.is_empty());
}

#[tokio::test]
async fn a_jsonrpc_error_from_the_server_becomes_the_probe_error() {
    let sandbox = Sandbox::new();
    write_fake_server(&sandbox, "grumpy", "error", &[]);

    let probe = sandbox
        .service()
        .probe("grumpy", ".mcp.json")
        .await
        .expect("probe reports a state");

    assert_eq!(probe.state, ProbeState::Error, "{probe:?}");
    assert!(
        probe
            .error
            .as_deref()
            .is_some_and(|error| error.contains("tools are unavailable")),
        "{probe:?}"
    );
}

#[tokio::test]
async fn http_probe_posts_tools_list_and_lists_the_tools() {
    let sandbox = Sandbox::new();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(body_partial_json(
            serde_json::json!({ "method": "tools/list" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": { "tools": [
                { "name": "search", "description": "Search the docs" },
                { "name": "fetch" }
            ] }
        })))
        .mount(&server)
        .await;
    sandbox.write_home(
        ".claude.json",
        &serde_json::json!({ "mcpServers": { "docs": { "type": "http", "url": format!("{}/mcp", server.uri()) } } })
            .to_string(),
    );

    let probe = sandbox
        .service()
        .probe("docs", "~/.claude.json")
        .await
        .expect("probe runs");

    assert_eq!(probe.state, ProbeState::Connected, "{probe:?}");
    assert_eq!(
        probe
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        ["search", "fetch"]
    );
    assert_eq!(
        probe.tools[0].description.as_deref(),
        Some("Search the docs")
    );
    assert_eq!(probe.stderr_tail, None, "HTTP servers have no stderr");
}

#[tokio::test]
async fn http_probe_reads_a_server_sent_event_answer() {
    let sandbox = Sandbox::new();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[{\"name\":\"sse-tool\"}]}}\n\n",
        ))
        .mount(&server)
        .await;
    sandbox.write_home(
        ".claude.json",
        &serde_json::json!({ "mcpServers": { "streamed": { "url": format!("{}/mcp", server.uri()) } } })
            .to_string(),
    );

    let probe = sandbox
        .service()
        .probe("streamed", "~/.claude.json")
        .await
        .expect("probe runs");

    assert_eq!(probe.state, ProbeState::Connected, "{probe:?}");
    assert_eq!(probe.tools[0].name, "sse-tool");
}

#[tokio::test]
async fn an_http_error_becomes_a_probe_state_instead_of_a_command_failure() {
    let sandbox = Sandbox::new();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).set_body_string("upstream is asleep"))
        .mount(&server)
        .await;
    sandbox.write_home(
        ".claude.json",
        &serde_json::json!({ "mcpServers": { "docs": { "url": format!("{}/mcp", server.uri()) } } })
            .to_string(),
    );

    let probe = sandbox
        .service()
        .probe("docs", "~/.claude.json")
        .await
        .expect("probe reports a state");

    assert_eq!(probe.state, ProbeState::Error, "{probe:?}");
    assert!(
        probe
            .error
            .as_deref()
            .is_some_and(|error| error.contains("503")),
        "{probe:?}"
    );
}

#[tokio::test]
async fn probing_an_unknown_server_is_not_found() {
    let sandbox = Sandbox::new();
    write_fake_server(&sandbox, "filesystem", "ok", &[]);
    let service = sandbox.service();

    let missing = service.probe("nope", ".mcp.json").await;
    assert!(matches!(missing, Err(McpError::NotFound(_))), "{missing:?}");

    // The source has to match too: the panel cannot ask for a definition from a file that never
    // declared it, so a stale row cannot spawn anything.
    let wrong_source = service.probe("filesystem", "~/.claude.json").await;
    assert!(
        matches!(wrong_source, Err(McpError::NotFound(_))),
        "{wrong_source:?}"
    );
}

#[tokio::test]
async fn a_probe_re_reads_the_config_so_a_removed_server_cannot_be_spawned() {
    let sandbox = Sandbox::new();
    let marker = sandbox.path("started.marker");
    let touch = format!("--touch={}", marker.display());
    let config_file = write_fake_server(&sandbox, "filesystem", "ok", &[&touch]);
    let service = sandbox.service();
    assert_eq!(
        service
            .probe("filesystem", ".mcp.json")
            .await
            .expect("probe runs")
            .state,
        ProbeState::Connected
    );
    assert!(wait_for_file(&marker, Duration::from_secs(1)));
    std::fs::remove_file(&marker).expect("clear the marker");
    std::fs::remove_file(&config_file).expect("remove the config");

    let probe = service.probe("filesystem", ".mcp.json").await;

    assert!(matches!(probe, Err(McpError::NotFound(_))), "{probe:?}");
    assert!(
        !wait_for_file(&marker, Duration::from_millis(400)),
        "a definition that no longer exists is never spawned"
    );
}
