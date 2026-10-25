//! The `pi` adapter: a host for Grove Sidecar Protocol v1, with pi behind the process boundary.
//!
//! pi's programmatic surface is its RPC mode (`pi --mode rpc`, JSONL on stdio). Rather than learn
//! pi's wire format here, this adapter launches a **sidecar** — the Node process in `sidecar/`, built
//! to `sidecar/dist/main.js` by `npm run build` — that speaks Grove Sidecar Protocol v1
//! (`docs/sidecar-protocol.md`): newline-delimited JSON-RPC 2.0, with a `session.event`
//! notification carrying an already-normalized `HarnessEvent`.
//!
//! The boundary is deliberate (see `architecture.md` §Sidecar protocol): the harness's wire format is
//! known only to the sidecar, so the adapter stays a transport. It does four things:
//!
//! 1. **Spawns** the sidecar: `config.command` (default `node`) and `config.args` (default
//!    `[<workspace>/sidecar/dist/main.js]`, discovered upwards from the executable, the crate root
//!    and the working directory; `config.sidecar` and `$GROVE_SIDECAR` name it explicitly).
//! 2. **Handshakes**: `initialize {protocolVersion: 1}` then
//!    `session.start {sessionId, cwd, model?, resumeToken?, config, context}`. A sidecar that cannot be
//!    launched, dies, times out, speaks another protocol version, or reports pi as `unavailable`
//!    becomes [`AdapterError::Unavailable`] with the reason it gave — never a half-started session.
//! 3. **Forwards** `session.event` notifications, re-validating each with serde. A malformed line, an
//!    unknown notification and an event the core cannot represent are logged and dropped; protocol
//!    drift is a log line, not a dead session.
//! 4. **Owns the process**: `session.send`/`session.cancel`/`session.stop` on the way in, and the
//!    whole tree killed on shutdown so a sidecar (and the `pi` it launched) cannot outlive the session.
//!
//! A dead sidecar is `Status { exited }` plus a retryable `Error`, exactly like a crashed harness.
//!
//! Command override is the documented hook: a user points `config.command`/`config.args` at a wrapper
//! (or a different sidecar implementation) and every test points it at
//! `fixtures/harnesses/pi_sidecar_fake.mjs`.
//!
//! Capability honesty (see [`PiAdapter::capabilities`]): pi has no tool-approval gate in its wire
//! protocol — its tools run without asking — so `permissions` is `false` and `respond_permission`
//! reports `Unsupported`; the sidecar logs extension UI requests instead of half-answering them.
//! `diffs` is `false` because pi sends no patch for edits.

use super::support::{find_on_path, kill_process_tree, truncate};
use super::{
    AdapterDeps, AdapterError, Capabilities, Detection, HarnessAdapter, HarnessId, HarnessSession,
    ResumeToken, StartContext, UserInput,
};
use crate::sessions::event::{Decision, EventKind, HarnessEvent, SessionState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

/// The harness's own name, as written into `SessionStarted` and stored on the session row.
const HARNESS: &str = "pi";
/// The sidecar is a Node program; Node is already a dependency of the build and probed by the doctor.
const DEFAULT_COMMAND: &str = "node";
/// Grove Sidecar Protocol v1.
const PROTOCOL_VERSION: u64 = 1;
/// A cold Windows runner has to start Node and (behind it) the harness.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
/// How long the sidecar gets to release pi itself after `session.stop` before the tree is killed.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const EVENT_BUFFER: usize = 1024;

pub struct PiAdapter {
    /// The sidecar launcher detection looks for. `config.command` can still override it per session.
    command: String,
}

impl Default for PiAdapter {
    fn default() -> Self {
        Self {
            command: DEFAULT_COMMAND.to_string(),
        }
    }
}

impl PiAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A different launcher for detection (a portable Node, a wrapper, a test double).
    pub fn with_command(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }

    /// The workspace sidecar build the adapter launches when nothing is configured: the first
    /// `sidecar/dist/main.js` found walking up from the executable, the crate root and the working
    /// directory. `None` means `npm run build` has not run (the doctor says so).
    pub fn default_sidecar_entry() -> Option<PathBuf> {
        default_sidecar_entry()
    }
}

