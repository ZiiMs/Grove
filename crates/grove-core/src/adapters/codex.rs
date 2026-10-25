//! The Codex adapter: `codex app-server`, JSON-RPC 2.0 over newline-delimited stdio.
//!
//! One long-lived app-server process per session, with the mandatory handshake once per connection:
//! `initialize` → the `initialized` notification → `thread/start` (or `thread/resume` with the stored
//! thread id), then one `turn/start` per user message. Notifications are mapped onto the normalized
//! event model; everything else is logged and dropped, so protocol drift is a log line rather than a
//! dead session.
//!
//! | app-server frame | Grove events |
//! |---|---|
//! | `initialize` result + `thread/started` | `SessionStarted { harness: "codex", resumed }`, `Status { starting }`, `Status { ready }` |
//! | `turn/start` result / `turn/started` | `Status { busy }`, turn id remembered for `turn/interrupt` |
//! | `item/agentMessage/delta` | `Delta { text }` |
//! | `item/completed` item `agentMessage` | `Message { assistant, [Text] }` (the authoritative text) |
//! | `item/started` item `commandExecution` | `ToolCall { call_id: item.id, name: "exec", title: command }` |
//! | `item/commandExecution/outputDelta` | buffered into the tool result's summary |
//! | `item/completed` item `commandExecution` | `ToolResult { status from exitCode, summary: aggregated output + exit code }` |
//! | `turn/diff/updated` | `Diff` (one per distinct aggregated patch, path and counts read from it) |
//! | `thread/tokenUsage/updated` | `Usage { input_tokens, output_tokens }` |
//! | `turn/completed` | `Status { idle }`; `failed` adds a non-retryable `Error` |
//! | approval server requests (`…/requestApproval`, `execCommandApproval`) | `PermissionRequest`; the answer is written back and mirrored as `PermissionResolved` |
//! | `error` | `Error { retryable: false }` |
//! | unknown method, unknown item type, malformed line | logged and skipped |
//! | process exits before `turn/completed` | `Error { retryable: true }` + `Status { exited }` |
//!
//! Frames are matched to the turn in flight by `turnId`. A cancel resolves the turn as soon as the
//! server acknowledges `turn/interrupt`, while the interrupted turn's items keep arriving: those late
//! frames are dropped instead of being attributed to the turn that replaced it.
//!
//! Deliberate limits, reflected in [`CodexAdapter::capabilities`]:
//!
//! - **No MCP mapping.** `mcpToolCall` items are logged, not mapped, so `mcp` is `false`.
//! - **No images.** Attachments are not part of the launch shape Grove uses.
//!
//! The approval reply payload is the one place the current protocol is not confirmed: older builds
//! document `{"decision": "accept" | "acceptForSession" | "decline" | "cancel"}`. Grove sends the
//! minimal `accept`/`decline` vocabulary and logs the request it answered, so a changed reply shape is
//! a log line rather than a silent hang.
//!
//! Command override: `config.command` and `config.args` replace/extend the binary and `app-server` is
//! appended, which is how a user points Grove at a wrapper and how every test points it at
//! `fixtures/harnesses/codex_fake.mjs`.

use super::support::{
    command_from_config, find_on_path, kill_process_tree, render_prompt, truncate,
};
use super::{
    AdapterDeps, AdapterError, Capabilities, Detection, HarnessAdapter, HarnessId, HarnessSession,
    ResumeToken, StartContext, UserInput,
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
use tokio::sync::mpsc;

const DEFAULT_COMMAND: &str = "codex";
/// The harness's own name, as written into `SessionStarted` and stored on the session row.
const HARNESS: &str = "codex";
/// A command execution can print megabytes; the transcript wants a summary, not a second copy.
const SUMMARY_LIMIT: usize = 500;
/// How much of an approval request's params the UI shows.
const DETAIL_LIMIT: usize = 600;
/// Closing stdin is the documented way to end an app-server session; this is how long it gets.
const GRACEFUL_EXIT: Duration = Duration::from_millis(300);
/// A handshake that never answers is a failed launch, not a hung session.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
const EVENT_BUFFER: usize = 1024;

/// Field spellings a `TokenUsageBreakdown` has carried; the schema is generated per Codex release.
const INPUT_TOKEN_KEYS: &[&str] = &[
    "input_tokens",
    "inputTokens",
    "input",
    "cached_input_tokens",
    "cachedInputTokens",
];
const OUTPUT_TOKEN_KEYS: &[&str] = &["output_tokens", "outputTokens", "output"];

pub struct CodexAdapter {
    /// The binary detection looks for. `config.command` can still override it per session.
    command: String,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self {
            command: DEFAULT_COMMAND.to_string(),
        }
    }
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A different binary name for detection (a wrapper, a versioned install, a test double).
    pub fn with_command(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }
}

#[async_trait::async_trait]
impl HarnessAdapter for CodexAdapter {
    fn id(&self) -> HarnessId {
        HarnessId(HARNESS)
    }

    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn skills_dir(&self) -> Option<&'static str> {
        Some(".codex/skills")
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // `item/agentMessage/delta` streams the answer token by token.
            streaming_deltas: true,
            // `commandExecution` items carry the command, its aggregated output and its exit code.
            tool_calls: true,
            // `turn/diff/updated` carries one aggregated unified diff per turn.
            diffs: true,
            // Approvals are server -> client requests, and the adapter answers them.
            permissions: true,
            // The thread id is the resume token, reused with `thread/resume`.
            resume: true,
            images: false,
            // `mcpToolCall` items are logged, not mapped: do not claim what is not emitted.
            mcp: false,
            usage: true,
        }
    }

    /// Side-effect free: it resolves the binary on `PATH` but never runs it. A missing binary is
    /// reported here and does not stop [`HarnessAdapter::start`]; the failure surfaces as an error
    /// event on the session so the row and its transcript exist either way.
    fn detect(&self, _deps: &AdapterDeps) -> Detection {
        match find_on_path(&self.command) {
            Some(path) => Detection {
                installed: true,
                // `--version` belongs to the doctor, which is allowed to run probes.
                version: None,
                path: Some(crate::paths::display_path(&path)),
                detail: None,
            },
            None => Detection::missing(format!(
                "`{}` is not on PATH; install the Codex CLI or set config.command",
                self.command
            )),
        }
    }

    async fn start(
        &self,
        ctx: StartContext,
        _deps: AdapterDeps,
    ) -> Result<Box<dyn HarnessSession>, AdapterError> {
        let (command, extra_args) = command_from_config(&ctx.config, &self.command);
        // Context documents are prepended to the first turn's text: app-server takes the prompt in
        // `turn/start`, not on argv.
        let initial_prompt = render_prompt(ctx.prompt.as_deref(), &ctx.context_files);

        let (events, events_rx) = mpsc::channel(EVENT_BUFFER);
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let token = Arc::new(Mutex::new(ctx.resume_token.clone()));
        let permissions = Arc::new(Mutex::new(HashMap::new()));
        let runner = Runner {
            events,
            token: Arc::clone(&token),
            permissions: Arc::clone(&permissions),
            cwd: ctx.cwd,
            command,
            extra_args,
            model: ctx.model,
            resume: ctx.resume_token.clone(),
            child: None,
            pid: None,
            input: None,
            reader: None,
            responses: HashMap::new(),
            requests: HashMap::new(),
            next_id: 0,
            thread_id: None,
            turn: None,
            last_turn_id: None,
            exec_output: HashMap::new(),
            prompts: VecDeque::new(),
        };
        tokio::spawn(run(runner, initial_prompt, commands_rx));

        Ok(Box::new(CodexSession {
            events: Some(events_rx),
            commands,
            token,
            permissions,
        }))
    }
}

