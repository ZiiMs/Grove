//! The Agent Client Protocol adapter: **any** ACP-speaking agent, one JSON-RPC 2.0 stream over stdio.
//!
//! ACP is a protocol, not a product, so this module knows no harness name: the command line comes
//! entirely from `config.command` and `config.args`. With no command configured there is nothing to
//! detect and nothing to launch, so [`AcpAdapter::detect`] reports "not installed" with the setting to
//! change and [`HarnessAdapter::start`] fails [`AdapterError::Unavailable`] — never a panic, never a
//! guessed binary. Detection never spawns anything.
//!
//! Launch sequence (`protocols.md` §3, pinned to `protocolVersion: 1`; v2 is a draft):
//!
//! 1. `initialize` with `protocolVersion: 1`, honest `clientCapabilities` and `clientInfo`;
//!    a version the agent answers with that Grove does not speak ends the session as `error`.
//! 2. `session/new` with the session `cwd` and the configured `mcpServers`, or `session/resume` with
//!    the stored session id. `session/load` is never used: it **replays the whole conversation** as
//!    `session/update` notifications, which would duplicate Grove's append-only transcript.
//! 3. `session/prompt` per user message — one long-lived request per turn, answered with a
//!    `stopReason`; `session/cancel` (a notification) cuts it short.
//!
//! `session/update` mapping, and nothing more (`Capabilities` matches this table exactly):
//!
//! | `session/update` | Grove events |
//! |---|---|
//! | `agent_message_chunk` with text, same `messageId` | `Delta { text }` per chunk |
//! | `agent_message_chunk`, `messageId` changes or the turn ends | `Message { assistant, [Text] }` for the text streamed so far |
//! | `agent_thought_chunk` | `Message { assistant, [Thinking] }` |
//! | `tool_call` | `ToolCall { call_id: toolCallId, name, title }` |
//! | `tool_call_update` with `completed` / `failed` | `ToolResult { call_id, ok / error, summary }` |
//! | `tool_call_update` with `pending` / `in_progress` | logged (progress only) |
//! | `user_message_chunk` | ignored: it is the replay echo that `session/load` would produce |
//! | `plan`, `usage_update`, `current_mode_update`, `config_option_update`, `session_info_update`, `available_commands_update`, `notice`, `compaction_*` | logged and ignored |
//! | unknown `sessionUpdate` variant, unknown notification | logged and ignored |
//! | unknown **request** (anything with an id) | answered with JSON-RPC `-32601`, so an agent never waits |
//! | `session/request_permission` | `PermissionRequest { request_id: <jsonrpc id>, title, detail }`, answered with `{outcome:{outcome:"selected", optionId}}` or `{outcome:{outcome:"cancelled"}}` |
//! | `session/prompt` result | `stopReason` `end_turn`/`cancelled` → `Status { idle }`; `refusal`, `max_tokens`, `max_turn_requests` → `Error` + `Status { idle }` |
//! | transport close | `Status { exited }` |
//!
//! Deliberate limits, reflected in [`AcpAdapter::capabilities`]:
//!
//! - **`diffs` is false.** ACP `diff` tool content is summarized, not turned into a patch yet.
//! - **`usage` is false.** `usage_update` reports context-window fill (`used`/`size`) and cost, not the
//!   input/output token split `Usage` carries, so nothing is invented.
//! - **`images` is false.** Prompts are text blocks; the agent's `promptCapabilities` are not trusted
//!   to accept images Grove cannot send.
//! - **`mcp` is false.** `config.mcpServers` is forwarded verbatim (the protocol requires the field),
//!   but Grove's own MCP catalog is not wired into ACP sessions yet.
//! - **No `authenticate`.** An agent that advertises `authMethods` is reported instead of silently
//!   failing on every turn: Grove inherits the user's own login, and the error says so.
//! - **No model selection.** ACP v1's `session/new` carries the working directory and MCP servers,
//!   not a model: `StartContext.model` is recorded on `SessionStarted`, and the agent's own
//!   configuration decides what actually runs.
//! - **No `session/close`.** Shutdown kills the agent's process tree and leaves the `exited`
//!   transition to the session manager; a graceful close would need `sessionCapabilities.close` and
//!   a timeout, and buys nothing while the process is ours to end.
//!
//! A cancelled turn keeps its `Delta`s but never completes a `Message`: fragments that arrive between
//! `session/cancel` and the prompt's `cancelled` stop reason are still accepted (the spec says the
//! agent may send them), and the buffered text is discarded rather than shown as a finished answer.
//!
//! Permissions are the one place where "always answer" matters more than anything else: every
//! `session/request_permission` gets a response, including when the user cancels — the spec says a
//! client **MUST** answer `cancelled` on cancellation or the agent may hang. Answering a request that
//! arrived outside an active turn is answered `cancelled` immediately for the same reason.

use super::{
    AdapterDeps, AdapterError, Capabilities, ContextDoc, Detection, HarnessAdapter, HarnessId,
    HarnessSession, ResumeToken, StartContext, UserInput,
};
use crate::sessions::event::{
    Block, Decision, EventKind, HarnessEvent, Role, SessionState, ToolStatus,
};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

/// The harness name written into `SessionStarted` and stored on the session row.
const HARNESS: &str = "acp";
/// The ACP major version this adapter speaks. v2 is a draft with renames; do not negotiate up.
const PROTOCOL_VERSION: u64 = 1;
const LOG_TARGET: &str = "grove::adapters::acp";
const EVENT_BUFFER: usize = 1024;
/// A tool result can be a whole file; the transcript wants a summary.
const SUMMARY_LIMIT: usize = 500;
/// How long the agent gets to confirm a cancelled turn before Grove ends it locally.
const CANCEL_TIMEOUT: Duration = Duration::from_secs(5);
/// How long to wait for a child that closed stdout to also exit before killing it.
const REAP_TIMEOUT: Duration = Duration::from_secs(5);

/// Any ACP agent. There is no default binary: the command is configuration, not detection.
#[derive(Default)]
pub struct AcpAdapter {
    /// The command detection looks for. `config.command` still overrides it per session.
    command: Option<String>,
}

impl AcpAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A hint for [`HarnessAdapter::detect`] only (a wrapper, a versioned install, a test double).
    /// Starting a session always uses `config.command`, so detection never gates a session.
    pub fn with_command(command: impl Into<String>) -> Self {
        Self {
            command: Some(command.into()),
        }
    }
}

