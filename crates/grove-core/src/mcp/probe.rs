//! Explicit MCP probes.
//!
//! A stdio probe spawns the server and speaks newline-delimited JSON-RPC per the MCP stdio transport:
//! `initialize`, then `notifications/initialized`, then `tools/list`. An HTTP probe POSTs `tools/list`
//! and accepts either a JSON body or a server-sent-event frame. Both are bounded by a timeout, and the
//! stdio child is killed before the probe returns — `kill_on_drop` covers every early return, and the
//! explicit kill covers the timeout, where the handshake future is dropped mid-flight.

use super::{McpServerView, McpService, McpTool, McpTransport, ProbeState, ServerProbe};
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};
use tokio::task::JoinHandle;

/// MCP protocol revision Grove speaks. Servers negotiate down; an unknown revision is a log line.
const PROTOCOL_VERSION: &str = "2024-11-05";
const INITIALIZE_ID: i64 = 1;
const TOOLS_LIST_ID: i64 = 2;

/// How much stderr a probe keeps. Enough to explain a crash, small enough to stay a UI detail.
const STDERR_TAIL_LIMIT: usize = 4000;

/// How long cleanup may take after the probe decided the outcome.
const CLEANUP_TIMEOUT: Duration = Duration::from_millis(500);

pub(crate) async fn run(
    service: &McpService,
    server: &McpServerView,
    timeout: Duration,
) -> ServerProbe {
    match server.transport {
        McpTransport::Stdio => stdio_probe(server, timeout).await,
        McpTransport::Http => http_probe(service, server, timeout).await,
    }
}

fn connected(
    server: &McpServerView,
    tools: Vec<McpTool>,
    stderr_tail: Option<String>,
) -> ServerProbe {
    ServerProbe {
        name: server.name.clone(),
        source: server.source.clone(),
        transport: server.transport,
        state: ProbeState::Connected,
        tools,
        error: None,
        stderr_tail,
    }
}

/// A failed probe is still a successful command: the panel renders the state, the message and the
/// stderr tail. Only a resolution failure (an unknown server) is an `IpcError`.
fn failed(
    server: &McpServerView,
    error: impl Into<String>,
    stderr_tail: Option<String>,
) -> ServerProbe {
    ServerProbe {
        name: server.name.clone(),
        source: server.source.clone(),
        transport: server.transport,
        state: ProbeState::Error,
        tools: Vec::new(),
        error: Some(error.into()),
        stderr_tail,
    }
}

async fn stdio_probe(server: &McpServerView, timeout: Duration) -> ServerProbe {
    let Some(command) = server.command.clone() else {
        return failed(server, "the definition has no command to run", None);
    };

    let tail = Arc::new(Mutex::new(String::new()));
    let mut child = match spawn(&command, &server.args) {
        Ok(child) => child,
        Err(error) => return failed(server, error, None),
    };
    let stderr_task = drain_stderr(child.stderr.take(), Arc::clone(&tail));

    let outcome = tokio::time::timeout(timeout, handshake(&mut child)).await;

    // Whatever happened, the child is gone before this function returns.
    let _ = child.start_kill();
    let _ = tokio::time::timeout(CLEANUP_TIMEOUT, child.wait()).await;
    if let Some(task) = stderr_task {
        // The killed child closed its stderr; wait for the reader so the tail is complete.
        let _ = tokio::time::timeout(CLEANUP_TIMEOUT, task).await;
    }
    let stderr_tail = tail_snapshot(&tail);

    match outcome {
        Ok(Ok(tools)) => connected(server, tools, stderr_tail),
        Ok(Err(error)) => failed(server, error, stderr_tail),
        Err(_) => failed(
            server,
            format!("timed out after {}", describe(timeout)),
            stderr_tail,
        ),
    }
}

fn spawn(command: &str, args: &[String]) -> Result<Child, String> {
    let mut spawned = Command::new(command);
    // MCP servers speak over stdio; a console window would be a flash of noise on Windows.
    crate::process::hide_console_async(&mut spawned);
    spawned
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A dropped `Child` kills the process, so no code path can leak an orphan.
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| format!("could not start `{command}`: {err}"))
}

