//! The OpenCode adapter: `opencode serve` over loopback HTTP plus its `/event` SSE bus.
//!
//! OpenCode is the first harness Grove talks to over the network instead of a stdio pipe. The server
//! is local (`127.0.0.1`) and so is every request: a session never leaves the machine, and a URL that
//! is not loopback is refused before anything is spawned. There are two launch shapes, both
//! first-class:
//!
//! - **spawn (default)** — pick a free loopback port, run
//!   `opencode serve --port <free> --hostname 127.0.0.1` in `StartContext.cwd`, and own the child for
//!   the life of the session (it is killed on shutdown, so it can never be an orphan).
//! - **attach** — `config.baseUrl` points at a server the user already runs (the human equivalent is
//!   `opencode attach <url>`). Grove never starts or stops that server, and the URL is part of the
//!   session: it is stored with the session's adapter config and reused on resume.
//!
//! Readiness is `GET /global/health`, polled until it answers or the timeout expires — never a guessed
//! port banner on stdout. A failure carries the captured stderr tail, because "the server did not
//! come up" is useless without the line that explains why.
//!
//! # Mapping
//!
//! One SSE frame per line on `GET /event`; `{type, properties}` as documented for the v1 server.
//!
//! | OpenCode frame | Grove event |
//! |---|---|
//! | `/global/health` + `POST /session` (or the resumed session) | `SessionStarted { harness: "opencode", resumed }`, `Status::Ready` |
//! | `session.status` `busy` | `Status::Busy` |
//! | `session.status` `idle` (or the older `session.idle`) | `Status::Idle` — the turn ends |
//! | `session.status` `retry` | `Error { retryable: true }` then `Status::Busy` |
//! | `message.part.updated` part `text` with `properties.delta` | `Delta { text: delta }` |
//! | `message.part.updated` part `text` without a delta | `Message { assistant, [Text] }` |
//! | `message.part.updated` part `reasoning` (final update) | `Message { assistant, [Thinking] }` |
//! | `message.part.updated` part `tool`, state `pending`/`running` | `ToolCall { call_id, name: tool, title }` (once per call) |
//! | `message.part.updated` part `tool`, state `completed` | `ToolResult { status: ok, summary: output }`, plus `Diff` for an edit tool whose metadata carries a patch |
//! | `message.part.updated` part `tool`, state `error` | `ToolResult { status: error, summary: error }` |
//! | `message.part.updated` part `step-finish` `tokens` | `Usage { input_tokens, output_tokens }` |
//! | `session.diff` (`FileDiff[]`) | one `Diff` per entry (its `patch`, or one built from `before`/`after`) |
//! | `permission.updated` | `Error { retryable: false }` explaining that this adapter cannot answer a prompt (see below) |
//! | `session.error` | `Error { retryable: false }` (a `MessageAbortedError` is a cancel, not a failure) |
//! | `message.updated` | role bookkeeping only, so a user's own message part is never rendered as an assistant turn |
//! | unknown type, malformed payload, another session's frame, `todo.updated`, `vcs.*`, `lsp.*`, `pty.*`, `tui.*`, `installation.*` | logged and dropped |
//! | `/event` dropped while the session is alive | reconnect with exponential backoff; after `config.reconnectAttempts` the failure is permanent → `Error { retryable: false }` + `Status::Exited` |
//!
//! # Deliberate limits
//!
//! - **No interactive permissions.** The `POST /session/{id}/permissions/{id}` body's `response` enum
//!   is unconfirmed in the published protocol, and the adapter would need to hold a turn open for an
//!   answer Grove's UI does not mediate yet. `capabilities.permissions` is therefore `false` and a
//!   permission prompt is reported as a non-retryable error instead of being silently dropped.
//! - **No transcript backfill.** `GET /session/{id}/message` exists, but the resume story here is "the
//!   server still has the session": the adapter probes `GET /session/{id}` and adopts it. If the
//!   server has forgotten it, a new conversation is created and the user is told.
//! - **One generation of the API.** The documented v1 REST surface (`/session`, `/event`,
//!   `/global/health`) is used; the volatile `/api/...` v2 surface is not mixed in.
//! - **A turn that never starts.** The v1 surface has a known bug where `prompt_async` answers 204
//!   and never starts the model, so no `busy` and no `idle` ever arrive. The adapter does not invent a
//!   turn timeout (a slow provider is not a dead one); cancel does, because its grace timer always
//!   returns the session to `idle`.
//!
//! # Config
//!
//! `config.command` / `config.args` replace the server binary (that is how tests point at
//! `fixtures/harnesses/opencode_fake.mjs`), `config.baseUrl` selects attach mode, `config.providerID`
//! pairs with a bare `StartContext.model` (`provider/model` needs no help), and
//! `readyTimeoutMs` / `healthIntervalMs` / `reconnectDelayMs` / `reconnectAttempts` tune the timing.

use super::{
    AdapterDeps, AdapterError, Capabilities, ContextDoc, Detection, HarnessAdapter, HarnessId,
    HarnessSession, ResumeToken, StartContext, UserInput,
};
use crate::sessions::event::{
    Block, Decision, EventKind, HarnessEvent, Role, SessionState, ToolStatus,
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant;

/// The harness's own name, as written into `SessionStarted` and stored on the session row.
const HARNESS: &str = "opencode";
/// The binary detection looks for. `config.command` can still override it per session.
const DEFAULT_COMMAND: &str = "opencode";
/// A session never leaves the machine: there is exactly one host Grove will talk to.
const LOOPBACK_HOST: &str = "127.0.0.1";
const EVENT_BUFFER: usize = 1024;
const SSE_BUFFER: usize = 256;
const MAX_SSE_BYTES: usize = 1 << 20;
const STDERR_LINES: usize = 40;
const SUMMARY_LIMIT: usize = 500;
const PATCH_LIMIT: usize = 20_000;
const DEFAULT_READY_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_HEALTH_INTERVAL_MS: u64 = 50;
const DEFAULT_RECONNECT_DELAY_MS: u64 = 100;
const MAX_RECONNECT_DELAY_MS: u64 = 2_000;
const DEFAULT_RECONNECT_ATTEMPTS: u32 = 5;
const HEALTH_TIMEOUT: Duration = Duration::from_millis(1_000);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const CANCEL_GRACE: Duration = Duration::from_secs(5);
const REAP_TIMEOUT: Duration = Duration::from_secs(5);

pub struct OpenCodeAdapter {
    /// The binary detection looks for. `config.command` can still override it per session.
    command: String,
    /// A server to attach to instead of spawning one (`config.baseUrl` overrides it per session).
    base_url: Option<String>,
}

impl Default for OpenCodeAdapter {
    fn default() -> Self {
        Self {
            command: DEFAULT_COMMAND.to_string(),
            base_url: None,
        }
    }
}

impl OpenCodeAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A different binary name for detection (a wrapper, a versioned install, a test double).
    pub fn with_command(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            base_url: None,
        }
    }

    /// A server this adapter always attaches to, so the doctor can report it.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            command: DEFAULT_COMMAND.to_string(),
            base_url: Some(base_url.into()),
        }
    }
}