#[async_trait::async_trait]
impl HarnessAdapter for AcpAdapter {
    fn id(&self) -> HarnessId {
        HarnessId(HARNESS)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // One `Delta` per `agent_message_chunk`, completed by a `Message` at the message boundary.
            streaming_deltas: true,
            tool_calls: true,
            // ACP sends `diff` tool content, but no patch is synthesized yet.
            diffs: false,
            // Every request is answered, including `cancelled` on cancel.
            permissions: true,
            // `session/resume` continues the same conversation without replaying it.
            resume: true,
            images: false,
            // The field is forwarded, but Grove's MCP catalog is not wired in yet.
            mcp: false,
            // `usage_update` is context-window fill, not an input/output token pair.
            usage: false,
        }
    }

    /// Side-effect free: it resolves the configured command on `PATH` and never runs it. A generic
    /// adapter has no binary to guess at, so the default is "not installed, here is what to set".
    fn detect(&self, _deps: &AdapterDeps) -> Detection {
        let Some(command) = self.command.as_deref() else {
            return Detection::missing(
                "no ACP agent command is configured: set the agent command in Settings \
                 (adapter config `command`, with its flags in `args`)",
            );
        };
        match find_on_path(command) {
            Some(path) => Detection {
                installed: true,
                // `--version` belongs to the doctor, which is allowed to run probes.
                version: None,
                path: Some(crate::paths::display_path(&path)),
                detail: None,
            },
            None => Detection::missing(format!(
                "`{command}` is not on PATH; set the ACP agent command in Settings (adapter config \
                 `command`)"
            )),
        }
    }

    async fn start(
        &self,
        ctx: StartContext,
        _deps: AdapterDeps,
    ) -> Result<Box<dyn HarnessSession>, AdapterError> {
        let launch = Launch::from_config(&ctx.config)?;

        let mut command = Command::new(&launch.command);
        command
            .args(&launch.args)
            .current_dir(&ctx.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // A dropped session must not leak an agent process.
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);

        let mut child = command.spawn().map_err(|error| {
            AdapterError::Unavailable(format!(
                "could not launch the ACP agent `{}`: {error}",
                launch.command
            ))
        })?;
        let pid = child.id();
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AdapterError::Other("the ACP agent has no stdin pipe".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AdapterError::Other("the ACP agent has no stdout pipe".into()))?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(log_stderr(stderr));
        }

        let (frames, frames_rx) = mpsc::channel(EVENT_BUFFER);
        tokio::spawn(read_frames(stdout, frames));
        let (writer, writer_rx) = mpsc::unbounded_channel();
        tokio::spawn(write_frames(stdin, writer_rx));

        let (events, events_rx) = mpsc::channel(EVENT_BUFFER);
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let token: ResumeToken = Arc::new(Mutex::new(ctx.resume_token.clone()));
        // A replayed tail (`StartContext.prompt`) and the pinned documents ride the first prompt only.
        let mut pending_prompts = VecDeque::new();
        if let Some(prompt) = render_prompt(ctx.prompt.as_deref(), &ctx.context_files) {
            pending_prompts.push_back(prompt);
        }
        let actor = Actor {
            events,
            token: Arc::clone(&token),
            writer,
            next_id: 1,
            session_id: ctx.resume_token.clone(),
            model: ctx.model.clone(),
            cwd: crate::paths::display_path(&ctx.cwd),
            mcp_servers: launch.mcp_servers,
            pending_prompts,
            pending_permissions: HashMap::new(),
            message: MessageBuffer::default(),
            turn_active: false,
            turn_cancelled: false,
            cancel_deadline: None,
            shutdown: false,
            child: Some(child),
            pid,
        };
        tokio::spawn(run(actor, commands_rx, frames_rx));

        Ok(Box::new(AcpSession {
            events: Some(events_rx),
            commands,
            token,
        }))
    }
}

/// The documented `config.command` / `config.args` override, plus the MCP servers to forward.
#[derive(Debug)]
struct Launch {
    command: String,
    args: Vec<String>,
    mcp_servers: Vec<Value>,
}

impl Launch {
    fn from_config(config: &Value) -> Result<Self, AdapterError> {
        let command = config
            .get("command")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .ok_or_else(|| {
                AdapterError::Unavailable(
                    "no ACP agent command is configured: set the agent command in Settings \
                     (adapter config `command`, with its flags in `args`) and resume the session"
                        .into(),
                )
            })?
            .to_string();
        let args = config
            .get("args")
            .and_then(Value::as_array)
            .map(|args| {
                args.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        // Passed through verbatim: ACP stdio transport is what every agent MUST support, and Grove
        // does not interpret the server definitions.
        let mcp_servers = config
            .get("mcpServers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(Self {
            command,
            args,
            mcp_servers,
        })
    }
}

/// A command from the session manager. `Permission` carries an acknowledgement so a caller learns
/// whether the request was really pending instead of getting a silent no-op.
enum SessionCommand {
    Send(String),
    Cancel,
    Permission {
        request_id: String,
        decision: Decision,
        ack: oneshot::Sender<Result<(), AdapterError>>,
    },
    Shutdown,
}

pub struct AcpSession {
    events: Option<mpsc::Receiver<HarnessEvent>>,
    commands: mpsc::UnboundedSender<SessionCommand>,
    token: ResumeToken,
}

impl AcpSession {
    fn send_command(&self, command: SessionCommand) -> Result<(), AdapterError> {
        self.commands
            .send(command)
            .map_err(|_| AdapterError::Other("the acp session has ended".into()))
    }
}

#[async_trait::async_trait]
impl HarnessSession for AcpSession {
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
        request_id: &str,
        decision: Decision,
    ) -> Result<(), AdapterError> {
        let (ack, acknowledged) = oneshot::channel();
        self.send_command(SessionCommand::Permission {
            request_id: request_id.to_string(),
            decision,
            ack,
        })?;
        match acknowledged.await {
            Ok(result) => result,
            Err(_) => Err(AdapterError::Other("the acp session has ended".into())),
        }
    }

    async fn cancel(&mut self) -> Result<(), AdapterError> {
        self.send_command(SessionCommand::Cancel)
    }

