//! MCP commands. Discovery only reads config files; `mcp_probe` spawns a server, so it is reachable
//! only from an explicit Connect in the panel — never on app start or panel mount.

use crate::state::AppState;
use grove_core::ipc::IpcResult;
use grove_core::mcp::{McpDiscovery, ServerProbe};

/// Every server definition the configured files declare, plus the files that could not be parsed.
#[tauri::command]
pub async fn mcp_servers(state: tauri::State<'_, AppState>) -> IpcResult<McpDiscovery> {
    Ok(state.core.mcp.discover())
}

/// Probe one server: a stdio JSON-RPC handshake, or an HTTP `tools/list` POST. The result carries the
/// state, the tools and a stderr tail; a refused connection is a state, not a command error.
#[tauri::command]
pub async fn mcp_probe(
    state: tauri::State<'_, AppState>,
    name: String,
    source: String,
) -> IpcResult<ServerProbe> {
    Ok(state.core.mcp.probe(&name, &source).await?)
}
