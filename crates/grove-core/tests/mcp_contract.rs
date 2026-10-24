//! Golden-fixture half of the MCP contract. The TS side parses the same files with zod
//! (`packages/contract/test/mcp.test.ts`), so either side drifting breaks a test.

use grove_core::mcp::{
    McpConfigWarning, McpDiscovery, McpServerView, McpTool, McpTransport, ProbeState, ServerProbe,
};

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract/fixtures")
        .join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read fixture {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("fixture is valid JSON")
}

#[test]
fn discovery_matches_fixture() {
    let sample = McpDiscovery {
        servers: vec![
            McpServerView {
                name: "filesystem".into(),
                source: ".mcp.json".into(),
                sources: vec![".mcp.json".into(), "~/.claude.json".into()],
                transport: McpTransport::Stdio,
                command: Some("npx".into()),
                args: vec![
                    "-y".into(),
                    "@modelcontextprotocol/server-filesystem".into(),
                    ".".into(),
                ],
                url: None,
            },
            McpServerView {
                name: "linear".into(),
                source: "~/.codex/config.toml".into(),
                sources: vec!["~/.codex/config.toml".into()],
                transport: McpTransport::Http,
                command: None,
                args: Vec::new(),
                url: Some("https://mcp.example.com/linear".into()),
            },
        ],
        warnings: vec![McpConfigWarning {
            source: "opencode.json".into(),
            message: "invalid JSON: expected value at line 2 column 1".into(),
        }],
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("mcp-servers.json")
    );
}

#[test]
fn connected_probe_matches_fixture() {
    let sample = ServerProbe {
        name: "filesystem".into(),
        source: ".mcp.json".into(),
        transport: McpTransport::Stdio,
        state: ProbeState::Connected,
        tools: vec![
            McpTool {
                name: "read_file".into(),
                description: Some("Read a file from the project".into()),
            },
            McpTool {
                name: "list_directory".into(),
                description: None,
            },
        ],
        error: None,
        stderr_tail: None,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("mcp-probe.json")
    );
}

#[test]
fn error_probe_matches_fixture() {
    let sample = ServerProbe {
        name: "sleeper".into(),
        source: "~/.codex/config.toml".into(),
        transport: McpTransport::Http,
        state: ProbeState::Error,
        tools: Vec::new(),
        error: Some("timed out after 10s".into()),
        stderr_tail: Some("fake mcp server: sleeping".into()),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("mcp-probe-error.json")
    );
}
