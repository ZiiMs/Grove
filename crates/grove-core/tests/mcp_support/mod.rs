//! Shared fixtures for the MCP tests. Lives in a subdirectory so Cargo only compiles it into the
//! tests that declare `mod mcp_support;`.
//!
//! Every test points `McpConfig` at a tempdir: no test reads the developer's real home directory.

use grove_core::mcp::{McpConfig, McpService};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A tempdir that stands in for the user's home and project directory.
pub struct Sandbox {
    dir: TempDir,
    /// The project root: `.mcp.json` and `opencode.json` live here.
    pub project: PathBuf,
    /// The fake home: `.claude.json` and `.codex/config.toml` live here.
    pub home: PathBuf,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Sandbox {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let project = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&project).expect("create project dir");
        std::fs::create_dir_all(&home).expect("create home dir");
        Self { dir, project, home }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn write_project(&self, name: &str, contents: &str) -> PathBuf {
        write(&self.project.join(name), contents)
    }

    pub fn write_home(&self, name: &str, contents: &str) -> PathBuf {
        write(&self.home.join(name), contents)
    }

    /// A config that reads only this sandbox, with a short probe timeout.
    pub fn config(&self) -> McpConfig {
        McpConfig {
            home: Some(self.home.clone()),
            project_root: Some(self.project.clone()),
            adapter_config: None,
            timeout: std::time::Duration::from_secs(5),
        }
    }

    pub fn service(&self) -> McpService {
        McpService::new(self.config())
    }
}

pub fn write(path: &Path, contents: &str) -> PathBuf {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
    }
    std::fs::write(path, contents).expect("write fixture");
    path.to_path_buf()
}

/// Absolute path of the fake stdio server shipped under `fixtures/mcp/`.
///
/// `std::path::absolute` rather than `canonicalize`: the latter returns a `\\?\` verbatim path on
/// Windows, which Node's module resolver mis-parses.
pub fn fake_server() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("mcp")
        .join("fake_stdio_server.mjs");
    let path = std::path::absolute(&path).expect("absolute path to the fake server");
    assert!(
        path.exists(),
        "fixtures/mcp/fake_stdio_server.mjs is missing at {}",
        path.display()
    );
    path
}

/// `{"command":"node","args":[...]}` pointing at the fake server in `mode`.
pub fn node_server_json(mode: &str, extra: &[&str]) -> Value {
    let mut args = vec![
        fake_server().to_string_lossy().to_string(),
        format!("--mode={mode}"),
    ];
    args.extend(extra.iter().map(|arg| arg.to_string()));
    json!({ "type": "stdio", "command": "node", "args": args })
}

/// Write `.mcp.json` in the project defining one stdio server named `name` that runs the fake
/// server in `mode`. Returns the config file path.
pub fn write_fake_server(sandbox: &Sandbox, name: &str, mode: &str, extra: &[&str]) -> PathBuf {
    let config = json!({ "mcpServers": { name: node_server_json(mode, extra) } });
    sandbox.write_project(".mcp.json", &config.to_string())
}

/// `.mcp.json` with one server definition map, so a test can spell out exact args.
pub fn write_mcp_json(sandbox: &Sandbox, servers: Value) -> PathBuf {
    sandbox.write_project(".mcp.json", &json!({ "mcpServers": servers }).to_string())
}

/// Poll until `path` exists, up to `timeout`. Used for files the spawned server writes.
pub fn wait_for_file(path: &Path, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    path.exists()
}