#[async_trait::async_trait]
impl HarnessAdapter for OpenCodeAdapter {
    fn id(&self) -> HarnessId {
        HarnessId(HARNESS)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // `properties.delta` on a text part is streamed as it arrives.
            streaming_deltas: true,
            tool_calls: true,
            // An edit tool's patch and `session.diff` entries both become `Diff` events.
            diffs: true,
            // See the module docs: the permission reply enum is unconfirmed, so it is not claimed.
            permissions: false,
            // The OpenCode session id is the resume token.
            resume: true,
            images: false,
            mcp: false,
            // `step-finish` parts carry token counts.
            usage: true,
        }
    }

    /// Side-effect free: resolves the binary on `PATH` but never runs it, and never probes a server.
    /// A configured attach URL is reported instead, because attaching needs no local binary at all.
    fn detect(&self, _deps: &AdapterDeps) -> Detection {
        let binary = find_on_path(&self.command);
        let attach = self
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty());
        match (binary, attach) {
            (Some(path), Some(url)) => Detection {
                installed: true,
                version: None,
                path: Some(crate::paths::display_path(&path)),
                detail: Some(format!(
                    "attaches to {url}; the server is probed when a session starts"
                )),
            },
            (Some(path), None) => Detection {
                installed: true,
                version: None,
                path: Some(crate::paths::display_path(&path)),
                detail: None,
            },
            (None, Some(url)) => Detection {
                installed: true,
                version: None,
                path: None,
                detail: Some(format!(
                    "attaches to {url} (no `{}` binary needed)",
                    self.command
                )),
            },
            (None, None) => Detection::missing(format!(
                "`{}` is not on PATH; install OpenCode or set config.command, or set config.baseUrl \
                 to attach to a running server",
                self.command
            )),
        }
    }

    async fn start(
        &self,
        ctx: StartContext,
        _deps: AdapterDeps,
    ) -> Result<Box<dyn HarnessSession>, AdapterError> {
        let config = AdapterConfig::from_value(&ctx.config, self);
        let (events, events_rx) = mpsc::channel(EVENT_BUFFER);
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let token: ResumeToken = Arc::new(Mutex::new(ctx.resume_token.clone()));
        let initial_prompt = render_prompt(ctx.prompt.as_deref(), &ctx.context_files);
        let actor = Actor {
            events,
            token: Arc::clone(&token),
            http: http_client(),
            config,
            cwd: ctx.cwd,
            model: ctx.model,
            initial_prompt,
            base_url: String::new(),
            session_id: ctx.resume_token.clone(),
            server: None,
            stderr: Arc::new(Mutex::new(VecDeque::new())),
            stdout_task: None,
            stderr_task: None,
            reader: None,
            sse: None,
            alive: Arc::new(AtomicBool::new(true)),
            state: None,
            inflight: false,
            cancel_deadline: None,
            closed: false,
            pending: VecDeque::new(),
            seen_calls: HashSet::new(),
            answered_calls: HashSet::new(),
            roles: HashMap::new(),
        };
        tokio::spawn(run(actor, commands_rx));

        Ok(Box::new(OpenCodeSession {
            events: Some(events_rx),
            commands,
            token,
        }))
    }
}

enum TurnCommand {
    Send(String),
    Cancel,
    Shutdown { ack: Option<oneshot::Sender<()>> },
}

pub struct OpenCodeSession {
    events: Option<mpsc::Receiver<HarnessEvent>>,
    commands: mpsc::UnboundedSender<TurnCommand>,
    token: ResumeToken,
}

impl OpenCodeSession {
    fn send_command(&self, command: TurnCommand) -> Result<(), AdapterError> {
        self.commands
            .send(command)
            .map_err(|_| AdapterError::Other("the opencode session has ended".into()))
    }
}

#[async_trait::async_trait]
impl HarnessSession for OpenCodeSession {
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
            "this adapter does not answer opencode permission prompts; approve them in the opencode \
             TUI or configure the agent to run without prompts"
                .into(),
        ))
    }

    async fn cancel(&mut self) -> Result<(), AdapterError> {
        self.send_command(TurnCommand::Cancel)
    }

    async fn shutdown(&mut self) -> Result<(), AdapterError> {
        // Wait for the actor to kill a server it spawned: shutdown is the "no orphans" promise, and
        // the manager owns the final `exited` transition.
        let (ack, received) = oneshot::channel();
        if self
            .commands
            .send(TurnCommand::Shutdown { ack: Some(ack) })
            .is_ok()
        {
            let _ = tokio::time::timeout(REAP_TIMEOUT, received).await;
        }
        Ok(())
    }

    fn resume_token(&self) -> Option<String> {
        self.token.lock().ok().and_then(|token| token.clone())
    }

    fn resume_token_slot(&self) -> Option<ResumeToken> {
        Some(Arc::clone(&self.token))
    }
}

/// Knobs read from `config`, with the defaults a real install wants.
struct AdapterConfig {
    base_url: Option<String>,
    command: String,
    args: Vec<String>,
    ready_timeout: Duration,
    health_interval: Duration,
    reconnect_base: Duration,
    reconnect_attempts: u32,
    provider_id: Option<String>,
}

impl AdapterConfig {
    fn from_value(config: &Value, adapter: &OpenCodeAdapter) -> Self {
        let base_url = config
            .get("baseUrl")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(str::to_string)
            .or_else(|| adapter.base_url.clone());
        let command = config
            .get("command")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .unwrap_or(&adapter.command)
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
        Self {
            base_url,
            command,
            args,
            ready_timeout: millis(config, "readyTimeoutMs", DEFAULT_READY_TIMEOUT_MS),
            health_interval: millis(config, "healthIntervalMs", DEFAULT_HEALTH_INTERVAL_MS),
            reconnect_base: millis(config, "reconnectDelayMs", DEFAULT_RECONNECT_DELAY_MS),
            reconnect_attempts: config
                .get("reconnectAttempts")
                .and_then(Value::as_u64)
                .map(|attempts| attempts.min(50) as u32)
                .unwrap_or(DEFAULT_RECONNECT_ATTEMPTS),
            provider_id: config
                .get("providerID")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|provider| !provider.is_empty())
                .map(str::to_string),
        }
    }
}

