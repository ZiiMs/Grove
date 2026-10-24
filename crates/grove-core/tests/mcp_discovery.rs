//! MCP discovery: read every harness config, normalise it, dedupe across files, and turn a broken
//! file into a warning row. All configs live in a tempdir; nothing here touches the developer's home.

mod mcp_support;

use grove_core::mcp::{McpConfigWarning, McpServerView, McpTransport};
use mcp_support::{fake_server, wait_for_file, write_fake_server, Sandbox};
use serde_json::json;

fn view(
    name: &str,
    source: &str,
    sources: &[&str],
    transport: McpTransport,
    command: Option<&str>,
    args: &[&str],
    url: Option<&str>,
) -> McpServerView {
    McpServerView {
        name: name.to_string(),
        source: source.to_string(),
        sources: sources.iter().map(|s| s.to_string()).collect(),
        transport,
        command: command.map(str::to_string),
        args: args.iter().map(|a| a.to_string()).collect(),
        url: url.map(str::to_string),
    }
}

fn warning(source: &str, needle: &str) -> impl Fn(&McpConfigWarning) -> bool {
    let source = source.to_string();
    let needle = needle.to_string();
    move |w| w.source == source && w.message.contains(&needle)
}

#[test]
fn reads_every_harness_config_and_normalises_the_servers() {
    let sandbox = Sandbox::new();
    sandbox.write_project(
        ".mcp.json",
        &json!({
            "mcpServers": {
                "filesystem": {
                    "type": "stdio",
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]
                }
            }
        })
        .to_string(),
    );
    sandbox.write_home(
        ".claude.json",
        &json!({
            "mcpServers": { "docs": { "type": "http", "url": "https://mcp.example.com/docs" } }
        })
        .to_string(),
    );
    sandbox.write_home(
        ".codex/config.toml",
        "[mcp_servers.linear]\ncommand = \"npx\"\nargs = [\"-y\", \"linear-mcp\"]\n",
    );
    sandbox.write_project(
        "opencode.json",
        &json!({
            "mcp": { "postgres": { "type": "local", "command": ["npx", "-y", "pg-mcp"] } }
        })
        .to_string(),
    );
    let adapter = sandbox.write_project(
        "adapter.json",
        &json!({ "mcpServers": { "custom": { "url": "http://127.0.0.1:9/mcp" } } }).to_string(),
    );

    let mut config = sandbox.config();
    config.adapter_config = Some(adapter);
    let discovery = grove_core::mcp::McpService::new(config).discover();

    assert_eq!(
        discovery.warnings,
        Vec::<McpConfigWarning>::new(),
        "every config is well formed"
    );
    assert_eq!(
        discovery.servers,
        // Source order, then name order within a file: `.mcp.json`, `~/.claude.json`,
        // `~/.codex/config.toml`, `opencode.json`, adapter config.
        vec![
            view(
                "filesystem",
                ".mcp.json",
                &[".mcp.json"],
                McpTransport::Stdio,
                Some("npx"),
                &["-y", "@modelcontextprotocol/server-filesystem", "."],
                None,
            ),
            view(
                "docs",
                "~/.claude.json",
                &["~/.claude.json"],
                McpTransport::Http,
                None,
                &[],
                Some("https://mcp.example.com/docs"),
            ),
            view(
                "linear",
                "~/.codex/config.toml",
                &["~/.codex/config.toml"],
                McpTransport::Stdio,
                Some("npx"),
                &["-y", "linear-mcp"],
                None,
            ),
            view(
                "postgres",
                "opencode.json",
                &["opencode.json"],
                McpTransport::Stdio,
                Some("npx"),
                &["-y", "pg-mcp"],
                None,
            ),
            view(
                "custom",
                "adapter:adapter.json",
                &["adapter:adapter.json"],
                McpTransport::Http,
                None,
                &[],
                Some("http://127.0.0.1:9/mcp"),
            ),
        ]
    );
}