#[async_trait::async_trait]
impl HarnessAdapter for PiAdapter {
    fn id(&self) -> HarnessId {
        HarnessId(HARNESS)
    }

    fn display_name(&self) -> &'static str {
        "pi"
    }

    fn skills_dir(&self) -> Option<&'static str> {
        Some(".pi/skills")
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // The sidecar forwards pi's `text_delta` frames as `Delta`.
            streaming_deltas: true,
            tool_calls: true,
            // pi sends no patch for edits; synthesizing one is a later ticket, not a quiet lie.
            diffs: false,
            // pi has no tool-approval gate: its tools run without asking (see the module docs).
            permissions: false,
            // `get_session_stats.sessionFile` is pi's resumable handle, reopened with
            // `switch_session`.
            resume: true,
            images: false,
            mcp: false,
            // `message_update.usage` / `turn_end` become `Usage`.
            usage: true,
        }
    }

    /// Side-effect free: it resolves Node on `PATH` and looks for the sidecar build; neither runs
    /// anything. Both are reported, because "pi is installed but the sidecar was never built" is the
    /// failure a user actually hits — and the doctor shows this detail in Settings.
    fn detect(&self, _deps: &AdapterDeps) -> Detection {
        let launcher = find_on_path(&self.command);
        match (launcher, default_sidecar_entry()) {
            (Some(node), Some(entry)) => Detection {
                installed: true,
                version: None,
                path: Some(crate::paths::display_path(&node)),
                detail: Some(format!(
                    "drives pi through the Node sidecar at {}",
                    crate::paths::display_path(&entry)
                )),
            },
            (None, _) => Detection::missing(format!(
                "`{}` is not on PATH; the pi adapter runs a Node sidecar",
                self.command
            )),
            (Some(_), None) => Detection::missing(
                "the Grove sidecar build is missing: run `npm run build` to produce \
                 sidecar/dist/main.js",
            ),
        }
    }

    async fn start(
        &self,
        ctx: StartContext,
        _deps: AdapterDeps,
    ) -> Result<Box<dyn HarnessSession>, AdapterError> {
        let (command, args) = sidecar_command(&ctx.config, &self.command)?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut process = Command::new(&command);
        process
            .args(&args)
            .current_dir(&ctx.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // A dropped session must not leak the sidecar (or, behind it, pi).
            .kill_on_drop(true);
        #[cfg(unix)]
        process.process_group(0);

        let mut child = process.spawn().map_err(|error| {
            AdapterError::Unavailable(format!(
                "could not launch the pi sidecar `{command}`: {error}; run `npm run build` and check \
                 config.command/config.args"
            ))
        })?;
        let pid = child.id();
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AdapterError::Other("the pi sidecar has no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AdapterError::Other("the pi sidecar has no stdout".into()))?;
        let mut lines = BufReader::new(stdout).lines();
        let stderr_task = child.stderr.take().map(|stderr| {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "grove::adapters::pi", "sidecar stderr: {line}");
                }
            })
        });

        let (events, events_rx) = mpsc::channel(EVENT_BUFFER);

        // --- handshake -------------------------------------------------------
        let initialize = json!({
            "jsonrpc": "2.0",
            "id": "initialize",
            "method": "initialize",
            "params": { "protocolVersion": PROTOCOL_VERSION },
        });
        let handshake = async {
            write_message(&mut stdin, &initialize).await?;
            let info = read_response(&mut lines, &events, &session_id, "initialize").await?;
            let version = info
                .get("protocolVersion")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if version != PROTOCOL_VERSION {
                return Err(format!(
                    "the pi sidecar speaks Grove Sidecar Protocol v{version}, this build speaks v{PROTOCOL_VERSION}"
                ));
            }
            let sidecar_version = info
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            tracing::debug!(
                target: "grove::adapters::pi",
                "sidecar {sidecar_version} initialised: pi {:?}",
                info.get("pi")
            );

            let mut params = json!({
                "sessionId": session_id,
                "cwd": crate::paths::display_path(&ctx.cwd),
                "config": ctx.config,
            });
            if let Some(model) = ctx.model.as_deref() {
                params["model"] = json!(model);
            }
            if let Some(prompt) = ctx.prompt.as_deref() {
                params["prompt"] = json!(prompt);
            }
            if !ctx.context_files.is_empty() {
                params["context"] = serde_json::to_value(&ctx.context_files)
                    .map_err(|error| format!("rendering the context documents failed: {error}"))?;
            }
            if let Some(token) = ctx.resume_token.as_deref() {
                params["resumeToken"] = json!(token);
            }
            write_message(
                &mut stdin,
                &json!({
                    "jsonrpc": "2.0",
                    "id": "session.start",
                    "method": "session.start",
                    "params": params,
                }),
            )
            .await?;
            let started = read_response(&mut lines, &events, &session_id, "session.start").await?;
            Ok::<Value, String>(started)
        }
        .await;

        let started = match handshake {
            Ok(started) => started,
            Err(reason) => {
                let _ = kill_process_tree(pid, &mut child).await;
                if let Some(task) = stderr_task {
                    task.abort();
                }
                return Err(AdapterError::Unavailable(format!(
                    "the pi sidecar handshake failed: {reason}"
                )));
            }
        };

        // The sidecar reports pi's session **file path**; keep the incoming token when it does not.
        let token = started
            .get("resumeToken")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| ctx.resume_token.clone());
        let token: ResumeToken = Arc::new(Mutex::new(token));

        let (commands, commands_rx) = mpsc::unbounded_channel();
        let actor = Actor {
            events: events.clone(),
            stdin,
            lines,
            child,
            pid,
            session_id,
            requests: 0,
            stderr_task,
        };
        tokio::spawn(actor.run(commands_rx));

        Ok(Box::new(PiSession {
            events: Some(events_rx),
            commands,
            token,
        }))
    }
}