fn millis(config: &Value, key: &str, default: u64) -> Duration {
    Duration::from_millis(
        config
            .get(key)
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .unwrap_or(default),
    )
}

/// One live conversation with an OpenCode server. The actor owns the child (when Grove spawned it),
/// the session id and the SSE reader; commands arrive on one channel and events leave on another.
struct Actor {
    events: mpsc::Sender<HarnessEvent>,
    token: ResumeToken,
    http: reqwest::Client,
    config: AdapterConfig,
    cwd: PathBuf,
    model: Option<String>,
    initial_prompt: Option<String>,
    base_url: String,
    session_id: Option<String>,
    server: Option<Child>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    sse: Option<mpsc::Receiver<Sse>>,
    alive: Arc<AtomicBool>,
    /// The last status written to the transcript, so a repeated `busy` is not repeated in the log.
    state: Option<SessionState>,
    inflight: bool,
    cancel_deadline: Option<Instant>,
    closed: bool,
    pending: VecDeque<String>,
    seen_calls: HashSet<String>,
    answered_calls: HashSet<String>,
    /// `messageID` → role, so a user's own message part never becomes an assistant turn.
    roles: HashMap<String, String>,
}

impl Actor {
    async fn emit(&self, kind: EventKind) {
        let _ = self.events.send(HarnessEvent::new(kind)).await;
    }

    async fn set_state(&mut self, state: SessionState) {
        if self.state == Some(state) {
            return;
        }
        self.state = Some(state);
        self.emit(EventKind::Status { state }).await;
    }

    fn adopt_session(&mut self, id: String) {
        self.session_id = Some(id.clone());
        if let Ok(mut token) = self.token.lock() {
            *token = Some(id);
        }
    }

    fn stderr_suffix(&self) -> String {
        let tail = tail_text(&self.stderr);
        if tail.trim().is_empty() {
            String::new()
        } else {
            format!("; stderr: {}", truncate(&tail, 600))
        }
    }

    /// Bring up the server (or attach), the session, and the event stream. Any failure carries a
    /// reason a human can act on.
    async fn bring_up(&mut self) -> Result<(), String> {
        match self.config.base_url.clone() {
            Some(raw) => {
                self.base_url = loopback_base_url(&raw)?;
                self.await_health().await?;
            }
            None => {
                self.spawn_server().await?;
                self.await_health().await?;
            }
        }
        self.resolve_session().await?;
        self.start_reader();
        self.await_event_stream().await
    }