enum TurnCommand {
    Send(String),
    Cancel,
    RespondPermission {
        request_id: String,
        decision: Decision,
    },
    Shutdown,
}

pub struct CodexSession {
    events: Option<mpsc::Receiver<HarnessEvent>>,
    commands: mpsc::UnboundedSender<TurnCommand>,
    token: ResumeToken,
    /// Approval ids the runner has emitted and not yet answered, shared so `respond_permission` can
    /// reject an id the harness never sent instead of pretending to resolve it.
    permissions: Arc<Mutex<HashMap<String, Value>>>,
}

impl CodexSession {
    fn send_command(&self, command: TurnCommand) -> Result<(), AdapterError> {
        self.commands
            .send(command)
            .map_err(|_| AdapterError::Other("the codex session has ended".into()))
    }
}

#[async_trait::async_trait]
impl HarnessSession for CodexSession {
    fn take_events(&mut self) -> mpsc::Receiver<HarnessEvent> {
        self.events
            .take()
            .expect("take_events is called exactly once by the session manager")
    }

    async fn send(&mut self, input: UserInput) -> Result<(), AdapterError> {
        self.send_command(TurnCommand::Send(input.text))
    }

    async fn respond_permission(
        &mut self,
        request_id: &str,
        decision: Decision,
    ) -> Result<(), AdapterError> {
        let known = self
            .permissions
            .lock()
            .map(|pending| pending.contains_key(request_id))
            .unwrap_or(false);
        if !known {
            return Err(AdapterError::Other(format!(
                "codex has no approval request {request_id} waiting for an answer"
            )));
        }
        self.send_command(TurnCommand::RespondPermission {
            request_id: request_id.to_string(),
            decision,
        })
    }

    async fn cancel(&mut self) -> Result<(), AdapterError> {
        self.send_command(TurnCommand::Cancel)
    }

    async fn shutdown(&mut self) -> Result<(), AdapterError> {
        // The manager owns the final `exited` transition, exactly like the other adapters.
        let _ = self.commands.send(TurnCommand::Shutdown);
        Ok(())
    }

    fn resume_token(&self) -> Option<String> {
        self.token.lock().ok().and_then(|token| token.clone())
    }

    fn resume_token_slot(&self) -> Option<ResumeToken> {
        Some(Arc::clone(&self.token))
    }
}

/// What a pending client request was for. Responses are matched on the JSON-RPC id, so a message the
/// server sends that does not answer one of ours is only ever a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Initialize,
    Thread,
    TurnStart,
    Interrupt,
}

/// One client turn in flight: its id (needed to interrupt it), the last aggregated diff already
/// rendered, and whether an error was already reported so a failure is never doubled.
#[derive(Debug, Default)]
struct Turn {
    id: Option<String>,
    cancel_requested: bool,
    interrupt_sent: bool,
    error_reported: bool,
    last_diff: Option<String>,
}

/// One decoded line from the app-server.
#[derive(Debug)]
enum Message {
    Notification {
        method: String,
        params: Value,
    },
    /// A server -> client *request*: it carries an id and expects a result (approvals live here).
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Response {
        id: Value,
        result: Result<Value, Value>,
    },
}

/// The actor: it owns the process and is the only writer on its stdin.
struct Runner {
    events: mpsc::Sender<HarnessEvent>,
    token: ResumeToken,
    permissions: Arc<Mutex<HashMap<String, Value>>>,
    cwd: PathBuf,
    command: String,
    extra_args: Vec<String>,
    model: Option<String>,
    /// The stored thread id, if this session is a restart.
    resume: Option<String>,
    child: Option<Child>,
    pid: Option<u32>,
    input: Option<ChildStdin>,
    reader: Option<tokio::task::JoinHandle<()>>,
    /// Handshake answers, keyed by request id, drained by [`Runner::wait_response`].
    responses: HashMap<u64, Result<Value, Value>>,
    requests: HashMap<u64, Pending>,
    next_id: u64,
    thread_id: Option<String>,
    turn: Option<Turn>,
    /// The id of the turn that ended last: its frames keep arriving after a cancel, and they must be
    /// recognisable before the next turn's id is known.
    last_turn_id: Option<String>,
    /// `item/commandExecution/outputDelta` text per item, used when `aggregatedOutput` is absent.
    exec_output: HashMap<String, String>,
    /// Prompts that arrived while a turn was running. They run in order, one turn at a time: app-server
    /// serves one thread and two overlapping turns would interleave.
    prompts: VecDeque<String>,
}

impl Runner {
    async fn emit(&self, kind: EventKind) {
        // A closed channel means the manager dropped the session; the actor stops on its own.
        let _ = self.events.send(HarnessEvent::new(kind)).await;
    }

    async fn status(&self, state: SessionState) {
        self.emit(EventKind::Status { state }).await;
    }

    /// Report a fatal condition and end the session's status honestly.
    async fn fail(&mut self, message: impl Into<String>, retryable: bool) {
        self.emit(EventKind::Error {
            message: message.into(),
            retryable,
        })
        .await;
        self.status(SessionState::Exited).await;
    }

    fn set_thread(&mut self, thread_id: &str) {
        self.thread_id = Some(thread_id.to_string());
        if let Ok(mut token) = self.token.lock() {
            *token = Some(thread_id.to_string());
        }
    }

    fn note_turn_id(&mut self, turn_id: &str) {
        if let Some(turn) = self.turn.as_mut() {
            if turn.id.is_none() {
                turn.id = Some(turn_id.to_string());
            }
        }
    }

    /// Take the turn id from a notification's `params.turnId` when the `turn/start` answer has not
    /// arrived yet, so an early cancel still has something to interrupt.
    fn note_turn_from(&mut self, params: &Value) {
        if let Some(turn_id) = params.get("turnId").and_then(Value::as_str) {
            self.note_turn_id(turn_id);
        }
    }