enum SessionCommand {
    Send(String),
    Cancel,
    Shutdown,
}

pub struct PiSession {
    events: Option<mpsc::Receiver<HarnessEvent>>,
    commands: mpsc::UnboundedSender<SessionCommand>,
    token: ResumeToken,
}

impl PiSession {
    fn send_command(&self, command: SessionCommand) -> Result<(), AdapterError> {
        self.commands
            .send(command)
            .map_err(|_| AdapterError::Other("the pi session has ended".into()))
    }
}

#[async_trait::async_trait]
impl HarnessSession for PiSession {
    fn take_events(&mut self) -> mpsc::Receiver<HarnessEvent> {
        self.events
            .take()
            .expect("take_events is called exactly once by the session manager")
    }

    async fn send(&mut self, input: UserInput) -> Result<(), AdapterError> {
        self.send_command(SessionCommand::Send(input.text))
    }

    async fn respond_permission(
        &mut self,
        _request_id: &str,
        _decision: Decision,
    ) -> Result<(), AdapterError> {
        Err(AdapterError::Unsupported(
            "pi has no tool-approval gate: its tools run without asking, so there is no permission \
             request to answer"
                .into(),
        ))
    }

    async fn cancel(&mut self) -> Result<(), AdapterError> {
        self.send_command(SessionCommand::Cancel)
    }

    async fn shutdown(&mut self) -> Result<(), AdapterError> {
        // The manager owns the final `exited` transition, exactly like the other adapters.
        let _ = self.commands.send(SessionCommand::Shutdown);
        Ok(())
    }

    fn resume_token(&self) -> Option<String> {
        self.token.lock().ok().and_then(|token| token.clone())
    }

    fn resume_token_slot(&self) -> Option<ResumeToken> {
        Some(Arc::clone(&self.token))
    }
}