    /// Poll `GET /global/health` until it answers, the child dies, or the timeout expires.
    async fn await_health(&mut self) -> Result<(), String> {
        let url = format!("{}/global/health", self.base_url);
        let deadline = Instant::now() + self.config.ready_timeout;
        loop {
            let last_error = match self.http.get(&url).timeout(HEALTH_TIMEOUT).send().await {
                Ok(response) if response.status().is_success() => {
                    let body = response.text().await.unwrap_or_default();
                    match serde_json::from_str::<Value>(&body) {
                        Ok(value)
                            if value.get("healthy").and_then(Value::as_bool) == Some(false) =>
                        {
                            format!("the server reported unhealthy: {}", truncate(&body, 160))
                        }
                        // A 2xx health answer is readiness. A body this adapter cannot parse is
                        // protocol drift, not a failed start.
                        _ => return Ok(()),
                    }
                }
                Ok(response) => format!("HTTP {}", response.status()),
                Err(error) => format!("{error}"),
            };
            // A child that gave up explains the failure better than the timeout would.
            let last_error = match self.server.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        return Err(format!(
                            "`{} serve` exited with {status} before it became healthy{}",
                            self.config.command,
                            self.stderr_suffix()
                        ));
                    }
                    Ok(None) => last_error,
                    Err(error) => format!("waiting for the server failed: {error}"),
                },
                None => last_error,
            };
            if Instant::now() >= deadline {
                return Err(format!(
                    "the opencode server at {} did not become healthy within {}ms ({last_error}){}",
                    self.base_url,
                    self.config.ready_timeout.as_millis(),
                    self.stderr_suffix()
                ));
            }
            tokio::time::sleep(self.config.health_interval).await;
        }
    }

    async fn spawn_server(&mut self) -> Result<(), String> {
        let port = free_loopback_port()?;
        let mut command = Command::new(&self.config.command);
        command
            .args(&self.config.args)
            .args([
                "serve",
                "--port",
                &port.to_string(),
                "--hostname",
                LOOPBACK_HOST,
            ])
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // A dropped session must not leak a server.
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn().map_err(|error| {
            format!("could not launch `{} serve`: {error}", self.config.command)
        })?;

        if let Some(stdout) = child.stdout.take() {
            self.stdout_task = Some(tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "grove::adapters::opencode", "stdout: {line}");
                }
            }));
        }
        if let Some(stderr) = child.stderr.take() {
            let tail = Arc::clone(&self.stderr);
            self.stderr_task = Some(tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "grove::adapters::opencode", "stderr: {line}");
                    append_tail(&tail, &line);
                }
            }));
        }

        self.server = Some(child);
        self.base_url = format!("http://{LOOPBACK_HOST}:{port}");
        Ok(())
    }

    /// Reuse the conversation the resume token names, or create one. The token is the OpenCode
    /// session id, and its state lives in the server, so a probe is the only honest way to know
    /// whether a restart can continue the same conversation.
    async fn resolve_session(&mut self) -> Result<(), String> {
        let existing = self.session_id.clone().filter(|id| !id.trim().is_empty());
        let Some(existing) = existing else {
            let created = self.create_session().await?;
            self.adopt_session(created);
            return Ok(());
        };

        let url = format!("{}/session/{}", self.base_url, existing);
        match self.http.get(&url).timeout(HTTP_TIMEOUT).send().await {
            Ok(response) if response.status().is_success() => {
                self.adopt_session(existing);
                Ok(())
            }
            Ok(response) if response.status() == reqwest::StatusCode::NOT_FOUND => {
                let created = self.create_session().await?;
                self.adopt_session(created.clone());
                self.emit(EventKind::Error {
                    message: format!(
                        "the opencode session {existing} no longer exists on {}; started a new \
                         conversation ({created})",
                        self.base_url
                    ),
                    retryable: false,
                })
                .await;
                Ok(())
            }
            Ok(response) => Err(format!(
                "GET /session/{existing} returned HTTP {}",
                response.status()
            )),
            Err(error) => Err(format!(
                "could not reach the opencode session {existing}: {error}"
            )),
        }
    }

    async fn create_session(&mut self) -> Result<String, String> {
        let url = format!("{}/session", self.base_url);
        let response = self
            .http
            .post(&url)
            .json(&json!({ "parentID": null, "title": "Grove session" }))
            .timeout(HTTP_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("could not create an opencode session: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            let detail = truncate(&response.text().await.unwrap_or_default(), 200);
            return Err(format!("POST /session returned HTTP {status}: {detail}"));
        }
        let value: Value = response
            .json()
            .await
            .map_err(|error| format!("POST /session returned invalid JSON: {error}"))?;
        value
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                format!(
                    "POST /session returned no session id: {}",
                    truncate(&value.to_string(), 200)
                )
            })
    }

    fn start_reader(&mut self) {
        let (sse, sse_rx) = mpsc::channel(SSE_BUFFER);
        let http = self.http.clone();
        let url = format!("{}/event", self.base_url);
        let alive = Arc::clone(&self.alive);
        let reconnect = Reconnect {
            base: self.config.reconnect_base,
            attempts: self.config.reconnect_attempts,
        };
        self.reader = Some(tokio::spawn(stream_events(
            http, url, sse, alive, reconnect,
        )));
        self.sse = Some(sse_rx);
    }

    async fn await_event_stream(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + self.config.ready_timeout;
        loop {
            match tokio::time::timeout_at(deadline, next_sse(&mut self.sse)).await {
                Err(_) => {
                    return Err(format!(
                        "the event stream at {}/event did not connect within {}ms{}",
                        self.base_url,
                        self.config.ready_timeout.as_millis(),
                        self.stderr_suffix()
                    ));
                }
                Ok(None) => {
                    return Err(format!(
                        "the opencode event stream closed before the session was ready{}",
                        self.stderr_suffix()
                    ));
                }
                Ok(Some(Sse::Connected)) => return Ok(()),
                Ok(Some(Sse::GaveUp { reason })) => return Err(reason),
                // Frame drift before `ready` is not content: it is dropped, exactly like an unknown
                // frame later in the turn.
                Ok(Some(Sse::Event(_))) | Ok(Some(Sse::Retry { .. })) => continue,
            }
        }
    }

    /// The main loop: dispatch queued prompts, then wait for a command, an event, or the cancel grace
    /// timer.
    async fn drive(&mut self, commands: &mut mpsc::UnboundedReceiver<TurnCommand>) {
        enum Wait {
            Command(Option<TurnCommand>),
            Sse(Option<Sse>),
            Deadline,
        }
        loop {
            while let Some(prompt) = self.pending.pop_front() {
                if self.closed {
                    self.pending.clear();
                    break;
                }
                self.send_prompt(prompt).await;
            }
            let deadline = self.cancel_deadline;
            let wait = {
                let sse = &mut self.sse;
                tokio::select! {
                    command = commands.recv() => Wait::Command(command),
                    message = next_sse(sse) => Wait::Sse(message),
                    _ = sleep_until(deadline), if deadline.is_some() => Wait::Deadline,
                }
            };
            match wait {
                Wait::Command(Some(TurnCommand::Send(text))) => {
                    if self.closed {
                        tracing::debug!(
                            target: "grove::adapters::opencode",
                            "dropping a prompt: the session is no longer connected"
                        );
                    } else {
                        self.pending.push_back(text);
                    }
                }
                Wait::Command(Some(TurnCommand::Cancel)) => self.cancel().await,
                Wait::Command(Some(TurnCommand::Shutdown { ack })) => {
                    self.shutdown().await;
                    if let Some(ack) = ack {
                        let _ = ack.send(());
                    }
                    return;
                }
                Wait::Command(None) => {
                    self.shutdown().await;
                    return;
                }
                Wait::Sse(Some(Sse::Event(value))) => self.handle_event(value).await,
                Wait::Sse(Some(Sse::Retry { attempt, reason })) => tracing::debug!(
                    target: "grove::adapters::opencode",
                    "event stream disconnected ({reason}); reconnect attempt {attempt} of {}",
                    self.config.reconnect_attempts
                ),
                Wait::Sse(Some(Sse::GaveUp { reason })) => self.permanent_failure(reason).await,
                Wait::Sse(Some(Sse::Connected)) => tracing::debug!(
                    target: "grove::adapters::opencode",
                    "event stream connected to {}/event",
                    self.base_url
                ),
                Wait::Sse(None) => self.sse = None,
                Wait::Deadline => self.finish_turn().await,
            }
        }
    }

    async fn finish_turn(&mut self) {
        self.inflight = false;
        self.cancel_deadline = None;
        self.set_state(SessionState::Idle).await;
    }

    async fn permanent_failure(&mut self, reason: String) {
        self.sse = None;
        self.inflight = false;
        self.cancel_deadline = None;
        self.emit(EventKind::Error {
            message: format!("the opencode event stream failed permanently: {reason}"),
            retryable: false,
        })
        .await;
        self.closed = true;
        self.alive.store(false, Ordering::SeqCst);
        // A server Grove spawned is useless without its stream; reaping it keeps the promise that a
        // session leaves no process behind.
        self.stop_server().await;
        self.set_state(SessionState::Exited).await;
    }

    async fn send_prompt(&mut self, text: String) {
        let Some(id) = self.session_id.clone() else {
            tracing::debug!(
                target: "grove::adapters::opencode",
                "dropping a prompt: no opencode session id"
            );
            return;
        };
        let mut body = json!({ "parts": [{ "type": "text", "text": text }] });
        if let Some(model) = model_object(self.model.as_deref(), self.config.provider_id.as_deref())
        {
            body["model"] = model;
        }

        let url = format!("{}/session/{}/prompt_async", self.base_url, id);
        match self
            .http
            .post(&url)
            .json(&body)
            .timeout(HTTP_TIMEOUT)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                self.inflight = true;
                self.cancel_deadline = None;
            }
            Ok(response) => {
                let status = response.status();
                let detail = truncate(&response.text().await.unwrap_or_default(), 300);
                self.emit(EventKind::Error {
                    message: format!("opencode rejected the prompt (HTTP {status}): {detail}"),
                    retryable: !status.is_client_error(),
                })
                .await;
                self.set_state(SessionState::Idle).await;
            }
            Err(error) => {
                self.emit(EventKind::Error {
                    message: format!("could not send the prompt to the opencode server: {error}"),
                    retryable: true,
                })
                .await;
                self.set_state(SessionState::Idle).await;
            }
        }
    }

    async fn cancel(&mut self) {
        if self.closed {
            // The session already ended (a permanent failure). A cancel must not resurrect it as
            // `idle` and contradict the `exited` the transcript recorded.
            return;
        }
        if !self.inflight {
            // Symmetric with the other adapters: cancelling an idle session confirms the idle state.
            self.set_state(SessionState::Idle).await;
            return;
        }
        let Some(id) = self.session_id.clone() else {
            return;
        };
        let url = format!("{}/session/{}/abort", self.base_url, id);
        let outcome = match self.http.post(&url).timeout(HTTP_TIMEOUT).send().await {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => Err(format!(
                "opencode refused the abort (HTTP {})",
                response.status()
            )),
            Err(error) => Err(format!("could not abort the opencode turn: {error}")),
        };
        if let Err(message) = outcome {
            self.emit(EventKind::Error {
                message,
                retryable: true,
            })
            .await;
        }
        // The server reports the idle transition. The grace timer only covers a server that never
        // does, so a cancel can never leave the session stuck in `busy`.
        self.cancel_deadline = Some(Instant::now() + CANCEL_GRACE);
    }

    async fn shutdown(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Some(reader) = self.reader.take() {
            reader.abort();
        }
        self.sse = None;
        self.stop_server().await;
        if let Some(task) = self.stdout_task.take() {
            task.abort();
        }
        if let Some(task) = self.stderr_task.take() {
            task.abort();
        }
    }

    async fn stop_server(&mut self) {
        let Some(mut child) = self.server.take() else {
            return;
        };
        if let Some(pid) = child.id() {
            kill_tree_by_pid(pid).await;
        }
        let _ = child.kill().await;
        let _ = tokio::time::timeout(REAP_TIMEOUT, child.wait()).await;
    }

    async fn handle_event(&mut self, value: Value) {
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let properties = value.get("properties").cloned().unwrap_or(Value::Null);
        match event_type.as_str() {
            "server.connected" => tracing::debug!(
                target: "grove::adapters::opencode",
                "the server accepted the event stream"
            ),
            "session.status" => {
                if !self.is_our_session(&properties) {
                    return;
                }
                let status = properties.get("status").cloned().unwrap_or(Value::Null);
                match status
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                {
                    "busy" => {
                        self.inflight = true;
                        self.set_state(SessionState::Busy).await;
                    }
                    "idle" => self.finish_turn().await,
                    "retry" => {
                        let detail = status
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("opencode is retrying the request");
                        let attempt = status.get("attempt").and_then(Value::as_u64).unwrap_or(0);
                        self.emit(EventKind::Error {
                            message: if attempt > 0 {
                                format!("opencode is retrying (attempt {attempt}): {detail}")
                            } else {
                                format!("opencode is retrying: {detail}")
                            },
                            retryable: true,
                        })
                        .await;
                        self.inflight = true;
                        self.set_state(SessionState::Busy).await;
                    }
                    other => tracing::debug!(
                        target: "grove::adapters::opencode",
                        "skipping an unknown session status: {other}"
                    ),
                }
            }
            "session.idle" => {
                if self.is_our_session(&properties) {
                    self.finish_turn().await;
                }
            }
            "message.updated" => {
                let info = properties.get("info").cloned().unwrap_or(Value::Null);
                if !self.is_our_session(&info) {
                    return;
                }
                if let (Some(id), Some(role)) = (
                    info.get("id").and_then(Value::as_str),
                    info.get("role").and_then(Value::as_str),
                ) {
                    self.roles.insert(id.to_string(), role.to_string());
                }
            }
            "message.part.updated" => {
                if !self.is_our_session(&properties) {
                    return;
                }
                let Some(part) = properties.get("part").cloned() else {
                    tracing::debug!(
                        target: "grove::adapters::opencode",
                        "skipping a part update without a part"
                    );
                    return;
                };
                let delta = properties
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.handle_part(part, delta).await;
            }
            "session.diff" => self.handle_session_diff(&properties).await,
            "session.error" => {
                if !self.is_our_session(&properties) {
                    return;
                }
                let error = properties.get("error").cloned().unwrap_or(Value::Null);
                let name = error.get("name").and_then(Value::as_str).unwrap_or("Error");
                if name == "MessageAbortedError" {
                    // A cancelled turn is not a failure.
                    return;
                }
                let detail = error
                    .get("data")
                    .and_then(|data| data.get("message"))
                    .and_then(Value::as_str)
                    .or_else(|| error.get("message").and_then(Value::as_str))
                    .unwrap_or_default();
                let message = if detail.is_empty() {
                    format!("opencode reported {name}")
                } else {
                    format!("opencode reported {name}: {}", truncate(detail, 300))
                };
                self.emit(EventKind::Error {
                    message,
                    retryable: false,
                })
                .await;
            }
            "permission.updated" => {
                if !self.is_our_session(&properties) {
                    return;
                }
                let title = properties
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("a tool call");
                let kind = properties.get("type").and_then(Value::as_str).unwrap_or("");
                self.emit(EventKind::Error {
                    message: format!(
                        "opencode asked for permission to run {title}{}; this adapter cannot answer \
                         permission prompts — approve it in the opencode TUI",
                        if kind.is_empty() {
                            String::new()
                        } else {
                            format!(" ({kind})")
                        }
                    ),
                    retryable: false,
                })
                .await;
            }
            other => tracing::debug!(
                target: "grove::adapters::opencode",
                "skipping an event type this adapter does not use: {other}"
            ),
        }
    }

    async fn handle_part(&mut self, part: Value, delta: String) {
        let part_type = part
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        match part_type.as_str() {
            "text" => {
                if self.is_user_part(&part) {
                    return;
                }
                if !delta.is_empty() {
                    self.emit(EventKind::Delta { text: delta }).await;
                    return;
                }
                if let Some(text) = part
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                {
                    self.emit(EventKind::Message {
                        role: Role::Assistant,
                        blocks: vec![Block::Text {
                            text: text.to_string(),
                        }],
                    })
                    .await;
                }
            }
            "reasoning" => {
                // Thinking is never streamed as a delta; the completed part carries the text.
                if !delta.is_empty() {
                    return;
                }
                if let Some(text) = part
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                {
                    self.emit(EventKind::Message {
                        role: Role::Assistant,
                        blocks: vec![Block::Thinking {
                            text: text.to_string(),
                        }],
                    })
                    .await;
                }
            }
            "tool" => self.handle_tool(&part).await,
            "step-finish" => {
                if let Some(tokens) = part.get("tokens") {
                    self.emit(EventKind::Usage {
                        input_tokens: tokens.get("input").and_then(Value::as_u64).unwrap_or(0),
                        output_tokens: tokens.get("output").and_then(Value::as_u64).unwrap_or(0),
                    })
                    .await;
                }
            }
            other => tracing::debug!(
                target: "grove::adapters::opencode",
                "skipping a part type this adapter does not use: {other}"
            ),
        }
    }

    async fn handle_tool(&mut self, part: &Value) {
        let call_id = part
            .get("callID")
            .or_else(|| part.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if call_id.is_empty() {
            tracing::debug!(
                target: "grove::adapters::opencode",
                "skipping a tool part without a call id"
            );
            return;
        }
        let name = part
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or("tool")
            .to_string();
        let state = part.get("state").cloned().unwrap_or(Value::Null);
        let status = state
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let title = state
            .get("title")
            .and_then(Value::as_str)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| name.clone());

        match status.as_str() {
            "pending" | "running" => {
                if self.seen_calls.insert(call_id.clone()) {
                    self.emit(EventKind::ToolCall {
                        call_id,
                        name,
                        title,
                    })
                    .await;
                }
            }
            "completed" | "error" => {
                // A server may skip the running state, so the call is recorded here too.
                if self.seen_calls.insert(call_id.clone()) {
                    self.emit(EventKind::ToolCall {
                        call_id: call_id.clone(),
                        name: name.clone(),
                        title: title.clone(),
                    })
                    .await;
                }
                if !self.answered_calls.insert(call_id.clone()) {
                    return;
                }
                let (result_status, summary) = if status == "error" {
                    let summary = state
                        .get("error")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| title.clone());
                    (ToolStatus::Error, summary)
                } else {
                    let summary = state
                        .get("output")
                        .and_then(Value::as_str)
                        .filter(|output| !output.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| title.clone());
                    (ToolStatus::Ok, summary)
                };
                self.emit(EventKind::ToolResult {
                    call_id: call_id.clone(),
                    status: result_status,
                    summary: truncate(&summary, SUMMARY_LIMIT),
                })
                .await;

                if result_status == ToolStatus::Ok && is_edit_tool(&name) {
                    if let Some((path, patch, added, removed)) = tool_diff(part, &state) {
                        self.emit(EventKind::Diff {
                            path,
                            patch,
                            added,
                            removed,
                        })
                        .await;
                    }
                }
            }
            other => tracing::debug!(
                target: "grove::adapters::opencode",
                "skipping a tool state this adapter does not use: {other}"
            ),
        }
    }

    async fn handle_session_diff(&mut self, properties: &Value) {
        if !self.is_our_session(properties) {
            return;
        }
        let entries = properties
            .get("diff")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for entry in entries {
            let Some(path) = entry
                .get("file")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                .map(str::to_string)
            else {
                tracing::debug!(
                    target: "grove::adapters::opencode",
                    "skipping a diff without a file"
                );
                continue;
            };
            let explicit = entry
                .get("patch")
                .and_then(Value::as_str)
                .filter(|patch| !patch.is_empty())
                .map(str::to_string);
            let patch = explicit.unwrap_or_else(|| {
                build_patch(
                    &path,
                    entry.get("before").and_then(Value::as_str).unwrap_or(""),
                    entry.get("after").and_then(Value::as_str).unwrap_or(""),
                )
            });
            if patch.trim().is_empty() {
                continue;
            }
            let (counted_added, counted_removed) = patch_counts(&patch);
            self.emit(EventKind::Diff {
                path,
                patch: truncate(&patch, PATCH_LIMIT),
                added: first_u32(&entry, &["additions", "added"]).unwrap_or(counted_added),
                removed: first_u32(&entry, &["deletions", "removed"]).unwrap_or(counted_removed),
            })
            .await;
        }
    }

    fn is_our_session(&self, value: &Value) -> bool {
        match (self.session_id.as_deref(), session_of(value)) {
            (Some(ours), Some(theirs)) => {
                let ours = ours == theirs;
                if !ours {
                    tracing::debug!(
                        target: "grove::adapters::opencode",
                        "ignoring a frame from another session: {theirs}"
                    );
                }
                ours
            }
            // A frame with no session reference is accepted: drift must not drop content.
            _ => true,
        }
    }

    fn is_user_part(&self, part: &Value) -> bool {
        part.get("messageID")
            .and_then(Value::as_str)
            .and_then(|id| self.roles.get(id))
            .is_some_and(|role| role == "user")
    }
}