    /// A turn-scoped frame belongs to the turn in flight only when the ids agree. Frames arrive late:
    /// a cancel resolves the turn at the interrupt acknowledgement while the harness is still writing
    /// the interrupted turn's items, and the next turn may already be running.
    fn matches_current_turn(&self, turn_id: Option<&str>) -> bool {
        let Some(turn) = self.turn.as_ref() else {
            return false;
        };
        match (turn_id, turn.id.as_deref()) {
            (Some(incoming), Some(current)) => incoming == current,
            // The turn in flight has no id yet: accept the frame unless it belongs to the turn that
            // just ended, whose late frames are exactly what arrive at this point.
            (Some(incoming), None) => self.last_turn_id.as_deref() != Some(incoming),
            (None, _) => true,
        }
    }

    async fn write_line(&mut self, value: &Value) -> bool {
        let Some(input) = self.input.as_mut() else {
            return false;
        };
        let mut line = match serde_json::to_vec(value) {
            Ok(line) => line,
            Err(error) => {
                tracing::warn!(target: "grove::adapters::codex", "could not encode a request: {error}");
                return false;
            }
        };
        line.push(b'\n');
        if let Err(error) = input.write_all(&line).await {
            tracing::warn!(target: "grove::adapters::codex", "writing to app-server failed: {error}");
            return false;
        }
        if let Err(error) = input.flush().await {
            tracing::warn!(target: "grove::adapters::codex", "flushing to app-server failed: {error}");
            return false;
        }
        true
    }

    async fn send_request(&mut self, method: &str, params: Value, kind: Pending) -> Option<u64> {
        let id = self.next_id;
        self.next_id += 1;
        self.requests.insert(id, kind);
        let request = json!({ "id": id, "method": method, "params": params });
        if !self.write_line(&request).await {
            self.requests.remove(&id);
            return None;
        }
        Some(id)
    }

    async fn send_result(&mut self, id: Value, result: Value) -> bool {
        self.write_line(&json!({ "id": id, "result": result }))
            .await
    }

    async fn send_error(&mut self, id: Value, code: i64, message: &str) -> bool {
        self.write_line(&json!({
            "id": id,
            "error": { "code": code, "message": message },
        }))
        .await
    }

