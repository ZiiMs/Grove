//! The Claude Code adapter: `claude --print --output-format stream-json --verbose`.
//!
//! The CLI's `stream-json` mode is newline-delimited JSON on stdout, one object per line, with the
//! session id on every line. This adapter runs the documented headless shape — one process per turn,
//! `--resume <session_id>` to continue — and maps the lines it recognises onto the normalized event
//! model. Everything else is logged and dropped: protocol drift is a log line, never a dead session.
//!
//! Deliberate limits, reflected in [`ClaudeAdapter::capabilities`]:
//!
//! - **No interactive permissions.** A one-shot `-p` run has no permission channel; the control
//!   protocol that could answer one is undocumented and version-dependent. A call that would prompt
//!   is denied by the CLI, which arrives here as an `error` event. Users grant access with
//!   `--permission-mode` or `--allowedTools` in `config.args`.
//! - **No synthesized diffs.** The CLI sends no patch for `Edit`/`Write`, so `diffs` is false rather
//!   than inventing one.
//! - **Cancel kills the process tree.** In one-shot mode that is the only stop: there is no
//!   `interrupt` receipt to wait for.
//!
//! Command override: `config.command` and `config.args` replace/extend the binary, which is how a
//! user points Grove at a wrapper and how every test points it at `fixtures/harnesses/claude_fake.mjs`.

use super::{
    AdapterDeps, AdapterError, Capabilities, ContextDoc, Detection, HarnessAdapter, HarnessId,
    HarnessSession, ResumeToken, StartContext, UserInput,
};
use crate::sessions::event::{
    Block, Decision, EventKind, HarnessEvent, Role, SessionState, ToolStatus,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

const DEFAULT_COMMAND: &str = "claude";
/// The harness's own name, as written into `SessionStarted` and stored on the session row.
const HARNESS: &str = "claude";
/// A tool result can be a whole file; the transcript wants a summary, not a second copy of it.
const SUMMARY_LIMIT: usize = 500;
/// How long to wait for a child that closed stdout to also exit before killing it.
const REAP_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_BUFFER: usize = 1024;

pub struct ClaudeAdapter {
    /// The binary detection looks for. `config.command` can still override it per session.
    command: String,
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self {
            command: DEFAULT_COMMAND.to_string(),
        }
    }
}

impl ClaudeAdapter {
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
impl HarnessAdapter for ClaudeAdapter {
    fn id(&self) -> HarnessId {
        HarnessId(HARNESS)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // A `Delta` per assistant text block, plus real token deltas when the CLI emits
            // `stream_event` lines (it does when `--include-partial-messages` is in `config.args`).
            streaming_deltas: true,
            tool_calls: true,
            // The CLI sends no patch for edits; a synthesized diff is a later ticket, so do not claim it.
            diffs: false,
            // One-shot `-p` has no answerable permission request (see the module docs).
            permissions: false,
            resume: true,
            images: false,
            mcp: false,
            usage: true,
        }
    }