#[test]
fn duplicate_definitions_are_deduped_with_all_sources_listed() {
    let sandbox = Sandbox::new();
    let definition = json!({ "type": "stdio", "command": "npx", "args": ["-y", "shared-mcp"] });
    sandbox.write_project(
        ".mcp.json",
        &json!({ "mcpServers": {
            "shared": definition.clone(),
            "project-only": { "command": "npx", "args": ["project-mcp"] }
        } })
        .to_string(),
    );
    sandbox.write_home(
        ".claude.json",
        &json!({ "mcpServers": {
            "shared": definition,
            // Same name, different definition: a different server, kept separately.
            "other": { "command": "npx", "args": ["other-mcp"] }
        } })
        .to_string(),
    );
    sandbox.write_home(
        ".codex/config.toml",
        "[mcp_servers.shared]\ncommand = \"npx\"\nargs = [\"-y\", \"shared-mcp\"]\n",
    );

    let discovery = sandbox.service().discover();

    let shared = discovery
        .servers
        .iter()
        .find(|server| server.name == "shared")
        .expect("shared server is listed once");
    assert_eq!(shared.source, ".mcp.json", "grouped under the first source");
    assert_eq!(
        shared.sources,
        vec![
            ".mcp.json".to_string(),
            "~/.claude.json".to_string(),
            "~/.codex/config.toml".to_string()
        ],
        "every source that defines it is listed"
    );
    assert_eq!(
        discovery
            .servers
            .iter()
            .filter(|server| server.name == "shared")
            .count(),
        1,
        "identical definitions merge into one row"
    );
    let mut names: Vec<&str> = discovery
        .servers
        .iter()
        .map(|server| server.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["other", "project-only", "shared"],
        "different definitions of the same name stay apart"
    );
}

#[test]
fn a_malformed_file_becomes_a_warning_row_not_a_failure() {
    let sandbox = Sandbox::new();
    sandbox.write_project(".mcp.json", "{ \"mcpServers\": { this is not json");
    sandbox.write_home(".codex/config.toml", "[mcp_servers.boom]\ncommand = npx\n");
    sandbox.write_home(
        ".claude.json",
        &json!({ "mcpServers": { "docs": { "url": "https://mcp.example.com/docs" } } }).to_string(),
    );

    let discovery = sandbox.service().discover();

    assert!(
        discovery
            .warnings
            .iter()
            .any(warning(".mcp.json", "invalid JSON")),
        "the broken project file is reported: {:?}",
        discovery.warnings
    );
    assert!(
        discovery
            .warnings
            .iter()
            .any(warning("~/.codex/config.toml", "invalid TOML")),
        "the broken TOML file is reported: {:?}",
        discovery.warnings
    );
    assert_eq!(
        discovery.servers.len(),
        1,
        "one broken file never hides another harness's servers"
    );
    assert_eq!(discovery.servers[0].name, "docs");
}

#[test]
fn a_definition_without_a_command_or_url_is_a_warning() {
    let sandbox = Sandbox::new();
    sandbox.write_project(
        ".mcp.json",
        &json!({ "mcpServers": {
            "broken": { "description": "no command, no url" },
            "no-command": { "type": "stdio", "url": "https://mcp.example.com/docs" }
        } })
        .to_string(),
    );

    let discovery = sandbox.service().discover();

    assert!(discovery.servers.is_empty());
    assert!(
        discovery
            .warnings
            .iter()
            .any(|w| w.source == ".mcp.json" && w.message.contains("broken")),
        "the warning names the server: {:?}",
        discovery.warnings
    );
    assert!(
        discovery
            .warnings
            .iter()
            .any(|w| w.source == ".mcp.json" && w.message.contains("no-command")),
        "a stdio declaration without a command is refused: {:?}",
        discovery.warnings
    );
}