    /// Spawn `codex app-server` and start its stdout reader. The receiver is returned rather than
    /// stored so the run loop can `select!` on it without borrowing the whole actor.
    async fn spawn_process(&mut self) -> Option<mpsc::Receiver<Message>> {
        let mut command = Command::new(&self.command);
        command
            .args(launch_args(&self.extra_args))
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // A dropped session must not leak a harness process.
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.fail(
                    format!("could not launch `{} app-server`: {error}", self.command),
                    false,
                )
                .await;
                return None;
            }
        };
        self.pid = child.id();
        self.input = child.stdin.take();

        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "grove::adapters::codex", "stderr: {line}");
                }
            });
        }

        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill().await;
            self.fail("codex app-server has no stdout pipe", false)
                .await;
            return None;
        };
        self.child = Some(child);
        let (messages, rx) = spawn_reader(stdout);
        self.reader = Some(messages);
        Some(rx)
    }

    /// Close stdin first (the documented way to end a session), then kill if it lingers.
    async fn close_process(&mut self) {
        self.input = None;
        if let Some(mut child) = self.child.take() {
            match tokio::time::timeout(GRACEFUL_EXIT, child.wait()).await {
                Ok(_) => {}
                Err(_) => kill_process_tree(self.pid, &mut child).await,
            }
        }
        if let Some(reader) = self.reader.take() {
            reader.abort();
        }
        self.requests.clear();
        if let Ok(mut pending) = self.permissions.lock() {
            pending.clear();
        }
    }

    /// `initialize` → `initialized` → `thread/start` (or `thread/resume`). Every step is a hard
    /// requirement of the protocol, so a failure is reported as an error event and the session ends
    /// instead of hanging.
    async fn handshake(&mut self, lines: &mut mpsc::Receiver<Message>) -> bool {
        let Some(id) = self
            .send_request("initialize", initialize_params(), Pending::Initialize)
            .await
        else {
            self.fail(
                "codex app-server closed its input before `initialize`",
                true,
            )
            .await;
            return false;
        };
        match self.wait_response(lines, id).await {
            Some(Ok(_)) => {}
            Some(Err(error)) => {
                self.fail(
                    format!(
                        "codex app-server refused `initialize`: {}",
                        error_message(&error)
                    ),
                    false,
                )
                .await;
                return false;
            }
            None => {
                self.fail(
                    "codex app-server did not answer `initialize` in time; is this really \
                     `codex app-server`?",
                    true,
                )
                .await;
                return false;
            }
        }

        // The handshake is `initialize` then this notification, before any other method: the server
        // answers `Not initialized` otherwise.
        if !self.write_line(&json!({ "method": "initialized" })).await {
            self.fail(
                "codex app-server closed its input before `initialized`",
                true,
            )
            .await;
            return false;
        }

        if let Some(thread_id) = self.resume.clone() {
            let params = json!({ "threadId": thread_id });
            let resumed = match self
                .send_request("thread/resume", params, Pending::Thread)
                .await
            {
                Some(id) => match self.wait_response(lines, id).await {
                    Some(Ok(_)) => self.thread_id.is_some(),
                    other => {
                        tracing::warn!(
                            target: "grove::adapters::codex",
                            "thread/resume for {} failed ({}); starting a new thread",
                            thread_id,
                            match other {
                                Some(Err(error)) => error_message(&error),
                                _ => "no answer".to_string(),
                            }
                        );
                        false
                    }
                },
                None => false,
            };
            if resumed {
                return true;
            }
        }

        let params = thread_start_params(&self.cwd, self.model.as_deref());
        let Some(id) = self
            .send_request("thread/start", params, Pending::Thread)
            .await
        else {
            self.fail(
                "codex app-server closed its input before `thread/start`",
                true,
            )
            .await;
            return false;
        };
        match self.wait_response(lines, id).await {
            Some(Ok(_)) => {
                if self.thread_id.is_some() {
                    true
                } else {
                    self.fail("codex app-server started a thread without an id", false)
                        .await;
                    false
                }
            }
            Some(Err(error)) => {
                self.fail(
                    format!(
                        "codex app-server refused `thread/start`: {}",
                        error_message(&error)
                    ),
                    false,
                )
                .await;
                false
            }
            None => {
                self.fail(
                    "codex app-server did not answer `thread/start` in time",
                    true,
                )
                .await;
                false
            }
        }
    }

    /// Wait for the answer to `id`, dispatching whatever else arrives meanwhile.
    async fn wait_response(
        &mut self,
        lines: &mut mpsc::Receiver<Message>,
        id: u64,
    ) -> Option<Result<Value, Value>> {
        let wait = async {
            loop {
                if let Some(response) = self.responses.remove(&id) {
                    return Some(response);
                }
                match lines.recv().await {
                    Some(message) => self.dispatch(message).await,
                    None => return self.responses.remove(&id),
                }
            }
        };
        tokio::time::timeout(HANDSHAKE_TIMEOUT, wait)
            .await
            .unwrap_or(None)
    }

    async fn dispatch(&mut self, message: Message) {
        match message {
            Message::Notification { method, params } => {
                self.on_notification(&method, &params).await
            }
            Message::Request { id, method, params } => self.on_request(id, &method, &params).await,
            Message::Response { id, result } => self.on_response(id, result).await,
        }
    }

    async fn on_response(&mut self, id: Value, result: Result<Value, Value>) {
        let Some(numeric) = id.as_u64() else {
            tracing::debug!(
                target: "grove::adapters::codex",
                "skipping a response with a non-numeric id: {id}"
            );
            return;
        };
        let Some(kind) = self.requests.remove(&numeric) else {
            tracing::debug!(
                target: "grove::adapters::codex",
                "skipping a response for an unknown request id: {numeric}"
            );
            return;
        };
        if matches!(kind, Pending::Initialize | Pending::Thread) {
            self.responses.insert(numeric, result.clone());
        }
        match kind {
            Pending::Initialize | Pending::Thread => {
                if let Ok(value) = &result {
                    if let Some(thread_id) = value.pointer("/thread/id").and_then(Value::as_str) {
                        self.set_thread(thread_id);
                    }
                }
            }
            Pending::TurnStart => match &result {
                Ok(value) => {
                    if let Some(turn_id) = value.pointer("/turn/id").and_then(Value::as_str) {
                        self.note_turn_id(turn_id);
                    }
                    self.interrupt_if_requested().await;
                }
                Err(error) => {
                    // A structured turn error is reported and the session stays alive.
                    self.turn = None;
                    self.emit(EventKind::Error {
                        message: format!(
                            "codex could not start the turn: {}",
                            error_message(error)
                        ),
                        retryable: false,
                    })
                    .await;
                    self.status(SessionState::Idle).await;
                }
            },
            Pending::Interrupt => match &result {
                Ok(_) => {
                    // Only the turn that was asked to stop is ended: a late acknowledgement must not
                    // close a turn that started in the meantime.
                    let ends = self.turn.as_ref().is_some_and(|turn| turn.cancel_requested);
                    if ends {
                        // A cancel is not an error and never produces an assistant message.
                        self.last_turn_id = self.turn.take().and_then(|turn| turn.id);
                        self.status(SessionState::Idle).await;
                    }
                }
                Err(error) => {
                    let message = format!("codex refused the interrupt: {}", error_message(error));
                    tracing::warn!(target: "grove::adapters::codex", "{message}");
                    self.emit(EventKind::Error {
                        message,
                        retryable: false,
                    })
                    .await;
                }
            },
        }
    }

    async fn on_request(&mut self, id: Value, method: &str, params: &Value) {
        if is_approval(method) {
            let request_id = id.to_string();
            let title = approval_title(method, params);
            let detail = truncate(&params.to_string(), DETAIL_LIMIT);
            if let Ok(mut pending) = self.permissions.lock() {
                pending.insert(request_id.clone(), id.clone());
            }
            tracing::debug!(
                target: "grove::adapters::codex",
                "codex asked for approval ({method}), request id {request_id}"
            );
            self.emit(EventKind::PermissionRequest {
                request_id,
                title,
                detail,
            })
            .await;
            return;
        }
        // Anything else the server asks for is a surface Grove does not implement yet: answer with a
        // JSON-RPC error so the harness is never left waiting on a reply that will not come.
        tracing::warn!(
            target: "grove::adapters::codex",
            "declining an unsupported codex server request: {method}"
        );
        self.send_error(id, -32601, &format!("grove does not support `{method}`"))
            .await;
    }

    async fn answer_permission(&mut self, request_id: &str, decision: Decision) {
        let id = self
            .permissions
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(request_id));
        let Some(id) = id else {
            tracing::warn!(
                target: "grove::adapters::codex",
                "no codex approval request {request_id} is waiting for an answer"
            );
            return;
        };
        // The v2 reply payload is not confirmed by a primary source: older builds document
        // `{"decision": "accept" | "acceptForSession" | "decline" | "cancel"}`. Send the minimal
        // vocabulary, and log so a changed shape is diagnosable.
        let decision_name = match decision {
            Decision::Approve => "accept",
            Decision::Deny => "decline",
        };
        tracing::debug!(
            target: "grove::adapters::codex",
            "answering codex approval {request_id} with {decision_name}"
        );
        self.send_result(id, json!({ "decision": decision_name }))
            .await;
        self.emit(EventKind::PermissionResolved {
            request_id: request_id.to_string(),
            decision,
        })
        .await;
    }

    async fn on_notification(&mut self, method: &str, params: &Value) {
        match method {
            "thread/started" => {
                if let Some(thread_id) = params.pointer("/thread/id").and_then(Value::as_str) {
                    self.set_thread(thread_id);
                }
            }
            "turn/started" => {
                let turn_id = params.pointer("/turn/id").and_then(Value::as_str);
                if self.matches_current_turn(turn_id) {
                    if let Some(turn_id) = turn_id {
                        self.note_turn_id(turn_id);
                    }
                    self.interrupt_if_requested().await;
                }
            }
            "item/started" => self.item_started(params).await,
            "item/completed" => self.item_completed(params).await,
            "item/agentMessage/delta" => self.agent_delta(params).await,
            "item/commandExecution/outputDelta" => self.exec_delta(params),
            "turn/diff/updated" => self.diff_updated(params).await,
            "thread/tokenUsage/updated" => self.usage_updated(params).await,
            "turn/completed" => self.turn_completed(params).await,
            "error" => self.protocol_error(params).await,
            other => tracing::debug!(
                target: "grove::adapters::codex",
                "skipping unknown codex notification: {other}"
            ),
        }
    }

    async fn item_started(&mut self, params: &Value) {
        if !self.matches_current_turn(params.get("turnId").and_then(Value::as_str)) {
            return;
        }
        self.note_turn_from(params);
        let Some(item) = params.get("item") else {
            return;
        };
        match item.get("type").and_then(Value::as_str).unwrap_or_default() {
            "commandExecution" => {
                let Some(call_id) = item.get("id").and_then(Value::as_str) else {
                    return;
                };
                let command = item
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or("command");
                self.emit(EventKind::ToolCall {
                    call_id: call_id.to_string(),
                    name: "exec".to_string(),
                    title: truncate(command, 200),
                })
                .await;
            }
            other => tracing::debug!(
                target: "grove::adapters::codex",
                "skipping a codex item of type {other}"
            ),
        }
    }

    async fn item_completed(&mut self, params: &Value) {
        // After a cancel the turn is over: late items must not resurrect it as transcript content.
        if !self.matches_current_turn(params.get("turnId").and_then(Value::as_str)) {
            return;
        }
        let Some(item) = params.get("item") else {
            return;
        };
        match item.get("type").and_then(Value::as_str).unwrap_or_default() {
            "agentMessage" => {
                let Some(text) = item.get("text").and_then(Value::as_str) else {
                    return;
                };
                if text.trim().is_empty() {
                    return;
                }
                self.emit(EventKind::Message {
                    role: Role::Assistant,
                    blocks: vec![Block::Text {
                        text: text.to_string(),
                    }],
                })
                .await;
            }
            "commandExecution" => {
                let Some(call_id) = item.get("id").and_then(Value::as_str).map(str::to_string)
                else {
                    return;
                };
                let output = item
                    .get("aggregatedOutput")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| self.exec_output.remove(&call_id))
                    .unwrap_or_default();
                let exit_code = item.get("exitCode").and_then(Value::as_i64);
                self.exec_output.remove(&call_id);
                self.emit(EventKind::ToolResult {
                    call_id,
                    status: command_status(item, exit_code),
                    summary: command_summary(&output, exit_code),
                })
                .await;
            }
            other => tracing::debug!(
                target: "grove::adapters::codex",
                "skipping a completed codex item of type {other}"
            ),
        }
    }

    async fn agent_delta(&mut self, params: &Value) {
        if !self.matches_current_turn(params.get("turnId").and_then(Value::as_str)) {
            return;
        }
        self.note_turn_from(params);
        let Some(delta) = params.get("delta").and_then(Value::as_str) else {
            return;
        };
        if delta.is_empty() {
            return;
        }
        self.emit(EventKind::Delta {
            text: delta.to_string(),
        })
        .await;
    }

    fn exec_delta(&mut self, params: &Value) {
        if !self.matches_current_turn(params.get("turnId").and_then(Value::as_str)) {
            return;
        }
        let (Some(item_id), Some(delta)) = (
            params.get("itemId").and_then(Value::as_str),
            params.get("delta").and_then(Value::as_str),
        ) else {
            return;
        };
        let buffer = self.exec_output.entry(item_id.to_string()).or_default();
        // A chatty command must not grow the actor's memory without bound.
        if buffer.len() < SUMMARY_LIMIT * 2 {
            buffer.push_str(delta);
        }
    }

    async fn diff_updated(&mut self, params: &Value) {
        if !self.matches_current_turn(params.get("turnId").and_then(Value::as_str)) {
            return;
        }
        let Some(patch) = params.get("diff").and_then(Value::as_str) else {
            return;
        };
        let patch = patch.trim();
        if patch.is_empty() {
            return;
        }
        // `turn/diff/updated` re-sends the whole aggregated patch as files land; render each distinct
        // revision once so the transcript does not double-render.
        if self
            .turn
            .as_ref()
            .and_then(|turn| turn.last_diff.as_deref())
            == Some(patch)
        {
            tracing::debug!(
                target: "grove::adapters::codex",
                "skipping a repeated turn/diff/updated"
            );
            return;
        }
        if let Some(turn) = self.turn.as_mut() {
            turn.last_diff = Some(patch.to_string());
        }
        let (path, added, removed) = summarize_diff(patch);
        self.emit(EventKind::Diff {
            path,
            patch: patch.to_string(),
            added,
            removed,
        })
        .await;
    }

    async fn usage_updated(&mut self, params: &Value) {
        if !self.matches_current_turn(params.get("turnId").and_then(Value::as_str)) {
            return;
        }
        let usage = params.get("tokenUsage").unwrap_or(params);
        let tokens = usage
            .get("total")
            .and_then(read_usage)
            .or_else(|| usage.get("last").and_then(read_usage));
        let Some((input_tokens, output_tokens)) = tokens else {
            tracing::debug!(
                target: "grove::adapters::codex",
                "skipping a token usage shape this build does not know: {usage}"
            );
            return;
        };
        self.emit(EventKind::Usage {
            input_tokens,
            output_tokens,
        })
        .await;
    }

    async fn turn_completed(&mut self, params: &Value) {
        // A `turn/completed` can arrive after the turn ended (a cancel resolves the turn at the
        // interrupt acknowledgement) or while the next turn is already running: match on the id, or a
        // late frame would end the wrong turn.
        if !self.matches_current_turn(params.pointer("/turn/id").and_then(Value::as_str)) {
            tracing::debug!(
                target: "grove::adapters::codex",
                "ignoring turn/completed for a turn that is not the one in flight"
            );
            return;
        }
        let status = params
            .pointer("/turn/status")
            .and_then(Value::as_str)
            .unwrap_or("completed");
        let report_error = matches!(status, "failed" | "error")
            && self
                .turn
                .as_ref()
                .is_some_and(|turn| !turn.error_reported && !turn.cancel_requested);
        self.last_turn_id = self.turn.take().and_then(|turn| turn.id);
        if report_error {
            self.emit(EventKind::Error {
                message: format!("codex reported the turn as {status}"),
                retryable: false,
            })
            .await;
        }
        self.status(SessionState::Idle).await;
    }

    async fn protocol_error(&mut self, params: &Value) {
        if let Some(turn) = self.turn.as_mut() {
            turn.error_reported = true;
        }
        self.emit(EventKind::Error {
            message: format!("codex reported an error: {}", error_message(params)),
            retryable: false,
        })
        .await;
    }

    /// The app-server process is gone. A turn that was running did not finish, so it is retryable —
    /// the stored thread id lets a resume continue the conversation.
    async fn process_ended(&mut self) {
        let unfinished = match self.turn.take() {
            Some(turn) => !turn.cancel_requested && !turn.error_reported,
            None => false,
        };
        if unfinished {
            self.emit(EventKind::Error {
                message: "codex app-server exited before the turn completed".to_string(),
                retryable: true,
            })
            .await;
        }
        self.status(SessionState::Exited).await;
        self.prompts.clear();
        self.requests.clear();
        if let Ok(mut pending) = self.permissions.lock() {
            pending.clear();
        }
    }

    async fn begin_turn(&mut self, prompt: &str) -> bool {
        let Some(thread_id) = self.thread_id.clone() else {
            self.fail("the codex thread was never started", false).await;
            return false;
        };
        self.exec_output.clear();
        self.turn = Some(Turn::default());
        self.status(SessionState::Busy).await;
        let params = turn_start_params(&thread_id, prompt);
        if self
            .send_request("turn/start", params, Pending::TurnStart)
            .await
            .is_none()
        {
            self.turn = None;
            self.fail("codex app-server closed its input", true).await;
            return false;
        }
        true
    }

    async fn cancel(&mut self) {
        if self.turn.is_none() {
            // Nothing is running: a cancel still confirms the idle state.
            self.status(SessionState::Idle).await;
            return;
        }
        if let Some(turn) = self.turn.as_mut() {
            turn.cancel_requested = true;
        }
        // Interrupt first, so the harness stops the turn before it reads the approval answer below.
        self.interrupt_if_requested().await;
        // A turn blocked on an approval must not stay blocked: answer it as cancelled.
        self.cancel_pending_approvals().await;
    }

    async fn cancel_pending_approvals(&mut self) {
        let pending: Vec<(String, Value)> = match self.permissions.lock() {
            Ok(mut pending) => pending.drain().collect(),
            Err(_) => return,
        };
        for (request_id, id) in pending {
            tracing::debug!(
                target: "grove::adapters::codex",
                "answering codex approval {request_id} as cancel: the turn was interrupted"
            );
            self.send_result(id, json!({ "decision": "cancel" })).await;
            self.emit(EventKind::PermissionResolved {
                request_id,
                decision: Decision::Deny,
            })
            .await;
        }
    }

    /// `turn/interrupt {threadId, turnId}` — a request, not a kill. When the cancel lands before the
    /// turn id is known the interrupt is sent as soon as it is.
    async fn interrupt_if_requested(&mut self) {
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        let params = {
            let Some(turn) = self.turn.as_mut() else {
                return;
            };
            if !turn.cancel_requested || turn.interrupt_sent {
                return;
            }
            let Some(turn_id) = turn.id.clone() else {
                tracing::debug!(
                    target: "grove::adapters::codex",
                    "cancel requested before the turn id arrived; interrupting once it does"
                );
                return;
            };
            turn.interrupt_sent = true;
            json!({ "threadId": thread_id, "turnId": turn_id })
        };
        self.send_request("turn/interrupt", params, Pending::Interrupt)
            .await;
    }
}