    async fn shutdown(&mut self) -> Result<(), AdapterError> {
        // The manager owns the final `exited` transition, exactly like the mock adapter.
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

/// Why the connection cannot be used. Every variant ends the session instead of leaving it hanging.
enum Failure {
    /// The agent answered `initialize` with a version Grove cannot speak.
    VersionMismatch { agent: String },
    /// The agent refused a request (a JSON-RPC error), needs authentication, or broke the handshake.
    Refused(String),
    /// The agent's output ended (process exit, pipe closed).
    TransportClosed,
    /// The manager shut the session down.
    Shutdown,
}

impl Failure {
    fn describe(&self) -> String {
        match self {
            Failure::VersionMismatch { agent } => {
                format!("the ACP agent speaks protocol version {agent}")
            }
            Failure::Refused(reason) => reason.clone(),
            Failure::TransportClosed => "the ACP transport is closed".to_string(),
            Failure::Shutdown => "the session is shutting down".to_string(),
        }
    }
}

/// The streamed text of the message currently being assembled.
#[derive(Default)]
struct MessageBuffer {
    id: Option<String>,
    text: String,
}

/// A `session/request_permission` that is waiting for the user.
struct PendingPermission {
    /// The exact JSON-RPC id the answer must echo (agents may use numbers or strings).
    id: Value,
    options: Vec<Value>,
}

/// The single writer for a session: it owns the agent process, its pipes and the turn state.
struct Actor {
    events: mpsc::Sender<HarnessEvent>,
    token: ResumeToken,
    /// JSON-RPC frames for the agent's stdin, newline-delimited.
    writer: mpsc::UnboundedSender<String>,
    next_id: u64,
    session_id: Option<String>,
    model: Option<String>,
    cwd: String,
    mcp_servers: Vec<Value>,
    /// Prompts that arrived while a turn (or the handshake) was running; they run in order.
    pending_prompts: VecDeque<String>,
    pending_permissions: HashMap<String, PendingPermission>,
    message: MessageBuffer,
    turn_active: bool,
    turn_cancelled: bool,
    cancel_deadline: Option<Instant>,
    shutdown: bool,
    child: Option<Child>,
    pid: Option<u32>,
}

impl Actor {
    async fn emit(&self, kind: EventKind) {
        // A closed channel means the manager dropped the session; the actor stops on its own.
        let _ = self.events.send(HarnessEvent::new(kind)).await;
    }

    async fn status(&self, state: SessionState) {
        self.emit(EventKind::Status { state }).await;
    }

    fn set_session_id(&self, session_id: &str) {
        if let Ok(mut token) = self.token.lock() {
            *token = Some(session_id.to_string());
        }
    }

    fn write(&self, frame: Value) -> Result<(), Failure> {
        self.writer
            .send(frame.to_string())
            .map_err(|_| Failure::TransportClosed)
    }

    async fn request(
        &mut self,
        method: &str,
        params: Value,
        commands: &mut mpsc::UnboundedReceiver<SessionCommand>,
        frames: &mut mpsc::Receiver<Value>,
    ) -> Result<Value, Failure> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;
        self.await_response(id, method, commands, frames).await
    }

    /// Wait for the response to `id`, servicing everything else that arrives meanwhile.
    async fn await_response(
        &mut self,
        id: u64,
        method: &str,
        commands: &mut mpsc::UnboundedReceiver<SessionCommand>,
        frames: &mut mpsc::Receiver<Value>,
    ) -> Result<Value, Failure> {
        loop {
            tokio::select! {
                frame = frames.recv() => {
                    let Some(frame) = frame else {
                        return Err(Failure::TransportClosed);
                    };
                    if response_id(&frame) == Some(id) {
                        if let Some(error) = frame.get("error") {
                            return Err(Failure::Refused(format!(
                                "the ACP agent rejected `{method}`: {}",
                                describe_error(error)
                            )));
                        }
                        return Ok(frame.get("result").cloned().unwrap_or(Value::Null));
                    }
                    self.handle_frame(frame).await;
                }
                command = commands.recv() => match command {
                    Some(SessionCommand::Send(prompt)) => self.pending_prompts.push_back(prompt),
                    Some(SessionCommand::Cancel) => tracing::debug!(
                        target: LOG_TARGET,
                        "ignoring a cancel: no turn is running yet"
                    ),
                    Some(SessionCommand::Permission { request_id, decision, ack }) => {
                        let result = self.answer_permission(&request_id, decision, false).await;
                        let _ = ack.send(result);
                    }
                    Some(SessionCommand::Shutdown) | None => {
                        self.shutdown = true;
                        return Err(Failure::Shutdown);
                    }
                },
            }
        }
    }

    /// `initialize` → `session/new` (or `session/resume`), before the first prompt.
    async fn handshake(
        &mut self,
        commands: &mut mpsc::UnboundedReceiver<SessionCommand>,
        frames: &mut mpsc::Receiver<Value>,
    ) -> Result<(), Failure> {
        let result = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    // Honest: Grove implements neither file-system nor terminal methods, so an agent
                    // must not call them. Advertising them would be a hang waiting to happen.
                    "clientCapabilities": {
                        "fs": { "readTextFile": false, "writeTextFile": false },
                        "terminal": false,
                    },
                    "clientInfo": {
                        "name": "grove",
                        "title": "Grove",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                }),
                commands,
                frames,
            )
            .await?;

        let version = result.get("protocolVersion").and_then(Value::as_u64);
        if version != Some(PROTOCOL_VERSION) {
            return Err(Failure::VersionMismatch {
                agent: version
                    .map(|version| version.to_string())
                    .unwrap_or_else(|| "an unknown version".to_string()),
            });
        }
        tracing::debug!(
            target: LOG_TARGET,
            "initialised: agentInfo {:?}, capabilities {:?}",
            result.get("agentInfo"),
            result.get("agentCapabilities")
        );
        let auth_methods = result
            .get("authMethods")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        if auth_methods > 0 {
            // Grove inherits the user's own login; it cannot drive an interactive auth flow.
            return Err(Failure::Refused(format!(
                "the ACP agent requires authentication ({auth_methods} method(s) advertised): sign \
                 in with the agent's own CLI, then resume the session"
            )));
        }