    /// Side-effect free: it resolves the binary on `PATH` but never runs it. A missing binary is
    /// reported here and does not stop [`HarnessAdapter::start`]; the failure surfaces as an error
    /// event on the first turn so the session row and its transcript exist either way.
    fn detect(&self, _deps: &AdapterDeps) -> Detection {
        match find_on_path(&self.command) {
            Some(path) => Detection {
                installed: true,
                // `--version` belongs to the doctor (ticket 07), which is allowed to run probes.
                version: None,
                path: Some(crate::paths::display_path(&path)),
                detail: None,
            },
            None => Detection::missing(format!(
                "`{}` is not on PATH; install Claude Code or set config.command",
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
        let initial_prompt = render_prompt(ctx.prompt.as_deref(), &ctx.context_files);

        let (events, events_rx) = mpsc::channel(EVENT_BUFFER);
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let token = Arc::new(Mutex::new(ctx.resume_token.clone()));
        let runner = Runner {
            events,
            token: Arc::clone(&token),
            cwd: ctx.cwd,
            command,
            extra_args,
            model: ctx.model,
            pending: VecDeque::new(),
            shutdown: false,
        };
        tokio::spawn(run(runner, initial_prompt, commands_rx));

        Ok(Box::new(ClaudeSession {
            events: Some(events_rx),
            commands,
            token,
        }))
    }
}

enum TurnCommand {
    Send(String),
    Cancel,
    Shutdown,
}

pub struct ClaudeSession {
    events: Option<mpsc::Receiver<HarnessEvent>>,
    commands: mpsc::UnboundedSender<TurnCommand>,
    token: ResumeToken,
}

impl ClaudeSession {
    fn send_command(&self, command: TurnCommand) -> Result<(), AdapterError> {
        self.commands
            .send(command)
            .map_err(|_| AdapterError::Other("the claude session has ended".into()))
    }
}

#[async_trait::async_trait]
impl HarnessSession for ClaudeSession {
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
        _request_id: &str,
        _decision: Decision,
    ) -> Result<(), AdapterError> {
        Err(AdapterError::Unsupported(
            "a one-shot claude session has no permission channel; grant access with \
             --permission-mode or --allowedTools in the adapter config"
                .into(),
        ))
    }

    async fn cancel(&mut self) -> Result<(), AdapterError> {
        self.send_command(TurnCommand::Cancel)
    }

    async fn shutdown(&mut self) -> Result<(), AdapterError> {
        // The manager owns the final `exited` transition, exactly like the mock adapter.
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

/// One process per turn. Between turns the actor just waits for commands, which is what keeps a
/// session alive across a crash and lets the manager resume it from the captured session id.
struct Runner {
    events: mpsc::Sender<HarnessEvent>,
    token: ResumeToken,
    cwd: PathBuf,
    command: String,
    extra_args: Vec<String>,
    model: Option<String>,
    /// Prompts that arrived while a turn was running. They run in order, one process at a time:
    /// two writers against one conversation id would interleave turns.
    pending: VecDeque<String>,
    shutdown: bool,
}

impl Runner {
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

    fn resume_token(&self) -> Option<String> {
        self.token.lock().ok().and_then(|token| token.clone())
    }

    /// Run one turn to completion, a cancel, or an unexpected exit.
    async fn turn(&mut self, prompt: String, commands: &mut mpsc::UnboundedReceiver<TurnCommand>) {
        self.status(SessionState::Busy).await;

        let resume = self.resume_token();
        let args = launch_args(self.model.as_deref(), resume.as_deref(), &self.extra_args);
        let mut command = Command::new(&self.command);
        command
            .args(&args)
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
                self.emit(EventKind::Error {
                    message: format!("could not launch `{}`: {error}", self.command),
                    retryable: false,
                })
                .await;
                self.status(SessionState::Exited).await;
                self.pending.clear();
                return;
            }
        };
        let pid = child.id();

        // The prompt goes on stdin and the pipe closes: that is the documented one-shot form, and it
        // sidesteps Windows command-line length and quoting limits.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(prompt.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
        }

        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                self.status(SessionState::Exited).await;
                return;
            }
        };
        let mut lines = BufReader::new(stdout).lines();
        let stderr_task = child.stderr.take().map(|stderr| {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "grove::adapters::claude", "stderr: {line}");
                }
            })
        });

        let mut saw_result = false;
        let mut cancelled = false;
        let mut shutdown = false;
        loop {
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(line)) => {
                        if self.handle_line(&line).await {
                            saw_result = true;
                        }
                    }
                    // The child closed stdout: it is done or dying. `result` tells us which.
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(
                            target: "grove::adapters::claude",
                            "reading stdout failed: {error}"
                        );
                        break;
                    }
                },
                command = commands.recv() => match command {
                    Some(TurnCommand::Send(text)) => self.pending.push_back(text),
                    Some(TurnCommand::Cancel) => {
                        // Stop reading *before* killing: a line already in the pipe must not become
                        // a message after the user cancelled.
                        cancelled = true;
                        break;
                    }
                    Some(TurnCommand::Shutdown) => {
                        shutdown = true;
                        break;
                    }
                    None => {
                        shutdown = true;
                        break;
                    }
                },
            }
        }

        let status = if cancelled || shutdown {
            kill_process_tree(pid, &mut child).await;
            None
        } else {
            match tokio::time::timeout(REAP_TIMEOUT, child.wait()).await {
                Ok(status) => status.ok(),
                Err(_) => {
                    kill_process_tree(pid, &mut child).await;
                    None
                }
            }
        };
        if let Some(task) = stderr_task {
            task.abort();
        }

        if shutdown {
            self.shutdown = true;
            return;
        }
        if cancelled {
            // A cancel that arrives after `result` must not overwrite the outcome the harness already
            // reported (idle for success, error for a failed turn).
            if !saw_result {
                self.status(SessionState::Idle).await;
            }
            return;
        }
        if !saw_result {
            // The process ended mid-turn: an exit the harness did not describe, so it is retryable —
            // the captured session id lets a resume continue the conversation.
            let detail = match status {
                Some(status) => format!(" (exit {status})"),
                None => String::new(),
            };
            self.emit(EventKind::Error {
                message: format!("claude exited before sending a result{detail}"),
                retryable: true,
            })
            .await;
            self.status(SessionState::Exited).await;
            self.pending.clear();
        }
    }

    /// Map one line. Returns `true` for the `result` line, which ends the turn.
    async fn handle_line(&mut self, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            // Windows harnesses emit stray CRLF and blank lines; they are not an error.
            return false;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                tracing::debug!(
                    target: "grove::adapters::claude",
                    "skipping a malformed line ({error}): {}",
                    truncate(line, 200)
                );
                return false;
            }
        };

        // The session id is on every line, so it survives a crash before `result`.
        if let Some(session_id) = value.get("session_id").and_then(Value::as_str) {
            self.set_session_id(session_id);
        }

        match value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "system" => self.handle_system(&value).await,
            "assistant" => self.handle_assistant(&value).await,
            "user" => self.handle_user(&value).await,
            "stream_event" => self.handle_stream_event(&value).await,
            "result" => return self.handle_result(&value).await,
            // The undocumented control channel. Answering it would need streaming-input mode, so it
            // is logged rather than half-supported.
            "control_request" | "sdk_control_request" => tracing::warn!(
                target: "grove::adapters::claude",
                "ignoring a claude control request: one-shot sessions have no permission channel"
            ),
            other => tracing::debug!(
                target: "grove::adapters::claude",
                "skipping unknown claude line type: {}",
                if other.is_empty() { "<missing type>" } else { other }
            ),
        }
        false
    }

    async fn handle_system(&mut self, value: &Value) {
        match value
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            // `init` is where a doctor could read capabilities[] and the model; the session id was
            // already captured above, which is all a session needs.
            "init" => {
                let model = value
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                tracing::debug!(
                    target: "grove::adapters::claude",
                    "claude initialised with model {model} and capabilities {:?}",
                    value.get("capabilities")
                );
            }
            "api_retry" => {
                let error = value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("an API error");
                let message = match (
                    value.get("attempt").and_then(Value::as_u64),
                    value.get("max_retries").and_then(Value::as_u64),
                ) {
                    (Some(attempt), Some(max)) => {
                        format!("claude is retrying after {error} (attempt {attempt}/{max})")
                    }
                    _ => format!("claude is retrying after {error}"),
                };
                self.emit(EventKind::Error {
                    message,
                    retryable: true,
                })
                .await;
                self.status(SessionState::Busy).await;
            }
            "permission_denied" => {
                self.emit(EventKind::Error {
                    message: "claude denied a tool call: a one-shot session has no interactive \
                              permission channel; pass --permission-mode or --allowedTools in the \
                              adapter config"
                        .into(),
                    retryable: false,
                })
                .await;
            }
            other => tracing::debug!(
                target: "grove::adapters::claude",
                "skipping unknown claude system subtype: {other}"
            ),
        }
    }

    async fn handle_assistant(&mut self, value: &Value) {
        if is_subagent(value) {
            tracing::debug!(
                target: "grove::adapters::claude",
                "skipping a subagent assistant message"
            );
            return;
        }
        let Some(content) = value
            .get("message")
            .and_then(|message| message.get("content"))
        else {
            return;
        };

        let mut blocks = Vec::new();
        let mut text = String::new();
        let mut calls = Vec::new();
        match content {
            Value::String(raw) => {
                text.push_str(raw);
                blocks.push(Block::Text { text: raw.clone() });
            }
            Value::Array(items) => {
                for item in items {
                    match item.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(raw) = item.get("text").and_then(Value::as_str) {
                                text.push_str(raw);
                                blocks.push(Block::Text {
                                    text: raw.to_string(),
                                });
                            }
                        }
                        Some("thinking") => {
                            let raw = item
                                .get("thinking")
                                .or_else(|| item.get("text"))
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            blocks.push(Block::Thinking {
                                text: raw.to_string(),
                            });
                        }
                        Some("tool_use") => {
                            let call_id = item
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let name = item
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            if !call_id.is_empty() && !name.is_empty() {
                                calls.push((call_id, name, item.get("input").cloned()));
                            }
                        }
                        other => tracing::debug!(
                            target: "grove::adapters::claude",
                            "skipping unknown assistant block type: {other:?}"
                        ),
                    }
                }
            }
            _ => return,
        }

        // The streamed delta first, then the message that completes it: the transcript folds the two
        // together, so a reload from SQLite shows the reply once.
        if !text.is_empty() {
            self.emit(EventKind::Delta { text }).await;
        }
        if !blocks.is_empty() {
            self.emit(EventKind::Message {
                role: Role::Assistant,
                blocks,
            })
            .await;
        }
        for (call_id, name, input) in calls {
            let title = tool_title(&name, input.as_ref());
            self.emit(EventKind::ToolCall {
                call_id,
                name,
                title,
            })
            .await;
        }
    }

    async fn handle_user(&mut self, value: &Value) {
        if is_subagent(value) {
            tracing::debug!(
                target: "grove::adapters::claude",
                "skipping a subagent tool result"
            );
            return;
        }
        let Some(content) = value
            .get("message")
            .and_then(|message| message.get("content"))
        else {
            return;
        };
        let Value::Array(items) = content else {
            return;
        };
        for item in items {
            if item.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let call_id = item
                .get("tool_use_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if call_id.is_empty() {
                continue;
            }
            let status = if item
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                ToolStatus::Error
            } else {
                ToolStatus::Ok
            };
            let summary = summarize(item.get("content"));
            self.emit(EventKind::ToolResult {
                call_id,
                status,
                summary,
            })
            .await;
        }
    }

    async fn handle_stream_event(&mut self, value: &Value) {
        // Only present with `--include-partial-messages`; tolerated when it is not, and when the
        // event is a shape this adapter does not use (thinking deltas are carried by `assistant`).
        let Some(event) = value.get("event") else {
            return;
        };
        if event.get("type").and_then(Value::as_str) != Some("content_block_delta") {
            return;
        }
        let Some(delta) = event.get("delta") else {
            return;
        };
        if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
            return;
        }
        if let Some(text) = delta.get("text").and_then(Value::as_str) {
            if !text.is_empty() {
                self.emit(EventKind::Delta {
                    text: text.to_string(),
                })
                .await;
            }
        }
    }

    async fn handle_result(&mut self, value: &Value) -> bool {
        let subtype = value
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or("success");
        let is_error = value
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || subtype.starts_with("error");

        if let Some(usage) = value.get("usage") {
            self.emit(EventKind::Usage {
                input_tokens: usage
                    .get("input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                output_tokens: usage
                    .get("output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            })
            .await;
        }

        if is_error {
            let detail = value
                .get("result")
                .and_then(Value::as_str)
                .unwrap_or(subtype);
            self.emit(EventKind::Error {
                message: format!("claude reported {subtype}: {}", truncate(detail, 300)),
                retryable: false,
            })
            .await;
            self.status(SessionState::Error).await;
        } else {
            self.status(SessionState::Idle).await;
        }
        true
    }
}

/// Drive the actor for the life of the session. The manager's pump ends the session when this task
/// drops the event sender.
async fn run(
    mut runner: Runner,
    initial_prompt: Option<String>,
    mut commands: mpsc::UnboundedReceiver<TurnCommand>,
) {
    let resumed = runner.resume_token().is_some();
    runner
        .emit(EventKind::SessionStarted {
            harness: HARNESS.to_string(),
            model: runner.model.clone(),
            resumed,
        })
        .await;
    runner.status(SessionState::Starting).await;
    runner.status(SessionState::Ready).await;

    if let Some(prompt) = initial_prompt {
        runner.pending.push_back(prompt);
    }
    loop {
        while let Some(prompt) = runner.pending.pop_front() {
            runner.turn(prompt, &mut commands).await;
            if runner.shutdown {
                return;
            }
        }
        match commands.recv().await {
            Some(TurnCommand::Send(prompt)) => runner.pending.push_back(prompt),
            // Cancel with nothing running is a no-op that still confirms the idle state.
            Some(TurnCommand::Cancel) => runner.status(SessionState::Idle).await,
            Some(TurnCommand::Shutdown) | None => return,
        }
    }
}

/// The documented flags, plus whatever the user configured. Extra args come first so a wrapper can
/// inspect them and so `--permission-mode`/`--include-partial-messages` behave like user flags.
fn launch_args(model: Option<&str>, resume: Option<&str>, extra: &[String]) -> Vec<String> {
    let mut args: Vec<String> = extra.to_vec();
    args.extend([
        "--print".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
    ]);
    if let Some(model) = model {
        args.push("--model".to_string());
        args.push(model.to_string());
    }
    if let Some(resume) = resume {
        args.push("--resume".to_string());
        args.push(resume.to_string());
    }
    args
}

fn command_from_config(config: &Value, default_command: &str) -> (String, Vec<String>) {
    let command = config
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .unwrap_or(default_command)
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
    (command, args)
}

/// Prepended context documents (pinned specs, the plan) become the head of the prompt, in markdown.
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

fn tool_title(name: &str, input: Option<&Value>) -> String {
    if let Some(input) = input {
        for key in ["command", "file_path", "description"] {
            if let Some(value) = input.get(key).and_then(Value::as_str) {
                let value = value.trim();
                if !value.is_empty() {
                    return truncate(value, 200);
                }
            }
        }
    }
    name.to_string()
}

fn summarize(content: Option<&Value>) -> String {
    let raw = match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    truncate(&raw, SUMMARY_LIMIT)
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(limit).collect();
    truncated.push('…');
    truncated
}

fn is_subagent(value: &Value) -> bool {
    // Subagent turns are interleaved into the same stream; only the main transcript belongs here.
    value
        .get("parent_tool_use_id")
        .map(|parent| !parent.is_null())
        .unwrap_or(false)
}

async fn kill_process_tree(pid: Option<u32>, child: &mut Child) {
    if let Some(pid) = pid {
        kill_tree_by_pid(pid).await;
    }
    let _ = child.kill().await;
    let _ = tokio::time::timeout(REAP_TIMEOUT, child.wait()).await;
}

#[cfg(windows)]
async fn kill_tree_by_pid(pid: u32) {
    // `Child::kill` reaches the direct child only, and `claude` spawns tools of its own. A failure is
    // fine: the direct kill below still runs.
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
    // The child is spawned as a process-group leader (see `Runner::turn`), so the negative pid is the
    // whole tree.
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
    fn launch_args_carry_the_documented_headless_flags() {
        let args = launch_args(
            Some("claude-sonnet-4"),
            Some("abc123"),
            &strings(&["--extra"]),
        );
        assert_eq!(
            args,
            strings(&[
                "--extra",
                "--print",
                "--output-format",
                "stream-json",
                "--verbose",
                "--model",
                "claude-sonnet-4",
                "--resume",
                "abc123",
            ])
        );
    }

    #[test]
    fn launch_args_omit_model_and_resume_when_absent() {
        assert_eq!(
            launch_args(None, None, &[]),
            strings(&["--print", "--output-format", "stream-json", "--verbose"])
        );
    }

    #[test]
    fn config_overrides_the_command_and_extends_the_args() {
        let config = serde_json::json!({
            "command": "node",
            "args": ["fixtures/harnesses/claude_fake.mjs", "--mode", "happy", 42],
        });
        let (command, args) = command_from_config(&config, "claude");
        assert_eq!(command, "node");
        assert_eq!(
            args,
            strings(&["fixtures/harnesses/claude_fake.mjs", "--mode", "happy"]),
            "non-string args are ignored rather than fatal"
        );

        let (command, args) = command_from_config(&serde_json::json!({}), "claude");
        assert_eq!(command, "claude");
        assert!(args.is_empty());
    }

    #[test]
    fn context_documents_are_prepended_to_the_prompt() {
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
        let rendered = render_prompt(Some("ship ticket 08"), &documents).expect("a prompt");
        assert!(rendered.starts_with("# Context\n\n"));
        assert!(rendered.contains("## Grove — architecture"));
        assert!(rendered.contains("grove-architecture"));
        assert!(rendered.contains("- step one"));
        assert!(rendered.contains("---\n\nship ticket 08"));

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
    fn tool_titles_prefer_the_command_then_the_path_then_the_description() {
        assert_eq!(
            tool_title(
                "Bash",
                Some(&serde_json::json!({ "command": "cargo test", "description": "run tests" }))
            ),
            "cargo test"
        );
        assert_eq!(
            tool_title(
                "Read",
                Some(&serde_json::json!({ "file_path": "src/main.rs" }))
            ),
            "src/main.rs"
        );
        assert_eq!(
            tool_title(
                "Task",
                Some(&serde_json::json!({ "description": "explain the repo" }))
            ),
            "explain the repo"
        );
        assert_eq!(tool_title("Bash", Some(&serde_json::json!({}))), "Bash");
        assert_eq!(tool_title("Bash", None), "Bash");
    }

    #[test]
    fn tool_results_are_summarised_and_truncated() {
        assert_eq!(summarize(Some(&serde_json::json!("done"))), "done");
        assert_eq!(
            summarize(Some(&serde_json::json!([
                { "type": "text", "text": "line one" },
                { "type": "image", "source": {} },
                { "type": "text", "text": "line two" },
            ]))),
            "line one\nline two"
        );
        let long = "x".repeat(SUMMARY_LIMIT + 10);
        let summary = summarize(Some(&serde_json::json!(long)));
        assert_eq!(summary.chars().count(), SUMMARY_LIMIT + 1);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn capabilities_are_honest() {
        let capabilities = ClaudeAdapter::default().capabilities();
        assert!(capabilities.streaming_deltas);
        assert!(capabilities.tool_calls);
        assert!(capabilities.resume);
        assert!(capabilities.usage);
        assert!(
            !capabilities.permissions,
            "one-shot -p has no permission channel"
        );
        assert!(!capabilities.diffs, "no patches are synthesized yet");
        assert!(!capabilities.images);
        assert!(!capabilities.mcp);
    }

    fn deps() -> AdapterDeps {
        AdapterDeps {
            paths: crate::paths::AppPaths::new(std::env::temp_dir().join("grove-claude-tests")),
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
        let installed = ClaudeAdapter::with_command(binary).detect(&deps());
        assert!(installed.installed);
        assert!(installed.path.is_some());
        assert!(
            installed.version.is_none(),
            "detection never runs the binary"
        );

        let missing =
            ClaudeAdapter::with_command("grove-definitely-not-a-real-binary").detect(&deps());
        assert!(!missing.installed);
        assert!(missing
            .detail
            .unwrap()
            .contains("grove-definitely-not-a-real-binary"));
    }
}