/// Drive the actor for the life of the session. The manager's pump ends the session when this task
/// drops the event sender.
async fn run(
    mut runner: Runner,
    initial_prompt: Option<String>,
    mut commands: mpsc::UnboundedReceiver<TurnCommand>,
) {
    let resumed = runner.resume.is_some();
    runner
        .emit(EventKind::SessionStarted {
            harness: HARNESS.to_string(),
            model: runner.model.clone(),
            resumed,
        })
        .await;
    runner.status(SessionState::Starting).await;

    let Some(mut lines) = runner.spawn_process().await else {
        return;
    };
    if !runner.handshake(&mut lines).await {
        runner.close_process().await;
        return;
    }
    runner.status(SessionState::Ready).await;

    if let Some(prompt) = initial_prompt {
        runner.prompts.push_back(prompt);
    }

    loop {
        if let Some(prompt) = runner.prompts.pop_front() {
            if !runner.begin_turn(&prompt).await {
                runner.close_process().await;
                return;
            }
            continue;
        }

        let keep_running = tokio::select! {
            message = lines.recv() => match message {
                Some(message) => {
                    runner.dispatch(message).await;
                    true
                }
                // The process closed stdout: it is gone.
                None => {
                    runner.process_ended().await;
                    false
                }
            },
            command = commands.recv() => match command {
                Some(TurnCommand::Send(text)) => {
                    runner.prompts.push_back(text);
                    true
                }
                Some(TurnCommand::Cancel) => {
                    runner.cancel().await;
                    true
                }
                Some(TurnCommand::RespondPermission { request_id, decision }) => {
                    runner.answer_permission(&request_id, decision).await;
                    true
                }
                Some(TurnCommand::Shutdown) | None => false,
            },
        };
        if !keep_running {
            runner.close_process().await;
            return;
        }
    }
}