/// Owns the sidecar process for the life of the session: it reads stdout (notifications and the
/// responses to commands nobody waits for), writes commands, and kills the tree on shutdown.
struct Actor {
    events: mpsc::Sender<HarnessEvent>,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    child: Child,
    pid: Option<u32>,
    session_id: String,
    /// Request ids only need to be unique within a session; responses are logged, not awaited.
    requests: u64,
    stderr_task: Option<tokio::task::JoinHandle<()>>,
}

impl Actor {
    fn next_id(&mut self) -> String {
        self.requests += 1;
        format!("{}:{}", self.session_id, self.requests)
    }

    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<SessionCommand>) {
        loop {
            tokio::select! {
                line = self.lines.next_line() => match line {
                    Ok(Some(line)) => self.handle_line(&line).await,
                    Ok(None) => {
                        self.died("the pi sidecar closed its output").await;
                        return;
                    }
                    Err(error) => {
                        self.died(&format!("reading the pi sidecar failed: {error}")).await;
                        return;
                    }
                },
                command = commands.recv() => match command {
                    Some(SessionCommand::Send(text)) => {
                        let id = self.next_id();
                        let message = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "method": "session.send",
                            "params": { "sessionId": self.session_id, "text": text },
                        });
                        if let Err(reason) = self.request(&message).await {
                            self.died(&reason).await;
                            return;
                        }
                    }
                    Some(SessionCommand::Cancel) => {
                        let id = self.next_id();
                        let message = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "method": "session.cancel",
                            "params": { "sessionId": self.session_id },
                        });
                        if let Err(reason) = self.request(&message).await {
                            self.died(&reason).await;
                            return;
                        }
                    }
                    Some(SessionCommand::Shutdown) | None => {
                        self.stop().await;
                        return;
                    }
                },
            }
        }
    }

    async fn handle_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            // A sidecar that writes a stray newline (or CRLF) is not speaking a different protocol.
            return;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                tracing::debug!(
                    target: "grove::adapters::pi",
                    "skipping a malformed sidecar line ({error}): {}",
                    truncate(line, 200)
                );
                return;
            }
        };
        if let Some(method) = value.get("method").and_then(Value::as_str) {
            forward_notification(&self.events, &self.session_id, method, &value).await;
            return;
        }
        // Responses are acknowledgements for `session.send`/`session.cancel`; failures surface as
        // events, so there is nothing to do with them here.
        tracing::debug!(
            target: "grove::adapters::pi",
            "ignoring a sidecar response: {}",
            truncate(line, 200)
        );
    }

    async fn request(&mut self, message: &Value) -> Result<(), String> {
        write_message(&mut self.stdin, message).await
    }

    /// The sidecar is gone: report it, then make sure nothing is left behind.
    async fn died(&mut self, reason: &str) {
        let _ = self
            .events
            .send(HarnessEvent::new(EventKind::Error {
                message: format!("{reason}; resume the session to continue"),
                retryable: true,
            }))
            .await;
        let _ = self
            .events
            .send(HarnessEvent::new(EventKind::Status {
                state: SessionState::Exited,
            }))
            .await;
        let _ = kill_process_tree(self.pid, &mut self.child).await;
        if let Some(task) = self.stderr_task.take() {
            task.abort();
        }
    }

    async fn stop(&mut self) {
        let id = self.next_id();
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session.stop",
            "params": { "sessionId": self.session_id },
        });
        let _ = write_message(&mut self.stdin, &message).await;
        // The sidecar kills pi and exits on `session.stop`; if it does not, the tree goes with it.
        if tokio::time::timeout(SHUTDOWN_GRACE, self.child.wait())
            .await
            .is_err()
        {
            let _ = kill_process_tree(self.pid, &mut self.child).await;
        }
        if let Some(task) = self.stderr_task.take() {
            task.abort();
        }
    }
}