/// Drive the actor for the life of the session. Dropping the event sender ends the pump, which is
/// what records the final `exited` when the session is closed.
async fn run(mut actor: Actor, mut commands: mpsc::UnboundedReceiver<TurnCommand>) {
    actor
        .emit(EventKind::SessionStarted {
            harness: HARNESS.to_string(),
            model: actor.model.clone(),
            resumed: actor.session_id.is_some(),
        })
        .await;
    actor.set_state(SessionState::Starting).await;

    if let Err(reason) = actor.bring_up().await {
        actor
            .emit(EventKind::Error {
                message: reason,
                retryable: false,
            })
            .await;
        actor.closed = true;
        actor.alive.store(false, Ordering::SeqCst);
        actor.shutdown().await;
        actor.set_state(SessionState::Exited).await;
    } else {
        actor.set_state(SessionState::Ready).await;
    }
    if let Some(prompt) = actor.initial_prompt.take() {
        actor.pending.push_back(prompt);
    }
    actor.drive(&mut commands).await;
}

enum Sse {
    /// The `/event` request returned 2xx: the stream is live.
    Connected,
    Event(Value),
    Retry {
        attempt: u32,
        reason: String,
    },
    GaveUp {
        reason: String,
    },
}

struct Reconnect {
    base: Duration,
    attempts: u32,
}