/// Read app-server stdout line by line into [`Message`]s. A dedicated task keeps the run loop's
/// `select!` free of the reader's borrow.
fn spawn_reader(stdout: ChildStdout) -> (tokio::task::JoinHandle<()>, mpsc::Receiver<Message>) {
    let (messages, rx) = mpsc::channel(EVENT_BUFFER);
    let handle = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if let Some(message) = parse_message(&line) {
                        if messages.send(message).await.is_err() {
                            return;
                        }
                    }
                }
                // Either the process exited or the pipe was closed: both mean "no more frames".
                Ok(None) => return,
                Err(error) => {
                    tracing::warn!(
                        target: "grove::adapters::codex",
                        "reading app-server stdout failed: {error}"
                    );
                    return;
                }
            }
        }
    });
    (handle, rx)
}

/// Decode one JSON-RPC line. app-server omits the `"jsonrpc":"2.0"` header, so dispatch is on which
/// of `method`/`id` is present: both = request, method only = notification, id only = response.
fn parse_message(line: &str) -> Option<Message> {
    let line = line.trim();
    if line.is_empty() {
        // Windows harnesses emit stray CRLF and blank lines; they are not an error.
        return None;
    }
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            tracing::debug!(
                target: "grove::adapters::codex",
                "skipping a malformed line ({error}): {}",
                truncate(line, 200)
            );
            return None;
        }
    };
    let object = match value.as_object() {
        Some(object) => object,
        None => {
            tracing::debug!(
                target: "grove::adapters::codex",
                "skipping a non-object line: {}",
                truncate(line, 200)
            );
            return None;
        }
    };
    let params = || object.get("params").cloned().unwrap_or(Value::Null);
    match (
        object.get("method").and_then(Value::as_str),
        object.get("id").cloned(),
    ) {
        (Some(method), Some(id)) => Some(Message::Request {
            id,
            method: method.to_string(),
            params: params(),
        }),
        (Some(method), None) => Some(Message::Notification {
            method: method.to_string(),
            params: params(),
        }),
        (None, Some(id)) => {
            let result = match object.get("error") {
                Some(error) => Err(error.clone()),
                None => Ok(object.get("result").cloned().unwrap_or(Value::Null)),
            };
            Some(Message::Response { id, result })
        }
        (None, None) => {
            tracing::debug!(
                target: "grove::adapters::codex",
                "skipping a line with neither a method nor an id: {}",
                truncate(line, 200)
            );
            None
        }
    }
}

/// `codex app-server` is the documented launch form; whatever the user configured comes first so a
/// wrapper can inspect it.
fn launch_args(extra: &[String]) -> Vec<String> {
    let mut args = extra.to_vec();
    args.push("app-server".to_string());
    args
}