        let requested = self.session_id.clone();
        let resume = requested.is_some() && supports_resume(&result);
        if requested.is_some() && !resume {
            // `session/load` would replay the whole conversation into Grove's append-only transcript.
            // A clean conversation is better than a duplicated one, so say what happened and move on.
            self.emit(EventKind::Error {
                message: "this ACP agent does not advertise `sessionCapabilities.resume`, so it \
                          cannot continue the previous conversation: Grove's transcript is intact, \
                          and the agent is starting a new session"
                    .into(),
                retryable: false,
            })
            .await;
        }

        let mut params = json!({
            "cwd": self.cwd.clone(),
            "mcpServers": self.mcp_servers.clone(),
        });
        let method = if resume {
            params["sessionId"] = json!(requested.clone());
            "session/resume"
        } else {
            "session/new"
        };
        let result = self.request(method, params, commands, frames).await?;
        let session_id = result
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(requested)
            .ok_or_else(|| Failure::Refused(format!("`{method}` did not return a session id")))?;
        self.set_session_id(&session_id);
        tracing::debug!(target: LOG_TARGET, "`{method}` established session {session_id}");
        self.session_id = Some(session_id);
        Ok(())
    }

    /// One turn: send `session/prompt`, map its updates, and end on the prompt's response.
    async fn turn(
        &mut self,
        prompt: String,
        commands: &mut mpsc::UnboundedReceiver<SessionCommand>,
        frames: &mut mpsc::Receiver<Value>,
    ) {
        self.status(SessionState::Busy).await;
        self.message = MessageBuffer::default();
        self.turn_cancelled = false;
        self.cancel_deadline = None;

        let Some(session_id) = self.session_id.clone() else {
            // Unreachable through the manager (no session id means the actor ended), but a wire-level
            // invariant must never turn into a panic.
            self.emit(EventKind::Error {
                message: "the ACP session was never established".into(),
                retryable: false,
            })
            .await;
            self.status(SessionState::Error).await;
            return;
        };

        self.turn_active = true;
        // ACP prompt blocks: text only, matching `capabilities().images == false`.
        let id = self.next_id;
        self.next_id += 1;
        if let Err(failure) = self.write(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session/prompt",
            "params": {
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": prompt }],
            },
        })) {
            self.turn_active = false;
            self.fail(failure).await;
            return;
        }

        loop {
            let far_future = Instant::now() + Duration::from_secs(3600);
            let deadline = self.cancel_deadline.unwrap_or(far_future);
            tokio::select! {
                frame = frames.recv() => {
                    let Some(frame) = frame else {
                        // The agent died mid-turn: persist the error, then mark the session exited.
                        self.turn_active = false;
                        self.turn_cancelled = false;
                        self.cancel_deadline = None;
                        self.message = MessageBuffer::default();
                        self.emit(EventKind::Error {
                            message: "the ACP agent exited before the turn finished".into(),
                            retryable: true,
                        })
                        .await;
                        self.status(SessionState::Exited).await;
                        self.shutdown = true;
                        return;
                    };
                    if response_id(&frame) == Some(id) {
                        self.turn_active = false;
                        self.cancel_deadline = None;
                        self.finish_turn(&frame).await;
                        return;
                    }
                    self.handle_frame(frame).await;
                }
                command = commands.recv() => match command {
                    Some(SessionCommand::Send(prompt)) => self.pending_prompts.push_back(prompt),
                    Some(SessionCommand::Cancel) => {
                        if self.turn_cancelled {
                            tracing::debug!(target: LOG_TARGET, "the turn is already cancelled");
                        } else {
                            self.turn_cancelled = true;
                            self.cancel_deadline = Some(Instant::now() + CANCEL_TIMEOUT);
                            let _ = self.write(json!({
                                "jsonrpc": "2.0",
                                "method": "session/cancel",
                                "params": { "sessionId": self.session_id.clone() },
                            }));
                            // The spec is explicit: answer every pending permission `cancelled`, or the
                            // agent may keep waiting on an answer that will never come.
                            let pending: Vec<String> =
                                self.pending_permissions.keys().cloned().collect();
                            for request_id in pending {
                                if let Err(error) =
                                    self.answer_permission(&request_id, Decision::Deny, true).await
                                {
                                    tracing::warn!(
                                        target: LOG_TARGET,
                                        "could not cancel the pending permission {request_id}: {error}"
                                    );
                                }
                            }
                        }
                    }
                    Some(SessionCommand::Permission { request_id, decision, ack }) => {
                        let result = self.answer_permission(&request_id, decision, false).await;
                        let _ = ack.send(result);
                    }
                    Some(SessionCommand::Shutdown) | None => {
                        self.turn_active = false;
                        self.shutdown = true;
                        self.kill().await;
                        return;
                    }
                },
                _ = tokio::time::sleep_until(deadline), if self.cancel_deadline.is_some() => {
                    // The agent never confirmed the cancel. End the turn locally rather than hanging
                    // the UI on a promise the agent may never keep; the session stays usable.
                    tracing::warn!(
                        target: LOG_TARGET,
                        "the ACP agent did not confirm the cancel in time; ending the turn locally"
                    );
                    self.cancel_deadline = None;
                    self.turn_cancelled = false;
                    self.turn_active = false;
                    self.message = MessageBuffer::default();
                    self.status(SessionState::Idle).await;
                    return;
                }
            }
        }
    }

    /// The prompt response is the end of the turn. A cancelled turn keeps its deltas but never
    /// completes an assistant message, so cancelling cannot leave a half-written answer behind.
    async fn finish_turn(&mut self, response: &Value) {
        let cancelled = self.turn_cancelled;
        self.turn_cancelled = false;
        let stop_reason = response
            .get("result")
            .and_then(|result| result.get("stopReason"))
            .and_then(Value::as_str)
            .unwrap_or("end_turn")
            .to_string();

        // The turn that asked is over: anything still pending can only be answered `cancelled`.
        let pending: Vec<String> = self.pending_permissions.keys().cloned().collect();
        for request_id in pending {
            let _ = self
                .answer_permission(&request_id, Decision::Deny, true)
                .await;
        }

        if cancelled || stop_reason == "cancelled" {
            self.message = MessageBuffer::default();
            self.status(SessionState::Idle).await;
            return;
        }

        self.flush_message().await;
        if let Some(error) = response.get("error") {
            self.emit(EventKind::Error {
                message: format!("the ACP agent failed the turn: {}", describe_error(error)),
                retryable: false,
            })
            .await;
        } else {
            match stop_reason.as_str() {
                "end_turn" => {}
                "refusal" => {
                    self.emit(EventKind::Error {
                        message: "the ACP agent refused the request (stop reason `refusal`)".into(),
                        retryable: false,
                    })
                    .await;
                }
                other => {
                    self.emit(EventKind::Error {
                        message: format!(
                            "the ACP agent stopped the turn early (stop reason `{other}`)"
                        ),
                        retryable: true,
                    })
                    .await;
                }
            }
        }
        self.status(SessionState::Idle).await;
    }

    /// Route one frame that is not the response we are waiting for.
    async fn handle_frame(&mut self, frame: Value) {
        let Some(method) = frame
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            tracing::debug!(
                target: LOG_TARGET,
                "ignoring an ACP response nobody is waiting for: {}",
                truncate(&frame.to_string(), 200)
            );
            return;
        };
        match method.as_str() {
            "session/update" => self.handle_update(&frame).await,
            "session/request_permission" => self.handle_permission_request(&frame).await,
            other => {
                tracing::warn!(target: LOG_TARGET, "ignoring an unknown ACP method: {other}");
                if let Some(id) = frame.get("id").filter(|id| !id.is_null()).cloned() {
                    // A request, not a notification: answering keeps an agent from waiting forever.
                    let _ = self.write(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": format!("grove does not implement `{other}`"),
                        },
                    }));
                }
            }
        }
    }

    async fn handle_update(&mut self, frame: &Value) {
        let Some(update) = frame.get("params").and_then(|params| params.get("update")) else {
            tracing::debug!(target: LOG_TARGET, "ignoring a session/update without an update body");
            return;
        };
        let Some(variant) = update.get("sessionUpdate").and_then(Value::as_str) else {
            tracing::debug!(target: LOG_TARGET, "ignoring a session/update without a variant");
            return;
        };
        match variant {
            "agent_message_chunk" => self.handle_agent_chunk(update).await,
            "agent_thought_chunk" => {
                if self.turn_cancelled {
                    tracing::debug!(
                        target: LOG_TARGET,
                        "dropping an agent_thought_chunk after a cancel"
                    );
                    return;
                }
                match content_text(update.get("content")) {
                    Some(text) if !text.is_empty() => {
                        self.emit(EventKind::Message {
                            role: Role::Assistant,
                            blocks: vec![Block::Thinking { text }],
                        })
                        .await;
                    }
                    _ => tracing::debug!(
                        target: LOG_TARGET,
                        "ignoring a non-text agent_thought_chunk"
                    ),
                }
            }
            // `session/load` replays the conversation; those echoes are not new information.
            "user_message_chunk" => {
                tracing::debug!(target: LOG_TARGET, "ignoring a replayed user_message_chunk")
            }
            "tool_call" => self.handle_tool_call(update).await,
            "tool_call_update" => self.handle_tool_call_update(update).await,
            // A later ticket may feed a plan panel; the event log is Grove's own plan.
            "plan" => {
                tracing::debug!(target: LOG_TARGET, "ignoring a plan update")
            }
            "usage_update" => tracing::debug!(
                target: LOG_TARGET,
                "ignoring a usage_update: it reports context-window fill, not token counts"
            ),
            other => tracing::debug!(
                target: LOG_TARGET,
                "ignoring an ACP session/update variant: {other}"
            ),
        }
    }

    async fn handle_agent_chunk(&mut self, update: &Value) {
        // A cancelled turn still accepts stream fragments: the spec says the agent may keep sending
        // updates until it answers the prompt, and a delta is not a claim that the answer is done.
        // `finish_turn` discards the buffer, so no `Message` completes after a cancel.
        let Some(text) = content_text(update.get("content")) else {
            tracing::debug!(
                target: LOG_TARGET,
                "ignoring a non-text agent_message_chunk: {}",
                truncate(&update.to_string(), 200)
            );
            return;
        };
        if text.is_empty() {
            return;
        }
        let message_id = update
            .get("messageId")
            .and_then(Value::as_str)
            .map(str::to_string);
        if self.message.id != message_id {
            // A new message id (or the first chunk): the previous message is complete — unless the
            // turn was cancelled, which must never complete a message.
            if self.turn_cancelled {
                self.message = MessageBuffer::default();
            } else {
                self.flush_message().await;
                self.message = MessageBuffer {
                    id: message_id,
                    text: String::new(),
                };
            }
        }
        self.emit(EventKind::Delta { text: text.clone() }).await;
        self.message.text.push_str(&text);
    }

    async fn flush_message(&mut self) {
        let text = std::mem::take(&mut self.message.text);
        self.message.id = None;
        if text.is_empty() {
            return;
        }
        self.emit(EventKind::Message {
            role: Role::Assistant,
            blocks: vec![Block::Text { text }],
        })
        .await;
    }

    async fn handle_tool_call(&mut self, update: &Value) {
        let Some(call_id) = update.get("toolCallId").and_then(Value::as_str) else {
            tracing::warn!(target: LOG_TARGET, "ignoring a tool_call without a toolCallId");
            return;
        };
        let name = update
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| update.get("kind").and_then(Value::as_str))
            .unwrap_or("tool")
            .to_string();
        let title = update
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or(name.as_str())
            .to_string();
        self.emit(EventKind::ToolCall {
            call_id: call_id.to_string(),
            name,
            title,
        })
        .await;
    }

    async fn handle_tool_call_update(&mut self, update: &Value) {
        let Some(call_id) = update.get("toolCallId").and_then(Value::as_str) else {
            tracing::warn!(
                target: LOG_TARGET,
                "ignoring a tool_call_update without a toolCallId"
            );
            return;
        };
        let status = match update.get("status").and_then(Value::as_str) {
            Some("completed") => ToolStatus::Ok,
            Some("failed") => ToolStatus::Error,
            // Progress, not a result: the call itself is already in the transcript.
            Some("pending") | Some("in_progress") | None => {
                tracing::debug!(
                    target: LOG_TARGET,
                    "ignoring a non-terminal tool_call_update for {call_id}"
                );
                return;
            }
            Some(other) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    "ignoring an unknown tool call status: {other}"
                );
                return;
            }
        };
        let summary = summarize_content(update.get("content"));
        self.emit(EventKind::ToolResult {
            call_id: call_id.to_string(),
            status,
            summary,
        })
        .await;
    }

    async fn handle_permission_request(&mut self, frame: &Value) {
        let Some(id) = frame.get("id").filter(|id| !id.is_null()).cloned() else {
            tracing::warn!(
                target: LOG_TARGET,
                "a session/request_permission without an id cannot be answered; ignoring it"
            );
            return;
        };
        if !self.turn_active || self.turn_cancelled {
            // Always answer: an agent that asked is blocked until it hears back. Nothing is running
            // that the user could be approving, so the only truthful outcome is `cancelled`.
            tracing::warn!(
                target: LOG_TARGET,
                "answering a permission request from outside an active turn with `cancelled`"
            );
            let _ = self.write(permission_cancelled(&id));
            return;
        }
        let params = frame.get("params").cloned().unwrap_or(Value::Null);
        let title = params
            .get("toolCall")
            .and_then(|call| call.get("title"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or("a tool call")
            .to_string();
        let options = params
            .get("options")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let detail = describe_options(&options);
        let request_id = frame_id(&id).unwrap_or_else(|| id.to_string());
        self.pending_permissions
            .insert(request_id.clone(), PendingPermission { id, options });
        self.emit(EventKind::PermissionRequest {
            request_id,
            title,
            detail,
        })
        .await;
    }

    /// Answer one permission request. `cancelled` is always a valid answer, so this can never leave
    /// the agent waiting, even when no offered option matches the decision.
    async fn answer_permission(
        &mut self,
        request_id: &str,
        decision: Decision,
        cancelled: bool,
    ) -> Result<(), AdapterError> {
        let Some(pending) = self.pending_permissions.remove(request_id) else {
            return Err(AdapterError::Other(format!(
                "no ACP permission request `{request_id}` is pending"
            )));
        };
        let (frame, resolved) = if cancelled {
            (permission_cancelled(&pending.id), Decision::Deny)
        } else {
            match select_option(&pending.options, decision) {
                Some(option_id) => (
                    json!({
                        "jsonrpc": "2.0",
                        "id": pending.id,
                        "result": {
                            "outcome": { "outcome": "selected", "optionId": option_id },
                        },
                    }),
                    decision,
                ),
                None => {
                    // The agent offered nothing matching the user's choice. `cancelled` is the
                    // documented answer and the only one that cannot hang the turn.
                    tracing::warn!(
                        target: LOG_TARGET,
                        "no permission option matches {decision:?}; answering `cancelled`"
                    );
                    (permission_cancelled(&pending.id), Decision::Deny)
                }
            }
        };
        self.write(frame)
            .map_err(|failure| AdapterError::Other(failure.describe()))?;
        self.emit(EventKind::PermissionResolved {
            request_id: request_id.to_string(),
            decision: resolved,
        })
        .await;
        Ok(())
    }

    /// End a session that cannot work. The status is the promise: `error` for a protocol problem,
    /// `exited` when the agent simply is not there.
    async fn fail(&mut self, failure: Failure) {
        let (message, retryable, state) = match failure {
            Failure::VersionMismatch { agent } => (
                format!(
                    "the ACP agent speaks protocol version {agent}, but Grove requires version \
                     {PROTOCOL_VERSION}: point the ACP adapter at an agent that speaks version \
                     {PROTOCOL_VERSION}, or update Grove"
                ),
                false,
                SessionState::Error,
            ),
            Failure::Refused(reason) => (reason, false, SessionState::Error),
            Failure::TransportClosed => (
                "the ACP agent exited before the session was ready".to_string(),
                true,
                SessionState::Exited,
            ),
            Failure::Shutdown => return,
        };
        self.emit(EventKind::Error { message, retryable }).await;
        self.status(state).await;
    }

    async fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Some(pid) = self.pid {
                kill_tree_by_pid(pid).await;
            }
            let _ = child.kill().await;
            let _ = tokio::time::timeout(REAP_TIMEOUT, child.wait()).await;
        }
    }
}