/// Read `/event`, reconnecting with exponential backoff while the session is alive. A stream that
/// cannot be re-established within `attempts` is permanent: the actor turns it into an error and an
/// exited status instead of retrying forever.
async fn stream_events(
    http: reqwest::Client,
    url: String,
    tx: mpsc::Sender<Sse>,
    alive: Arc<AtomicBool>,
    reconnect: Reconnect,
) {
    let mut attempt: u32 = 0;
    loop {
        if !alive.load(Ordering::SeqCst) {
            return;
        }
        let outcome = read_stream(&http, &url, &tx, &alive).await;
        if !alive.load(Ordering::SeqCst) {
            return;
        }
        // A connection that worked resets the backoff, so a flaky-but-recovering server never gives
        // up; only a stream that cannot be established at all becomes permanent.
        if matches!(outcome, Ok(true)) {
            attempt = 0;
        }
        let reason = match outcome {
            Ok(_) => "the server closed the event stream".to_string(),
            Err(error) => error,
        };
        attempt += 1;
        if attempt > reconnect.attempts {
            let _ = tx
                .send(Sse::GaveUp {
                    reason: format!("{reason} (gave up after {attempt} reconnect attempts)"),
                })
                .await;
            return;
        }
        let _ = tx.send(Sse::Retry { attempt, reason }).await;
        let delay = backoff(reconnect.base, attempt);
        tracing::debug!(
            target: "grove::adapters::opencode",
            "reconnecting to {url} in {delay:?} (attempt {attempt}/{})",
            reconnect.attempts
        );
        tokio::time::sleep(delay).await;
    }
}