/// app-server has **no** `protocolVersion`: version identity is the binary version, and the shapes are
/// generated per release. Only the client identity and the capability flags go in.
fn initialize_params() -> Value {
    json!({
        "clientInfo": {
            "name": "grove",
            "title": "Grove",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "capabilities": { "experimentalApi": true },
    })
}

fn thread_start_params(cwd: &Path, model: Option<&str>) -> Value {
    let mut params = json!({ "cwd": cwd.to_string_lossy() });
    if let Some(model) = model {
        params["model"] = json!(model);
    }
    params
}

/// The documented `turn/start` input shape: one text element with an empty `text_elements` array.
fn turn_start_params(thread_id: &str, prompt: &str) -> Value {
    json!({
        "threadId": thread_id,
        "input": [{ "type": "text", "text": prompt, "text_elements": [] }],
    })
}

fn is_approval(method: &str) -> bool {
    method.ends_with("requestApproval")
        || matches!(method, "execCommandApproval" | "applyPatchApproval")
}

fn approval_title(method: &str, params: &Value) -> String {
    for key in ["command", "path", "reason"] {
        if let Some(value) = params.get(key).and_then(Value::as_str) {
            let value = value.trim();
            if !value.is_empty() {
                return truncate(value, 200);
            }
        }
    }
    method.to_string()
}

/// The exit code is the status; it is spelled out in the summary when it is not zero, because a failed
/// command is exactly when the number matters.
fn command_status(item: &Value, exit_code: Option<i64>) -> ToolStatus {
    match exit_code {
        Some(0) => ToolStatus::Ok,
        Some(_) => ToolStatus::Error,
        None => match item
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "completed" => ToolStatus::Ok,
            "declined" | "cancelled" | "interrupted" => ToolStatus::Cancelled,
            _ => ToolStatus::Error,
        },
    }
}

fn command_summary(output: &str, exit_code: Option<i64>) -> String {
    let mut summary = truncate(output.trim_end(), SUMMARY_LIMIT);
    if let Some(code) = exit_code.filter(|code| *code != 0) {
        if !summary.is_empty() {
            summary.push('\n');
        }
        summary.push_str(&format!("exit code: {code}"));
    }
    if summary.is_empty() {
        summary = "no output".to_string();
    }
    summary
}

/// Added/removed line counts and a path for the aggregated turn diff: `turn/diff/updated` sends the
/// whole unified diff, so the counts and the path are read from it rather than invented.
fn summarize_diff(patch: &str) -> (String, u32, u32) {
    let mut added = 0;
    let mut removed = 0;
    let mut paths: Vec<String> = Vec::new();
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            push_diff_path(&mut paths, rest);
        } else if let Some(rest) = line.strip_prefix("--- ") {
            push_diff_path(&mut paths, rest);
        } else if line.starts_with('+') {
            added += 1;
        } else if line.starts_with('-') {
            removed += 1;
        }
    }
    let path = match paths.len() {
        1 => paths.remove(0),
        // Several files in one turn: the aggregated patch is the transcript entry.
        _ => "*".to_string(),
    };
    (path, added, removed)
}

fn push_diff_path(paths: &mut Vec<String>, raw: &str) {
    let raw = raw.trim();
    if raw == "/dev/null" {
        return;
    }
    let path = raw
        .strip_prefix("b/")
        .or_else(|| raw.strip_prefix("a/"))
        .unwrap_or(raw);
    if path.is_empty() || paths.iter().any(|existing| existing == path) {
        return;
    }
    paths.push(path.to_string());
}

/// Read a `TokenUsageBreakdown`. The field spelling is generated per Codex release, so both cases are
/// accepted; a shape this build does not recognise is a log line, never a fabricated `0/0`.
fn read_usage(value: &Value) -> Option<(u64, u64)> {
    let input = read_tokens(value, INPUT_TOKEN_KEYS);
    let output = read_tokens(value, OUTPUT_TOKEN_KEYS);
    match (input, output) {
        (None, None) => None,
        (input, output) => Some((input.unwrap_or(0), output.unwrap_or(0))),
    }
}