#[test]
fn missing_config_files_are_silent() {
    let sandbox = Sandbox::new();

    let discovery = sandbox.service().discover();

    assert!(discovery.servers.is_empty());
    assert!(
        discovery.warnings.is_empty(),
        "a harness the user does not use is not a warning: {:?}",
        discovery.warnings
    );
}

#[test]
fn a_config_without_an_mcp_section_is_not_a_warning() {
    let sandbox = Sandbox::new();
    sandbox.write_project("opencode.json", &json!({ "theme": "dark" }).to_string());

    let discovery = sandbox.service().discover();

    assert!(discovery.servers.is_empty());
    assert!(discovery.warnings.is_empty());
}

#[test]
fn discovery_never_spawns_a_server() {
    // The fake server touches a marker file the moment it starts, so an accidental spawn is visible.
    let sandbox = Sandbox::new();
    let marker = sandbox.path("spawned.marker");
    let extra = [format!("--touch={}", marker.display())];
    let extra: Vec<&str> = extra.iter().map(String::as_str).collect();
    write_fake_server(&sandbox, "filesystem", "ok", &extra);

    let service = sandbox.service();
    // App start constructs the service; give a spawn the same window the probe tests use, so this
    // cannot pass by being too fast.
    assert!(
        !wait_for_file(&marker, std::time::Duration::from_millis(300)),
        "constructing the service must not spawn an MCP server"
    );

    let discovery = service.discover();

    assert_eq!(discovery.servers.len(), 1);
    assert_eq!(
        discovery.servers[0].command.as_deref(),
        Some("node"),
        "the definition is reported, not executed"
    );
    assert_eq!(
        discovery.servers[0].args[0],
        fake_server().to_string_lossy(),
        "args are reported verbatim"
    );
    // A discovery must never spawn either: this false is the whole point of the ticket.
    assert!(
        !wait_for_file(&marker, std::time::Duration::from_millis(400)),
        "discovery must never spawn an MCP server"
    );

    // Two discoveries in a row are still silent: no probe on discovery or on panel refresh.
    let _ = service.discover();
    assert!(!marker.exists(), "discovery stays side-effect free");
}

#[test]
fn an_unreadable_adapter_config_is_a_warning_row() {
    let sandbox = Sandbox::new();
    let adapter = sandbox.path("does-not-exist/adapter.json");
    let mut config = sandbox.config();
    config.adapter_config = Some(adapter);

    let discovery = grove_core::mcp::McpService::new(config).discover();

    assert!(discovery.servers.is_empty());
    assert!(
        discovery.warnings.is_empty(),
        "a missing adapter config is simply absent: {:?}",
        discovery.warnings
    );

    let broken = mcp_support::write(&sandbox.path("adapter.json"), "[1, 2, 3]");
    let mut config = sandbox.config();
    config.adapter_config = Some(broken);
    let discovery = grove_core::mcp::McpService::new(config).discover();
    assert!(
        discovery
            .warnings
            .iter()
            .any(warning("adapter:adapter.json", "JSON object")),
        "{:?}",
        discovery.warnings
    );
}

#[test]
fn a_toml_adapter_config_is_read_as_codex_style() {
    let sandbox = Sandbox::new();
    let adapter = sandbox.write_project(
        "adapter.toml",
        "[mcp_servers.injected]\ncommand = \"node\"\nargs = [\"injected.mjs\"]\n",
    );
    let mut config = sandbox.config();
    config.adapter_config = Some(adapter);

    let discovery = grove_core::mcp::McpService::new(config).discover();

    assert_eq!(discovery.servers.len(), 1);
    assert_eq!(discovery.servers[0].name, "injected");
    assert_eq!(discovery.servers[0].source, "adapter:adapter.toml");
    assert_eq!(
        discovery.servers[0].command.as_deref(),
        Some("node"),
        "a .toml adapter config is read like Codex's"
    );
    assert_eq!(discovery.servers[0].args, ["injected.mjs"]);
}