/// One `/event` connection. `Ok(true)` means the stream was live before it ended.
async fn read_stream(
    http: &reqwest::Client,
    url: &str,
    tx: &mpsc::Sender<Sse>,
    alive: &AtomicBool,
) -> Result<bool, String> {
    let response = http
        .get(url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .send()
        .await
        .map_err(|error| format!("{error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    if tx.send(Sse::Connected).await.is_err() {
        return Ok(true);
    }

    let mut decoder = SseDecoder::default();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if !alive.load(Ordering::SeqCst) {
            return Ok(true);
        }
        let chunk = chunk.map_err(|error| format!("{error}"))?;
        for payload in decoder.push(&chunk) {
            match serde_json::from_str::<Value>(&payload) {
                Ok(value) => {
                    if tx.send(Sse::Event(value)).await.is_err() {
                        return Ok(true);
                    }
                }
                Err(error) => tracing::debug!(
                    target: "grove::adapters::opencode",
                    "skipping a malformed SSE payload ({error}): {}",
                    truncate(&payload, 200)
                ),
            }
        }
    }
    Ok(true)
}

async fn next_sse(rx: &mut Option<mpsc::Receiver<Sse>>) -> Option<Sse> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Incremental `text/event-stream` decoder: bytes in, complete `data:` payloads out.
///
/// Multi-byte characters can be split across chunks, so the buffer stays bytes until a whole line is
/// complete. `event:`/`id:`/`retry:` fields and `: comment` keepalives carry nothing this adapter
/// uses; multiple `data:` lines in one frame are joined with a newline, per the SSE grammar.
#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
    data: String,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut payloads = Vec::new();
        while let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=index).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1])
                .trim_end_matches('\r')
                .to_string();
            if line.is_empty() {
                if !self.data.is_empty() {
                    payloads.push(std::mem::take(&mut self.data));
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("data:") {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
        if self.buffer.len() > MAX_SSE_BYTES {
            // A server that never sends a newline must not grow this forever.
            self.buffer.clear();
            self.data.clear();
        }
        payloads
    }
}

fn backoff(base: Duration, attempt: u32) -> Duration {
    let factor = 1u32 << attempt.saturating_sub(1).min(5);
    (base * factor).min(Duration::from_millis(MAX_RECONNECT_DELAY_MS))
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(format!("Grove/{}", crate::VERSION))
        // A session talks to one loopback server; an environment proxy must never reroute it.
        .no_proxy()
        .build()
        .unwrap_or_default()
}

fn free_loopback_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind((LOOPBACK_HOST, 0))
        .map_err(|error| format!("could not reserve a loopback port: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("could not read the reserved loopback port: {error}"))?
        .port();
    drop(listener);
    Ok(port)
}

/// Validate an attach URL: it must parse, and it must be loopback. A session never leaves the
/// machine, so anything else is refused before a single request is made.
fn loopback_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|error| format!("config.baseUrl is not a URL ({error}): {raw}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("config.baseUrl must be http or https: {raw}"));
    }
    let host = parsed.host_str().unwrap_or_default();
    if !is_loopback_host(host) {
        return Err(format!(
            "config.baseUrl must be a loopback address (127.0.0.1, localhost or ::1), not `{host}`: \
             a Grove session never leaves this machine"
        ));
    }
    Ok(trimmed.to_string())
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

/// The model object `prompt_async` expects. `provider/model` needs no extra config; a bare model id
/// needs `config.providerID`, and without one the server default is used rather than a guess.
fn model_object(model: Option<&str>, provider: Option<&str>) -> Option<Value> {
    let model = model.map(str::trim).filter(|model| !model.is_empty())?;
    if let Some((provider, id)) = model.split_once('/') {
        if !provider.is_empty() && !id.is_empty() {
            return Some(json!({ "providerID": provider, "modelID": id }));
        }
    }
    match provider
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
    {
        Some(provider) => Some(json!({ "providerID": provider, "modelID": model })),
        None => {
            tracing::debug!(
                target: "grove::adapters::opencode",
                "config.providerID is required to send model `{model}`; using the server default"
            );
            None
        }
    }
}

fn session_of(value: &Value) -> Option<&str> {
    value
        .get("sessionID")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("info")
                .and_then(|info| info.get("sessionID"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            value
                .get("part")
                .and_then(|part| part.get("sessionID"))
                .and_then(Value::as_str)
        })
}

fn is_edit_tool(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("edit") || name.contains("write") || name.contains("patch")
}

/// A diff carried by an edit tool's state metadata, the shape OpenCode's edit tools use.
fn tool_diff(part: &Value, state: &Value) -> Option<(String, String, u32, u32)> {
    let patch = string_at(state, &["metadata", "diff"])
        .or_else(|| string_at(state, &["metadata", "patch"]))
        .or_else(|| string_at(part, &["metadata", "diff"]))?;
    if patch.trim().is_empty() {
        return None;
    }
    let path = string_at(state, &["input", "filePath"])
        .or_else(|| string_at(state, &["input", "file_path"]))
        .or_else(|| string_at(state, &["input", "path"]))
        .or_else(|| string_at(state, &["metadata", "file"]))
        .or_else(|| string_at(part, &["filename"]))?;
    let (added, removed) = patch_counts(&patch);
    Some((path, truncate(&patch, PATCH_LIMIT), added, removed))
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn first_u32(value: &Value, keys: &[&str]) -> Option<u32> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .map(|count| count.min(u32::MAX as u64) as u32)
}

/// A unified diff header plus the whole before/after. There is no way to compute real hunks without a
/// diff algorithm, and inventing one would be a worse lie than a readable full-file patch; the added
/// and removed counts come from the server when it sends them.
fn build_patch(path: &str, before: &str, after: &str) -> String {
    let mut patch = format!("--- a/{path}\n+++ b/{path}\n");
    for line in before.lines() {
        patch.push('-');
        patch.push_str(line);
        patch.push('\n');
    }
    for line in after.lines() {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    patch
}

fn patch_counts(patch: &str) -> (u32, u32) {
    let mut added = 0u32;
    let mut removed = 0u32;
    for line in patch.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added += 1;
        } else if line.starts_with('-') {
            removed += 1;
        }
    }
    (added, removed)
}

fn append_tail(tail: &Arc<Mutex<VecDeque<String>>>, line: &str) {
    if let Ok(mut lines) = tail.lock() {
        lines.push_back(line.to_string());
        while lines.len() > STDERR_LINES {
            lines.pop_front();
        }
    }
}

fn tail_text(tail: &Arc<Mutex<VecDeque<String>>>) -> String {
    tail.lock()
        .map(|lines| {
            lines
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_default()
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(limit).collect();
    truncated.push('…');
    truncated
}

/// Prepended context documents (pinned specs, the plan) become the head of the first prompt, in
/// markdown, like every other adapter.
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

async fn kill_tree_by_pid(pid: u32) {
    #[cfg(windows)]
    {
        // `Child::kill` reaches the direct child only, and `opencode` may spawn tools of its own. A
        // failure is fine: the direct kill still runs.
        let pid = pid.to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    #[cfg(not(windows))]
    {
        // The child is spawned as a process-group leader (see `spawn_server`), so the negative pid is
        // the whole tree.
        let group = format!("-{pid}");
        let _ = Command::new("kill")
            .args(["-KILL", group.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
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

    #[test]
    fn only_loopback_urls_are_accepted() {
        assert_eq!(
            loopback_base_url("http://127.0.0.1:4096/").unwrap(),
            "http://127.0.0.1:4096"
        );
        assert!(loopback_base_url("http://localhost:4096").is_ok());
        assert!(loopback_base_url("http://[::1]:4096").is_ok());
        let refused = loopback_base_url("http://example.com:4096").unwrap_err();
        assert!(refused.contains("loopback"), "{refused}");
        assert!(loopback_base_url("not a url").is_err());
    }

    #[test]
    fn the_model_object_prefers_provider_model_and_falls_back_to_config() {
        assert_eq!(
            model_object(Some("anthropic/claude-sonnet-4"), None),
            Some(json!({ "providerID": "anthropic", "modelID": "claude-sonnet-4" }))
        );
        assert_eq!(
            model_object(Some("claude-sonnet-4"), Some("anthropic")),
            Some(json!({ "providerID": "anthropic", "modelID": "claude-sonnet-4" }))
        );
        assert_eq!(
            model_object(Some("claude-sonnet-4"), None),
            None,
            "a bare model id without a provider uses the server default instead of guessing"
        );
        assert_eq!(model_object(None, Some("anthropic")), None);
    }

    #[test]
    fn the_sse_decoder_survives_split_lines_and_noise() {
        let mut decoder = SseDecoder::default();
        assert!(decoder.push(b": keepalive\n\n").is_empty());
        assert!(decoder.push(b"event: message\n").is_empty());
        // A payload split across chunks, including mid-character.
        let payload = "{\"type\":\"t\",\"text\":\"héllo\"}";
        let frame = format!("data: {payload}\n\n");
        let bytes = frame.as_bytes();
        let split = bytes
            .iter()
            .position(|byte| *byte == 0xC3)
            .expect("the multi-byte character is in the frame");
        assert!(decoder.push(&bytes[..split + 1]).is_empty());
        let mut payloads = decoder.push(&bytes[split + 1..]);
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads.pop().unwrap(), payload);

        // Multiple data lines in one frame are joined with a newline, per the SSE grammar.
        let payloads = decoder.push(b"data: line one\ndata: line two\n\n");
        assert_eq!(payloads, vec!["line one\nline two".to_string()]);
        // A blank frame with no data emits nothing.
        assert!(decoder.push(b"\n\n").is_empty());
    }

    #[test]
    fn patches_carry_a_header_and_honest_counts() {
        let patch = build_patch("src/app.rs", "fn main() {}\n", "fn main() {}\nrun();\n");
        assert!(
            patch.starts_with("--- a/src/app.rs\n+++ b/src/app.rs\n"),
            "{patch}"
        );
        assert!(patch.contains("-fn main() {}"), "{patch}");
        assert!(patch.contains("+run();"), "{patch}");
        assert_eq!(patch_counts(&patch), (2, 1));

        let explicit = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n";
        assert_eq!(patch_counts(explicit), (1, 1), "headers are not counted");
    }

    #[test]
    fn an_edit_tool_patch_becomes_a_diff_with_a_path_and_counts() {
        let part = json!({ "callID": "call_1", "tool": "edit" });
        let state = json!({
            "status": "completed",
            "input": { "filePath": "src/main.rs" },
            "metadata": { "diff": "--- a/src/main.rs\n+++ b/src/main.rs\n@@\n+let x = 1;\n" },
        });
        let (path, patch, added, removed) =
            tool_diff(&part, &state).expect("the patch and the path are both present");
        assert_eq!(path, "src/main.rs");
        assert!(patch.contains("+let x = 1;"), "{patch}");
        assert_eq!((added, removed), (1, 0));

        assert_eq!(
            tool_diff(
                &part,
                &json!({ "status": "completed", "output": "no patch" })
            ),
            None,
            "a tool without a patch produces no diff rather than an invented one"
        );
        assert!(is_edit_tool("edit") && is_edit_tool("write") && is_edit_tool("apply_patch"));
        assert!(!is_edit_tool("bash") && !is_edit_tool("read"));
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        let base = Duration::from_millis(100);
        assert_eq!(backoff(base, 1), Duration::from_millis(100));
        assert_eq!(backoff(base, 2), Duration::from_millis(200));
        assert_eq!(backoff(base, 3), Duration::from_millis(400));
        assert_eq!(
            backoff(base, 20),
            Duration::from_millis(MAX_RECONNECT_DELAY_MS),
            "the delay is capped"
        );
    }

    #[test]
    fn context_documents_are_prepended_to_the_prompt() {
        let documents = vec![ContextDoc {
            id: "plan".into(),
            title: "Plan".into(),
            body: "- step one".into(),
        }];
        let rendered = render_prompt(Some("ship ticket 12"), &documents).expect("a prompt");
        assert!(rendered.starts_with("# Context\n\n"));
        assert!(rendered.contains("## Plan"));
        assert!(rendered.contains("---\n\nship ticket 12"));
        assert_eq!(render_prompt(Some("plain"), &[]).as_deref(), Some("plain"));
    }
}