fn drain_stderr(
    stderr: Option<tokio::process::ChildStderr>,
    tail: Arc<Mutex<String>>,
) -> Option<JoinHandle<()>> {
    let mut stderr = stderr?;
    Some(tokio::spawn(async move {
        let mut buffer = [0u8; 1024];
        loop {
            match stderr.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let chunk = String::from_utf8_lossy(&buffer[..read]);
                    if let Ok(mut tail) = tail.lock() {
                        tail.push_str(&chunk);
                        keep_tail(&mut tail);
                    }
                }
            }
        }
    }))
}

/// Keep only the recent end of the tail: the last lines before a crash are the useful ones.
fn keep_tail(tail: &mut String) {
    if tail.len() <= STDERR_TAIL_LIMIT {
        return;
    }
    let mut cut = tail.len() - STDERR_TAIL_LIMIT;
    while cut < tail.len() && !tail.is_char_boundary(cut) {
        cut += 1;
    }
    tail.drain(..cut);
}

fn tail_snapshot(tail: &Arc<Mutex<String>>) -> Option<String> {
    let guard = tail.lock().ok()?;
    let trimmed = guard.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// `initialize`, then `notifications/initialized`, then `tools/list`, reading until the answer to
/// each request arrives. Notifications and anything unparsable are skipped, as a forward-compatible
/// client must.
async fn handshake(child: &mut Child) -> Result<Vec<McpTool>, String> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "the server has no stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "the server has no stdout".to_string())?;
    let mut lines = BufReader::new(stdout).lines();

    write_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": INITIALIZE_ID,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": crate::APP_NAME, "version": crate::VERSION },
            },
        }),
    )
    .await?;
    read_response(&mut lines, INITIALIZE_ID).await?;

    write_message(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await?;
    write_message(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": TOOLS_LIST_ID, "method": "tools/list", "params": {} }),
    )
    .await?;
    let listed = read_response(&mut lines, TOOLS_LIST_ID).await?;
    parse_tools(&listed)
}

async fn write_message(
    stdin: &mut tokio::process::ChildStdin,
    message: &Value,
) -> Result<(), String> {
    let mut line = serde_json::to_string(message).map_err(|err| err.to_string())?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|err| format!("could not write to the server: {err}"))?;
    stdin
        .flush()
        .await
        .map_err(|err| format!("could not write to the server: {err}"))
}

async fn read_response(
    lines: &mut Lines<BufReader<ChildStdout>>,
    id: i64,
) -> Result<Value, String> {
    loop {
        let Some(line) = lines
            .next_line()
            .await
            .map_err(|err| format!("could not read the server's stdout: {err}"))?
        else {
            return Err("the server closed its stdout before answering".to_string());
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Banners and log lines on stdout are noise, not a reason to fail a probe.
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if message.get("id").and_then(Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = message.get("error") {
            return Err(jsonrpc_error(error));
        }
        return Ok(message);
    }
}

fn jsonrpc_error(error: &Value) -> String {
    match error.get("message").and_then(Value::as_str) {
        Some(message) => format!("the server returned an error: {message}"),
        None => format!("the server returned an error: {error}"),
    }
}

fn parse_tools(message: &Value) -> Result<Vec<McpTool>, String> {
    let tools = message
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .ok_or_else(|| "tools/list returned no `result.tools` array".to_string())?;
    Ok(tools
        .iter()
        .filter_map(|tool| {
            // A tool without a name cannot be shown or called; skip it rather than fail the probe.
            let name = tool.get("name").and_then(Value::as_str)?;
            Some(McpTool {
                name: name.to_string(),
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|description| !description.is_empty())
                    .map(str::to_string),
            })
        })
        .collect())
}

async fn http_probe(
    service: &McpService,
    server: &McpServerView,
    timeout: Duration,
) -> ServerProbe {
    let Some(url) = server.url.clone() else {
        return failed(server, "the definition has no url to reach", None);
    };
    let request = service
        .http()
        .post(&url)
        .timeout(timeout)
        .header("accept", "application/json, text/event-stream")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": TOOLS_LIST_ID,
            "method": "tools/list",
            "params": {},
        }));
    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => return failed(server, format!("could not reach `{url}`: {err}"), None),
    };
    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(err) => return failed(server, format!("could not read the response: {err}"), None),
    };
    if !status.is_success() {
        return failed(
            server,
            format!("`{url}` returned {status}: {}", excerpt(&body)),
            None,
        );
    }

    let message = match parse_jsonrpc_body(&body) {
        Ok(message) => message,
        Err(error) => return failed(server, error, None),
    };
    if let Some(error) = message.get("error") {
        return failed(server, jsonrpc_error(error), None);
    }
    match parse_tools(&message) {
        Ok(tools) => connected(server, tools, None),
        Err(error) => failed(server, error, None),
    }
}