/// Drive the actor for the life of the session. Dropping the event sender ends the session in the
/// manager's pump.
async fn run(
    mut actor: Actor,
    mut commands: mpsc::UnboundedReceiver<SessionCommand>,
    mut frames: mpsc::Receiver<Value>,
) {
    actor
        .emit(EventKind::SessionStarted {
            harness: HARNESS.to_string(),
            model: actor.model.clone(),
            resumed: actor.session_id.is_some(),
        })
        .await;
    actor.status(SessionState::Starting).await;

    match actor.handshake(&mut commands, &mut frames).await {
        Ok(()) => actor.status(SessionState::Ready).await,
        Err(failure) => {
            actor.fail(failure).await;
            actor.kill().await;
            return;
        }
    }

    loop {
        while let Some(prompt) = actor.pending_prompts.pop_front() {
            actor.turn(prompt, &mut commands, &mut frames).await;
            if actor.shutdown {
                return;
            }
        }
        match commands.recv().await {
            Some(SessionCommand::Send(prompt)) => actor.pending_prompts.push_back(prompt),
            // Cancel with nothing running is a no-op that still confirms the idle state.
            Some(SessionCommand::Cancel) => actor.status(SessionState::Idle).await,
            Some(SessionCommand::Permission {
                request_id,
                decision: _,
                ack,
            }) => {
                let _ = ack.send(Err(AdapterError::Other(format!(
                    "no ACP permission request `{request_id}` is pending"
                ))));
            }
            Some(SessionCommand::Shutdown) | None => {
                actor.shutdown = true;
                return;
            }
        }
    }
}