/// Write one JSON-RPC message as a single line and flush it: the sidecar frames on LF only.
async fn write_message(stdin: &mut ChildStdin, message: &Value) -> Result<(), String> {
    let mut line = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    line.push(b'\n');
    stdin
        .write_all(&line)
        .await
        .map_err(|error| format!("writing to the pi sidecar failed: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("flushing the pi sidecar failed: {error}"))
}

/// Read lines until the response with `id` arrives, forwarding any notification that comes first.
async fn read_response(
    lines: &mut Lines<BufReader<ChildStdout>>,
    events: &mpsc::Sender<HarnessEvent>,
    session_id: &str,
    id: &str,
) -> Result<Value, String> {
    let deadline = tokio::time::Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(format!(
                "`{id}` was not answered within {HANDSHAKE_TIMEOUT:?}"
            ));
        }
        let line = match tokio::time::timeout(remaining, lines.next_line()).await {
            Ok(Ok(Some(line))) => line,
            Ok(Ok(None)) => return Err("the sidecar exited before answering the handshake".into()),
            Ok(Err(error)) => return Err(format!("reading the sidecar failed: {error}")),
            Err(_) => {
                return Err(format!(
                    "`{id}` was not answered within {HANDSHAKE_TIMEOUT:?}"
                ))
            }
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                tracing::debug!(
                    target: "grove::adapters::pi",
                    "skipping a malformed sidecar line ({error}): {}",
                    truncate(line, 200)
                );
                continue;
            }
        };
        if let Some(method) = value.get("method").and_then(Value::as_str) {
            forward_notification(events, session_id, method, &value).await;
            continue;
        }
        if value.get("id").and_then(Value::as_str) != Some(id) {
            tracing::debug!(
                target: "grove::adapters::pi",
                "ignoring an unexpected sidecar message during the handshake: {}",
                truncate(line, 200)
            );
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(error_reason(error));
        }
        return Ok(value.get("result").cloned().unwrap_or(Value::Null));
    }
}

/// A `session.event` notification becomes one `HarnessEvent`; anything else is logged and dropped.
async fn forward_notification(
    events: &mpsc::Sender<HarnessEvent>,
    session_id: &str,
    method: &str,
    value: &Value,
) {
    if method != "session.event" {
        tracing::debug!(
            target: "grove::adapters::pi",
            "ignoring a sidecar notification: {method}"
        );
        return;
    }
    let params = value.get("params");
    let incoming = params
        .and_then(|params| params.get("sessionId"))
        .and_then(Value::as_str);
    if incoming != Some(session_id) {
        tracing::debug!(
            target: "grove::adapters::pi",
            "dropping a session.event for another session: {incoming:?}"
        );
        return;
    }
    let Some(event) = params.and_then(|params| params.get("event")) else {
        tracing::debug!(
            target: "grove::adapters::pi",
            "dropping a session.event without an event payload"
        );
        return;
    };
    match serde_json::from_value::<HarnessEvent>(event.clone()) {
        Ok(event) => {
            // A closed channel means the manager dropped the session; the actor ends on its own.
            if events.send(event).await.is_err() {
                tracing::debug!(
                    target: "grove::adapters::pi",
                    "the session stopped accepting events"
                );
            }
        }
        Err(error) => tracing::warn!(
            target: "grove::adapters::pi",
            "dropping a sidecar event the core cannot represent: {error}"
        ),
    }
}

fn error_reason(error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("the sidecar reported an error");
    let reason = error
        .get("data")
        .and_then(|data| data.get("reason"))
        .and_then(Value::as_str)
        .unwrap_or(message);
    reason.to_string()
}

/// `config.command`/`config.args` replace the launcher; with no args, the workspace sidecar build is
/// the single default argument.
fn sidecar_command(
    config: &Value,
    default_command: &str,
) -> Result<(String, Vec<String>), AdapterError> {
    let command = config
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .unwrap_or(default_command)
        .to_string();
    if let Some(args) = config.get("args").and_then(Value::as_array) {
        let args = args
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        return Ok((command, args));
    }
    match sidecar_entry(config) {
        Some(entry) => Ok((command, vec![crate::paths::display_path(&entry)])),
        None => Err(AdapterError::Unavailable(
            "the Grove sidecar build is missing: run `npm run build` (it writes \
             sidecar/dist/main.js) or point config.args at a sidecar entry point"
                .into(),
        )),
    }
}