fn read_tokens(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

/// A JSON-RPC error or an `error` notification, whichever shape it arrived in.
fn error_message(value: &Value) -> String {
    if let Some(message) = value.get("message").and_then(Value::as_str) {
        return truncate(message, 300);
    }
    if let Some(message) = value.pointer("/error/message").and_then(Value::as_str) {
        return truncate(message, 300);
    }
    let raw = value.to_string();
    if raw == "null" || raw == "{}" {
        return "unknown error".to_string();
    }
    truncate(&raw, 300)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::ContextDoc;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn the_launch_form_is_codex_app_server() {
        assert_eq!(launch_args(&[]), strings(&["app-server"]));
        assert_eq!(
            launch_args(&strings(&["--profile", "work"])),
            strings(&["--profile", "work", "app-server"]),
            "configured args come first so a wrapper can inspect them"
        );
    }

    #[test]
    fn initialize_carries_a_client_identity_and_no_protocol_version() {
        let params = initialize_params();
        assert_eq!(
            params.pointer("/clientInfo/name").and_then(Value::as_str),
            Some("grove")
        );
        assert!(params.pointer("/clientInfo/version").is_some());
        assert!(
            params.get("protocolVersion").is_none(),
            "app-server has no protocol version field: {params}"
        );
    }

    #[test]
    fn turn_start_uses_the_documented_text_input_shape() {
        let params = turn_start_params("thr_1", "fix the test");
        assert_eq!(
            params,
            json!({
                "threadId": "thr_1",
                "input": [{ "type": "text", "text": "fix the test", "text_elements": [] }],
            })
        );
    }

    #[test]
    fn thread_start_carries_the_cwd_and_only_the_model_it_was_given() {
        let cwd = Path::new("C:/work/grove");
        let with_model = thread_start_params(cwd, Some("gpt-5-codex"));
        assert_eq!(
            with_model.pointer("/model").and_then(Value::as_str),
            Some("gpt-5-codex")
        );
        assert!(with_model.get("cwd").is_some());
        let without = thread_start_params(cwd, None);
        assert!(without.get("model").is_none());
    }

    #[test]
    fn config_overrides_the_command_and_extends_the_args() {
        let config = json!({
            "command": "node",
            "args": ["fixtures/harnesses/codex_fake.mjs", "--mode", "happy", 42],
        });
        let (command, args) = command_from_config(&config, "codex");
        assert_eq!(command, "node");
        assert_eq!(
            args,
            strings(&["fixtures/harnesses/codex_fake.mjs", "--mode", "happy"]),
            "non-string args are ignored rather than fatal"
        );

        let (command, args) = command_from_config(&json!({}), "codex");
        assert_eq!(command, "codex");
        assert!(args.is_empty());
    }

    #[test]
    fn context_documents_are_prepended_to_the_first_turn() {
        let documents = vec![
            ContextDoc {
                id: "grove-architecture".into(),
                title: "Grove — architecture".into(),
                body: "# Architecture\n\nThe contract.\n".into(),
            },
            ContextDoc {
                id: "plan".into(),
                title: "Plan".into(),
                body: "- step one".into(),
            },
        ];
        let rendered = render_prompt(Some("ship ticket 10"), &documents).expect("a prompt");
        assert!(rendered.starts_with("# Context\n\n"));
        assert!(rendered.contains("## Grove — architecture"));
        assert!(rendered.contains("grove-architecture"));
        assert!(rendered.contains("- step one"));
        assert!(rendered.contains("---\n\nship ticket 10"));

        assert_eq!(
            render_prompt(Some("plain"), &[]).as_deref(),
            Some("plain"),
            "no documents means the prompt is untouched"
        );
        assert_eq!(render_prompt(None, &[]), None);
        assert!(
            render_prompt(None, &documents).is_some(),
            "documents alone are a prompt"
        );
    }

    #[test]
    fn lines_are_classified_by_method_and_id() {
        assert!(matches!(
            parse_message("{\"method\":\"item/agentMessage/delta\",\"params\":{\"delta\":\"hi\"}}"),
            Some(Message::Notification { method, .. }) if method == "item/agentMessage/delta"
        ));
        assert!(matches!(
            parse_message("{\"id\":3,\"method\":\"item/commandExecution/requestApproval\",\"params\":{}}"),
            Some(Message::Request { method, .. }) if method == "item/commandExecution/requestApproval"
        ));
        assert!(matches!(
            parse_message("{\"id\":10,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}"),
            Some(Message::Response { result: Ok(_), .. })
        ));
        assert!(matches!(
            parse_message("{\"id\":11,\"error\":{\"code\":-32000,\"message\":\"boom\"}}"),
            Some(Message::Response { result: Err(_), .. })
        ));
        assert!(parse_message("not json at all").is_none());
        assert!(parse_message("[1,2,3]").is_none());
        assert!(parse_message("   ").is_none());
        assert!(parse_message("{\"params\":{}}").is_none());
    }

    #[test]
    fn approvals_are_recognised_and_titled_by_what_will_run() {
        assert!(is_approval("item/commandExecution/requestApproval"));
        assert!(is_approval("execCommandApproval"));
        assert!(!is_approval("item/tool/requestUserInput"));
        assert_eq!(
            approval_title(
                "item/commandExecution/requestApproval",
                &json!({ "command": "rm -rf build", "reason": "clean" })
            ),
            "rm -rf build"
        );
        assert_eq!(
            approval_title(
                "item/fileChange/requestApproval",
                &json!({ "path": "src/main.rs" })
            ),
            "src/main.rs"
        );
        assert_eq!(
            approval_title("applyPatchApproval", &json!({})),
            "applyPatchApproval"
        );
        assert!(
            error_message(&json!({ "code": -32000, "message": "529 overloaded" }))
                .contains("529 overloaded")
        );
        assert_eq!(error_message(&json!({})), "unknown error");
    }

    #[test]
    fn command_results_carry_the_exit_code_and_the_aggregated_output() {
        assert_eq!(
            command_status(&json!({ "exitCode": 0 }), Some(0)),
            ToolStatus::Ok
        );
        assert_eq!(
            command_status(&json!({ "exitCode": 2 }), Some(2)),
            ToolStatus::Error
        );
        assert_eq!(
            command_status(&json!({ "exitCode": null, "status": "completed" }), None),
            ToolStatus::Ok
        );
        assert_eq!(
            command_status(&json!({ "exitCode": null, "status": "declined" }), None),
            ToolStatus::Cancelled
        );
        assert_eq!(
            command_summary("all 24 passed", Some(0)),
            "all 24 passed",
            "a zero exit code is already the ok status"
        );
        assert_eq!(
            command_summary("boom\n", Some(1)),
            "boom\nexit code: 1",
            "a failing command spells out its exit code"
        );
        assert_eq!(command_summary("", Some(0)), "no output");
        let long = "x".repeat(SUMMARY_LIMIT + 10);
        assert_eq!(
            command_summary(&long, None).chars().count(),
            SUMMARY_LIMIT + 1
        );
    }

    #[test]
    fn the_aggregated_diff_is_counted_and_named_from_its_own_patch() {
        let patch = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,2 +1,3 @@\n one\n+two\n three\n";
        let (path, added, removed) = summarize_diff(patch);
        assert_eq!(path, "src/a.rs");
        assert_eq!((added, removed), (1, 0));
        assert_eq!(summarize_diff("").0, "*");

        let multi =
            "--- a/one.rs\n+++ b/one.rs\n@@\n-a\n+b\n--- a/two.rs\n+++ b/two.rs\n@@\n-c\n+d\n";
        assert_eq!(
            summarize_diff(multi).0,
            "*",
            "a turn that touched several files is shown as one aggregated entry"
        );
        assert_eq!(summarize_diff(multi).1, 2);
        assert_eq!(summarize_diff(multi).2, 2);
        assert_eq!(
            summarize_diff("--- /dev/null\n+++ b/new.rs\n@@\n+x\n").0,
            "new.rs",
            "an added file takes its path from the +++ side"
        );
    }

    #[test]
    fn token_usage_accepts_the_spellings_the_schema_has_used() {
        assert_eq!(
            read_usage(&json!({ "input_tokens": 40, "output_tokens": 15 })),
            Some((40, 15))
        );
        assert_eq!(
            read_usage(&json!({ "inputTokens": 7, "outputTokens": 3 })),
            Some((7, 3))
        );
        assert_eq!(read_usage(&json!({ "input_tokens": 9 })), Some((9, 0)));
        assert_eq!(
            read_usage(&json!({ "modelContextWindow": 272000 })),
            None,
            "a shape this build does not know is skipped, not reported as 0/0"
        );
    }

    #[test]
    fn capabilities_are_honest() {
        let capabilities = CodexAdapter::default().capabilities();
        assert!(capabilities.streaming_deltas);
        assert!(capabilities.tool_calls);
        assert!(capabilities.diffs);
        assert!(capabilities.permissions);
        assert!(capabilities.resume);
        assert!(capabilities.usage);
        assert!(!capabilities.images);
        assert!(!capabilities.mcp);
    }

    fn deps() -> AdapterDeps {
        AdapterDeps {
            paths: crate::paths::AppPaths::new(std::env::temp_dir().join("grove-codex-tests")),
            http: reqwest::Client::new(),
            secrets: Arc::new(crate::secrets::MemorySecretStore::new()),
        }
    }

    #[test]
    fn detection_matches_the_binary_it_was_configured_with() {
        let binary = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let installed = CodexAdapter::with_command(binary).detect(&deps());
        assert!(installed.installed);
        assert!(installed.path.is_some());
        assert!(
            installed.version.is_none(),
            "detection never runs the binary"
        );

        let missing =
            CodexAdapter::with_command("grove-definitely-not-a-real-binary").detect(&deps());
        assert!(!missing.installed);
        assert!(missing
            .detail
            .unwrap()
            .contains("grove-definitely-not-a-real-binary"));
    }
}