/// Read one JSON-RPC frame per line. Malformed lines are a log entry, not a dead session.
async fn read_frames(stdout: ChildStdout, frames: mpsc::Sender<Value>) {
    let mut lines = BufReader::new(stdout).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(line) {
                    Ok(frame) => {
                        if frames.send(frame).await.is_err() {
                            return;
                        }
                    }
                    Err(error) => tracing::debug!(
                        target: LOG_TARGET,
                        "skipping a malformed ACP line ({error}): {}",
                        truncate(line, 200)
                    ),
                }
            }
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(target: LOG_TARGET, "reading ACP output failed: {error}");
                return;
            }
        }
    }
}

/// Write one newline-delimited JSON-RPC frame per message. The only writer on the agent's stdin.
async fn write_frames(mut stdin: ChildStdin, mut frames: mpsc::UnboundedReceiver<String>) {
    while let Some(frame) = frames.recv().await {
        if stdin.write_all(frame.as_bytes()).await.is_err()
            || stdin.write_all(b"\n").await.is_err()
            || stdin.flush().await.is_err()
        {
            break;
        }
    }
    let _ = stdin.shutdown().await;
}

async fn log_stderr(stderr: tokio::process::ChildStderr) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::debug!(target: LOG_TARGET, "agent stderr: {line}");
    }
}