/// `config.sidecar`, else `$GROVE_SIDECAR`, else the workspace build.
fn sidecar_entry(config: &Value) -> Option<PathBuf> {
    if let Some(configured) = config
        .get("sidecar")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        return Some(PathBuf::from(configured));
    }
    if let Some(entry) = std::env::var_os("GROVE_SIDECAR") {
        let entry = PathBuf::from(entry);
        if !entry.as_os_str().is_empty() {
            return Some(entry);
        }
    }
    default_sidecar_entry()
}

fn default_sidecar_entry() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        roots.extend(executable.ancestors().map(Path::to_path_buf));
    }
    // `target/debug/deps/…` walks up to the repository root, and the crate root is right there for
    // a workspace build; `cwd` covers a packaged layout that keeps the sidecar beside the app.
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().map(Path::to_path_buf));
    }
    for root in roots {
        let candidate = root.join("sidecar").join("dist").join("main.js");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn config_overrides_the_launcher_and_its_arguments() {
        let config = json!({
            "command": "deno",
            "args": ["C:/grove/sidecar/main.ts", "--flag", 42],
        });
        let (command, args) = sidecar_command(&config, "node").expect("configured launcher");
        assert_eq!(command, "deno");
        assert_eq!(
            args,
            strings(&["C:/grove/sidecar/main.ts", "--flag"]),
            "non-string args are ignored rather than fatal"
        );
    }

    #[test]
    fn without_args_the_workspace_sidecar_build_is_the_entry_point() {
        let config = json!({ "sidecar": "C:/grove/sidecar/dist/main.js" });
        let (command, args) = sidecar_command(&config, "node").expect("a configured sidecar");
        assert_eq!(command, "node");
        assert_eq!(args, strings(&["C:/grove/sidecar/dist/main.js"]));
    }

    #[test]
    fn a_missing_sidecar_build_is_an_actionable_unavailable() {
        // Point discovery somewhere that cannot contain a build, and make sure the message says what
        // to do instead of failing at spawn time with ENOENT.
        let dir = tempfile::tempdir().expect("tempdir");
        let config = json!({ "sidecar": dir.path().join("missing.js").to_string_lossy() });
        // A configured entry is used as given, so this one launches and fails at spawn; the actionable
        // message is produced when nothing is configured at all.
        let (_, args) = sidecar_command(&config, "node").expect("a configured entry is used");
        assert_eq!(args.len(), 1);

        let discovered = default_sidecar_entry();
        if discovered.is_none() {
            let error = sidecar_command(&json!({}), "node").expect_err("no build, no launch");
            assert!(
                matches!(&error, AdapterError::Unavailable(reason) if reason.contains("npm run build")),
                "{error:?}"
            );
        }
    }

    #[test]
    fn the_protocol_error_reason_prefers_the_sidecars_own_reason() {
        let error = json!({
            "code": -32000,
            "message": "pi is unavailable",
            "data": { "reason": "pi is not installed or not on PATH (looked for `pi`)" },
        });
        assert_eq!(
            error_reason(&error),
            "pi is not installed or not on PATH (looked for `pi`)"
        );
        assert_eq!(
            error_reason(&json!({ "code": -32000, "message": "boom" })),
            "boom"
        );
        assert_eq!(
            error_reason(&json!({ "code": -32000 })),
            "the sidecar reported an error"
        );
    }

    #[test]
    fn capabilities_are_honest() {
        let capabilities = PiAdapter::default().capabilities();
        assert!(capabilities.streaming_deltas, "pi's text_delta is mapped");
        assert!(capabilities.tool_calls);
        assert!(capabilities.resume, "the session file path round trips");
        assert!(capabilities.usage);
        assert!(
            !capabilities.permissions,
            "pi has no tool-approval gate in its wire protocol"
        );
        assert!(!capabilities.diffs, "no patches are synthesized yet");
        assert!(!capabilities.images);
        assert!(!capabilities.mcp);
    }
}
