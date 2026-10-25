//! The MCP subsystem: what servers each harness *would* use, and an explicit probe for one of them.
//!
//! **Discovery only reads files.** The configs belong to other tools (`.mcp.json`, `~/.claude.json`,
//! `~/.codex/config.toml`, `opencode.json`, an adapter-provided file); Grove parses them defensively
//! and never rewrites them. A file it cannot read or parse becomes a warning row, so one broken
//! harness config never hides the servers the others define.
//!
//! **Probing is explicit only.** [`McpService::probe`] spawns a third-party process, so nothing calls
//! it on discovery, on app start or on panel mount — the only caller is the `mcp_probe` command, i.e.
//! a click. A stdio probe speaks newline-delimited JSON-RPC (`initialize`, then `tools/list`); an HTTP
//! probe POSTs `tools/list`. Both have a timeout, and a stdio child is killed when the probe ends, so
//! a broken server can never leave an orphan behind.

pub mod discover;
pub mod probe;

use crate::ipc::{ErrorCode, IpcError};
use crate::paths::ProjectRoot;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a probe may take before the child is killed. Long enough for a cold `npx` download,
/// short enough that the Connect button never looks hung.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// How a server is reached. `sse` definitions from older Claude configs are normalised to [`Http`],
/// which is the transport they speak.
///
/// [`Http`]: McpTransport::Http
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    Http,
}

/// One server definition, normalised. Pinned by `contract/fixtures/mcp-servers.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerView {
    pub name: String,
    /// The config file this row is grouped under, e.g. `.mcp.json` or `~/.claude.json`.
    pub source: String,
    /// Every config file that defines this same server. `source` is always `sources[0]`.
    pub sources: Vec<String>,
    pub transport: McpTransport,
    /// The executable for a stdio server; `None` for http.
    pub command: Option<String>,
    /// Arguments for a stdio server; empty for http.
    pub args: Vec<String>,
    /// The endpoint for an http server; `None` for stdio.
    pub url: Option<String>,
}

impl McpServerView {
    /// True when both rows describe the same definition. The name is part of the identity: two
    /// managers pointing at the same binary under different names are two rows, not one.
    pub(crate) fn same_definition(&self, other: &Self) -> bool {
        self.name == other.name
            && self.transport == other.transport
            && self.command == other.command
            && self.args == other.args
            && self.url == other.url
    }
}

/// A config file the scanner could not read or parse, or a definition it could not normalise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigWarning {
    /// The config file the problem is in, matching [`McpServerView::source`].
    pub source: String,
    /// Human-readable, never a secret: a parse error or the server name plus what is missing.
    pub message: String,
}

/// What `mcp_servers` returns: every definition found, plus the files that could not be parsed.
/// Pinned by `contract/fixtures/mcp-servers.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDiscovery {
    pub servers: Vec<McpServerView>,
    pub warnings: Vec<McpConfigWarning>,
}

/// Whether the probe reached the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeState {
    Connected,
    Error,
}

/// One tool the server advertises.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
}

/// The result of one explicit probe. A failed connection is *this* result, not an `IpcError`: the
/// panel renders it as a state badge plus the detail. Pinned by `contract/fixtures/mcp-probe*.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProbe {
    pub name: String,
    pub source: String,
    pub transport: McpTransport,
    pub state: ProbeState,
    /// Empty unless the probe connected.
    pub tools: Vec<McpTool>,
    /// Why the probe failed: a timeout, a spawn error, an HTTP status or a JSON-RPC error.
    pub error: Option<String>,
    /// The tail of the server's stderr — the useful part of a crash report. HTTP has no stderr.
    pub stderr_tail: Option<String>,
}

/// Where to look and how patient to be. Tests always inject `home` and `project_root` so no test
/// ever reads the developer's real config.
#[derive(Debug, Clone)]
pub struct McpConfig {
    /// Overrides the OS home directory (`~/.claude.json`, `~/.codex/config.toml`).
    pub home: Option<PathBuf>,
    /// Overrides the project root (`.mcp.json`, `opencode.json`).
    pub project_root: Option<PathBuf>,
    /// An extra config file an adapter contributes. Read as JSON, or as TOML when it ends in `.toml`.
    pub adapter_config: Option<PathBuf>,
    /// How long one probe may take before the child is killed.
    pub timeout: Duration,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            home: None,
            project_root: None,
            adapter_config: None,
            timeout: DEFAULT_PROBE_TIMEOUT,
        }
    }
}