/// Streamable HTTP may answer with a JSON body or with server-sent events; take the first frame that
/// parses as JSON-RPC.
fn parse_jsonrpc_body(body: &str) -> Result<Value, String> {
    let body = body.trim();
    if body.is_empty() {
        return Err("the server returned an empty response".to_string());
    }
    if let Ok(message) = serde_json::from_str::<Value>(body) {
        return Ok(message);
    }
    for line in body.lines() {
        let Some(data) = line.trim_start().strip_prefix("data:") else {
            continue;
        };
        if let Ok(message) = serde_json::from_str::<Value>(data.trim()) {
            return Ok(message);
        }
    }
    Err("the server did not answer with JSON-RPC".to_string())
}

fn excerpt(body: &str) -> String {
    const LIMIT: usize = 200;
    let body = body.trim();
    if body.len() <= LIMIT {
        return body.to_string();
    }
    let mut end = LIMIT;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &body[..end])
}

fn describe(timeout: Duration) -> String {
    if timeout.as_millis() < 1000 {
        format!("{}ms", timeout.as_millis())
    } else {
        format!("{}s", timeout.as_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_jsonrpc_body_is_read_as_json() {
        let message =
            parse_jsonrpc_body("{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[]}}")
                .expect("plain JSON");
        assert_eq!(message["id"], 2);
    }

    #[test]
    fn a_server_sent_event_body_is_read_from_its_data_frames() {
        let body =
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[]}}\n\n";
        let message = parse_jsonrpc_body(body).expect("SSE frame");
        assert_eq!(message["id"], 2);
    }

    #[test]
    fn a_body_without_jsonrpc_is_refused() {
        assert!(parse_jsonrpc_body("").is_err());
        assert!(parse_jsonrpc_body("<html>nope</html>").is_err());
    }

    #[test]
    fn tools_without_a_name_are_skipped_and_blank_descriptions_are_none() {
        let message = json!({ "result": { "tools": [
            { "name": "read_file", "description": "Read a file" },
            { "name": "list_directory", "description": "  " },
            { "description": "nameless" }
        ] } });
        let tools = parse_tools(&message).expect("tools");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[1].name, "list_directory");
        assert_eq!(tools[1].description, None);
    }

    #[test]
    fn a_response_without_a_tools_array_is_an_error() {
        let error = parse_tools(&json!({ "result": {} })).unwrap_err();
        assert!(error.contains("result.tools"), "{error}");
    }

    #[test]
    fn the_stderr_tail_keeps_only_the_end() {
        let mut tail = "a".repeat(STDERR_TAIL_LIMIT * 2);
        tail.push_str("the last line");
        keep_tail(&mut tail);
        assert!(tail.len() <= STDERR_TAIL_LIMIT);
        assert!(tail.ends_with("the last line"));
    }

    #[test]
    fn a_jsonrpc_error_object_becomes_a_readable_message() {
        let message = jsonrpc_error(&json!({ "code": -32603, "message": "nope" }));
        assert!(message.contains("nope"), "{message}");
        let message = jsonrpc_error(&json!({ "code": -32603 }));
        assert!(message.contains("-32603"), "{message}");
    }

    #[test]
    fn timeouts_are_described_in_the_unit_a_human_reads() {
        assert_eq!(describe(Duration::from_millis(750)), "750ms");
        assert_eq!(describe(Duration::from_secs(10)), "10s");
    }
}