/// The response id, and only for a response: a request from the agent has an id too, and echoing it
/// back as "the answer we were waiting for" would be a protocol bug.
fn response_id(frame: &Value) -> Option<u64> {
    if frame.get("method").is_some() {
        return None;
    }
    frame.get("id").and_then(Value::as_u64)
}

/// The id as the string a `PermissionRequest` carries (agents may use numbers or strings).
fn frame_id(id: &Value) -> Option<String> {
    match id {
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

fn permission_cancelled(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "outcome": { "outcome": "cancelled" } },
    })
}

/// Pick the option that matches the user's decision. `None` means "answer `cancelled`".
fn select_option(options: &[Value], decision: Decision) -> Option<String> {
    let preferred: &[&str] = match decision {
        Decision::Approve => &["allow_once", "allow_always"],
        Decision::Deny => &["reject_once", "reject_always"],
    };
    for kind in preferred {
        let option = options
            .iter()
            .find(|option| option.get("kind").and_then(Value::as_str) == Some(kind));
        if let Some(option_id) = option
            .and_then(|option| option.get("optionId"))
            .and_then(Value::as_str)
        {
            return Some(option_id.to_string());
        }
    }
    None
}

fn describe_options(options: &[Value]) -> String {
    if options.is_empty() {
        return "the agent offered no options".to_string();
    }
    options
        .iter()
        .filter_map(|option| {
            let name = option.get("name").and_then(Value::as_str)?;
            let kind = option
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("other");
            Some(format!("{name} ({kind})"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn content_text(content: Option<&Value>) -> Option<String> {
    let content = content?;
    if content.get("type").and_then(Value::as_str) != Some("text") {
        return None;
    }
    content
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Tool content is a list of text blocks, diffs and terminals; the transcript wants one summary.
fn summarize_content(content: Option<&Value>) -> String {
    let Some(items) = content.and_then(Value::as_array) else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("content") => {
                if let Some(inner) = item.get("content") {
                    match inner.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(text) = inner.get("text").and_then(Value::as_str) {
                                parts.push(text.to_string());
                            }
                        }
                        Some("diff") => {
                            if let Some(path) = inner.get("path").and_then(Value::as_str) {
                                parts.push(format!("[diff] {path}"));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("diff") => {
                if let Some(path) = item.get("path").and_then(Value::as_str) {
                    parts.push(format!("[diff] {path}"));
                }
            }
            Some("terminal") => {
                if let Some(id) = item.get("terminalId").and_then(Value::as_str) {
                    parts.push(format!("[terminal] {id}"));
                }
            }
            _ => {}
        }
    }
    truncate(&parts.join("\n"), SUMMARY_LIMIT)
}

fn describe_error(error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("an error with no message");
    match error.get("code").and_then(Value::as_i64) {
        Some(code) => format!("{message} (code {code})"),
        None => message.to_string(),
    }
}

/// An agent advertises `session/resume` in its `initialize` result (nested under `agentCapabilities`
/// in the v1 schema; accepted at the top level too, since the wording of the spec is ambiguous).
fn supports_resume(initialize_result: &Value) -> bool {
    fn advertised(container: Option<&Value>) -> bool {
        match container.and_then(|container| container.get("resume")) {
            Some(Value::Bool(advertised)) => *advertised,
            // An empty object (`{"resume": {}}`) means "supported with no options".
            Some(Value::Null) | None => false,
            Some(_) => true,
        }
    }
    let capabilities = initialize_result.get("agentCapabilities");
    advertised(capabilities.and_then(|value| value.get("sessionCapabilities")))
        || advertised(initialize_result.get("sessionCapabilities"))
}

/// Prepended context documents (pinned specs, the plan) become the head of the first prompt.
fn render_prompt(prompt: Option<&str>, documents: &[ContextDoc]) -> Option<String> {
    if documents.is_empty() {
        return prompt.map(str::to_string);
    }
    let mut rendered = String::from("# Context\n\n");
    for document in documents {
        rendered.push_str(&format!(
            "## {}\n\n<!-- id: {} -->\n\n",
            document.title, document.id
        ));
        rendered.push_str(document.body.trim_end());
        rendered.push_str("\n\n");
    }
    if let Some(prompt) = prompt.map(str::trim).filter(|prompt| !prompt.is_empty()) {
        rendered.push_str("---\n\n");
        rendered.push_str(prompt);
        rendered.push('\n');
    }
    Some(rendered)
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(limit).collect();
    truncated.push('…');
    truncated
}

#[cfg(windows)]
async fn kill_tree_by_pid(pid: u32) {
    // ACP agents spawn tools of their own; killing the direct child would leave them running.
    let pid = pid.to_string();
    let _ = Command::new("taskkill")
        .args(["/PID", pid.as_str(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

#[cfg(not(windows))]
async fn kill_tree_by_pid(pid: u32) {
    // The child is spawned as a process-group leader (see `AcpAdapter::start`).
    let group = format!("-{pid}");
    let _ = Command::new("kill")
        .args(["-KILL", group.as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

/// Resolve a binary the way a shell would: an explicit path as given, otherwise `PATH` + `PATHEXT`.
fn find_on_path(binary: &str) -> Option<PathBuf> {
    if binary.contains('/') || binary.contains('\\') || Path::new(binary).is_absolute() {
        let path = PathBuf::from(binary);
        return path.is_file().then_some(path);
    }
    let search_path = std::env::var_os("PATH")?;
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .ok()
            .map(|raw| {
                raw.split(';')
                    .map(str::trim)
                    .filter(|extension| !extension.is_empty())
                    .map(str::to_ascii_lowercase)
                    .collect()
            })
            .filter(|extensions: &Vec<String>| !extensions.is_empty())
            .unwrap_or_else(|| vec![".exe".into(), ".cmd".into(), ".bat".into(), ".com".into()])
    } else {
        vec![String::new()]
    };
    for directory in std::env::split_paths(&search_path) {
        for extension in &extensions {
            let candidate = directory.join(format!("{binary}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
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
    fn the_command_line_is_configuration_and_a_missing_one_is_unavailable() {
        let launch = Launch::from_config(&serde_json::json!({
            "command": " node ",
            "args": ["agent.mjs", "--flag", 42],
            "mcpServers": [{ "name": "fs", "command": "npx", "args": [], "env": [] }],
        }))
        .expect("a configured agent");
        assert_eq!(launch.command, "node");
        assert_eq!(
            launch.args,
            strings(&["agent.mjs", "--flag"]),
            "non-string args are ignored rather than fatal"
        );
        assert_eq!(launch.mcp_servers.len(), 1);

        for config in [
            serde_json::json!({}),
            serde_json::json!({ "command": "   " }),
            serde_json::json!({ "args": ["agent.mjs"] }),
        ] {
            let error = Launch::from_config(&config).expect_err("no command, no session");
            assert!(matches!(error, AdapterError::Unavailable(_)), "{error:?}");
        }

        let launch = Launch::from_config(&serde_json::json!({ "command": "node" })).unwrap();
        assert!(launch.args.is_empty());
        assert!(launch.mcp_servers.is_empty());
    }

    #[test]
    fn capabilities_are_honest() {
        let capabilities = AcpAdapter::default().capabilities();
        assert!(capabilities.streaming_deltas);
        assert!(capabilities.tool_calls);
        assert!(
            capabilities.permissions,
            "every request is answered, including cancelled"
        );
        assert!(capabilities.resume, "session/resume keeps the same id");
        assert!(!capabilities.diffs, "no patch is synthesized yet");
        assert!(
            !capabilities.usage,
            "usage_update is context-window fill, not a token pair"
        );
        assert!(!capabilities.images);
        assert!(!capabilities.mcp, "Grove's MCP catalog is not wired in");
    }

    #[test]
    fn permission_answers_prefer_the_documented_option_kind() {
        let options = vec![
            serde_json::json!({ "optionId": "allow-always", "name": "Always", "kind": "allow_always" }),
            serde_json::json!({ "optionId": "allow-once", "name": "Once", "kind": "allow_once" }),
            serde_json::json!({ "optionId": "reject-once", "name": "No", "kind": "reject_once" }),
        ];
        assert_eq!(
            select_option(&options, Decision::Approve).as_deref(),
            Some("allow-once")
        );
        assert_eq!(
            select_option(&options, Decision::Deny).as_deref(),
            Some("reject-once")
        );

        let reject_only =
            vec![serde_json::json!({ "optionId": "no", "name": "No", "kind": "reject_once" })];
        assert_eq!(
            select_option(&reject_only, Decision::Approve),
            None,
            "an approval with only rejections offered answers `cancelled` instead of guessing"
        );
        assert_eq!(select_option(&[], Decision::Deny), None);
    }

    #[test]
    fn permission_requests_become_the_ids_a_user_can_answer() {
        assert_eq!(
            frame_id(&serde_json::json!(5)).as_deref(),
            Some("5"),
            "numeric ids are stable strings"
        );
        assert_eq!(
            frame_id(&serde_json::json!("perm-a")).as_deref(),
            Some("perm-a")
        );
        assert_eq!(frame_id(&Value::Null), None);
        assert_eq!(describe_options(&[]), "the agent offered no options");
        assert_eq!(
            describe_options(&[
                serde_json::json!({ "name": "Allow once", "kind": "allow_once" }),
                serde_json::json!({ "name": "Reject", "kind": "reject_once" }),
            ]),
            "Allow once (allow_once), Reject (reject_once)"
        );
    }

    #[test]
    fn a_resumed_session_is_only_attempted_when_the_agent_advertises_it() {
        assert!(supports_resume(&serde_json::json!({
            "agentCapabilities": { "sessionCapabilities": { "resume": {} } },
        })));
        assert!(supports_resume(&serde_json::json!({
            "sessionCapabilities": { "resume": {} },
        })));
        assert!(!supports_resume(&serde_json::json!({
            "agentCapabilities": { "sessionCapabilities": { "resume": false } },
        })));
        assert!(!supports_resume(&serde_json::json!({
            "agentCapabilities": { "loadSession": true },
        })));
        assert!(!supports_resume(
            &serde_json::json!({ "agentCapabilities": {} })
        ));
    }

    #[test]
    fn only_responses_match_a_pending_request_id() {
        assert_eq!(
            response_id(&serde_json::json!({ "jsonrpc": "2.0", "id": 4, "result": {} })),
            Some(4)
        );
        assert_eq!(
            response_id(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "session/request_permission",
                "params": {},
            })),
            None,
            "a request that happens to reuse our id is not the answer to it"
        );
        assert_eq!(
            response_id(&serde_json::json!({ "jsonrpc": "2.0", "method": "session/update" })),
            None
        );
    }

    #[test]
    fn tool_content_is_summarised_and_truncated() {
        assert_eq!(summarize_content(None), "");
        assert_eq!(
            summarize_content(Some(&serde_json::json!([
                { "type": "content", "content": { "type": "text", "text": "config loaded" } },
                { "type": "diff", "path": "src/main.rs" },
                { "type": "terminal", "terminalId": "term-1" },
            ]))),
            "config loaded\n[diff] src/main.rs\n[terminal] term-1"
        );
        let long = "x".repeat(SUMMARY_LIMIT + 5);
        let summary = summarize_content(Some(&serde_json::json!([
            { "type": "content", "content": { "type": "text", "text": long } },
        ])));
        assert_eq!(summary.chars().count(), SUMMARY_LIMIT + 1);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn context_documents_are_prepended_to_the_first_prompt() {
        let documents = vec![ContextDoc {
            id: "plan".into(),
            title: "Plan".into(),
            body: "- step one\n".into(),
        }];
        let rendered = render_prompt(Some("ship ticket 11"), &documents).expect("a prompt");
        assert!(rendered.starts_with("# Context\n\n"));
        assert!(rendered.contains("## Plan"));
        assert!(rendered.contains("<!-- id: plan -->"));
        assert!(rendered.contains("---\n\nship ticket 11"));
        assert_eq!(render_prompt(Some("plain"), &[]).as_deref(), Some("plain"));
        assert_eq!(render_prompt(None, &[]), None);
    }
}