/// Every failure the MCP subsystem reports as an `IpcError`. A probe that fails to connect is a
/// [`ServerProbe`] with `state: error`, *not* an error here.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum McpError {
    #[error("no MCP server named `{0}` is configured in that file")]
    NotFound(String),
    #[error("invalid MCP request: {0}")]
    Invalid(String),
}

impl McpError {
    pub fn code(&self) -> ErrorCode {
        match self {
            McpError::NotFound(_) => ErrorCode::NotFound,
            McpError::Invalid(_) => ErrorCode::Invalid,
        }
    }
}

impl From<McpError> for IpcError {
    fn from(err: McpError) -> Self {
        IpcError::new(err.code(), err.to_string())
    }
}

/// The MCP subsystem as the rest of the app sees it: discover, then probe one server on request.
pub struct McpService {
    config: McpConfig,
    /// The project whose `.mcp.json` / `opencode.json` are read. Shared with the rest of the core, so
    /// switching sessions moves discovery too instead of leaving it on the app's own directory.
    project_root: ProjectRoot,
    http: reqwest::Client,
}

impl McpService {
    /// Cheap: no I/O and no process starts. Constructing the service must never spawn a server.
    pub fn new(config: McpConfig) -> Self {
        let project_root = ProjectRoot::new(
            config
                .project_root
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from(".")),
        );
        Self::with_project_root(config, project_root)
    }

    /// Share the core's project root, so discovery follows the active session instead of caching the
    /// directory the app happened to start in.
    pub fn with_project_root(config: McpConfig, project_root: ProjectRoot) -> Self {
        Self {
            config,
            project_root,
            http: reqwest::Client::builder()
                .user_agent(format!("Grove/{}", crate::VERSION))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn config(&self) -> &McpConfig {
        &self.config
    }

    /// The project the config files are read from: the shared one, unless a config names its own.
    pub fn project_root(&self) -> Option<PathBuf> {
        Some(self.project_root.path())
    }

    /// Every definition the configured files declare. Reads files; never spawns or fetches.
    pub fn discover(&self) -> McpDiscovery {
        discover::discover(self)
    }

    /// Probe one server, explicitly. The definition is re-read first, so a stale panel row can never
    /// spawn something the config no longer declares.
    pub async fn probe(&self, name: &str, source: &str) -> Result<ServerProbe, McpError> {
        if name.trim().is_empty() {
            return Err(McpError::Invalid("a server name is required".to_string()));
        }
        let server = self
            .discover()
            .servers
            .into_iter()
            .find(|server| server.name == name && server.sources.iter().any(|s| s == source))
            .ok_or_else(|| McpError::NotFound(format!("{name} ({source})")))?;
        Ok(probe::run(self, &server, self.config.timeout).await)
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }
}

impl std::fmt::Debug for McpService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpService")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Home directory from the injected config (tests) or the environment. Windows is the first target,
/// so `USERPROFILE` is checked before `HOME`. Same rule as `skills::targets`.
fn home_dir(configured: Option<&Path>) -> PathBuf {
    if let Some(home) = configured {
        return home.to_path_buf();
    }
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructing_the_service_does_no_io() {
        // A config that points at nothing at all: construction must not care.
        let service = McpService::new(McpConfig {
            home: Some(PathBuf::from("nowhere")),
            project_root: Some(PathBuf::from("nowhere")),
            adapter_config: None,
            timeout: DEFAULT_PROBE_TIMEOUT,
        });
        assert_eq!(service.config().timeout, DEFAULT_PROBE_TIMEOUT);
    }

    #[test]
    fn errors_map_to_the_documented_codes() {
        assert_eq!(McpError::NotFound("x".into()).code(), ErrorCode::NotFound);
        assert_eq!(McpError::Invalid("x".into()).code(), ErrorCode::Invalid);
        let ipc: IpcError = McpError::Invalid("no name".into()).into();
        assert_eq!(ipc.code, ErrorCode::Invalid);
    }
}
