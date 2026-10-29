//! The agent runtime: one conversation, driven in-process against a provider.
//!
//! This replaces the process-per-provider model. A session holds its conversation in memory, asks the
//! provider for the next turn, streams the text out as deltas, and executes any tool the model asked
//! for — asking the user first when the tool would change the machine.
//!
//! Ordering is the whole game here, because the transcript is the record:
//!
//! ```text
//! status busy → delta* → message(assistant) → tool_call → permission_request →
//!   permission_resolved → tool_result → diff? → (next turn) → usage → status idle
//! ```
//!
//! A turn is spawned rather than awaited inside [`AgentSession::send`], because the session manager
//! serializes commands with a mutex: a turn that ran under that lock would make `cancel` unreachable.
//! Turns are still serialized against each other by [`AgentSessionImpl::gate`].
//!
//! A `subagent` call nests a worker session inside `run_tool`: the worker's permission and question
//! requests are re-emitted on the parent's stream under `sub-` ids, and only its final report goes
//! back to the parent model.

use super::credentials::Credentials;
use super::wire::{
    api_error, authorize, Message, MsgBlock, MsgRole, SseFramer, StopReason, StreamDecoder,
    StreamEvent,
};
use super::{
    anthropic, catalog, google, openai, tools, AgentProvider, AgentSession, Capabilities,
    CompletionRequest, ContextDoc, Detection, ProviderDeps, ProviderError, ProviderSpec,
    StartContext, ThinkingLevel, UserInput, Wire,
};
use crate::sessions::event::{
    Block, Decision, EventKind, Question, QuestionAnswer, Role, SessionEvent, SessionState,
    ToolStatus,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;

/// The tool-loop ceiling. A model that keeps calling tools without converging costs money, so the
/// turn stops and says so rather than running forever.
pub const MAX_STEPS: u32 = 24;
/// How much of one tool result is kept when it is written to the transcript row.
const TOOL_SUMMARY_CHARS: usize = 280;
/// How much of a worker subagent's final report is handed back to the parent model.
const SUBAGENT_REPORT_CHARS: usize = 100_000;
/// How many events may queue for the pump before new ones are dropped. A transcript row lost to a
/// full buffer is better than a turn that blocks on the writer.
const EVENT_BUFFER: usize = 1024;

/// The model-facing shape of the `ask_user` tool.
#[derive(Deserialize)]
struct AskUserInput {
    questions: Vec<Question>,
}

/// The model-facing shape of the `subagent` tool.
#[derive(Deserialize)]
struct SubagentInput {
    description: String,
    prompt: String,
}

/// One provider, as the registry holds it. The wire format decides the encoding, so one
/// implementation serves OpenAI and OpenRouter without either knowing about the other.
pub struct WireProvider {
    spec: ProviderSpec,
}

impl WireProvider {
    pub fn new(spec: ProviderSpec) -> Self {
        Self { spec }
    }
}

#[async_trait::async_trait]
impl AgentProvider for WireProvider {
    fn id(&self) -> &str {
        &self.spec.id
    }

    fn spec(&self) -> &ProviderSpec {
        &self.spec
    }

    fn capabilities(&self) -> Capabilities {
        self.spec.capabilities.clone()
    }

    fn detect(&self, deps: &ProviderDeps) -> Detection {
        Credentials::new(Arc::clone(&deps.secrets)).detect(&self.spec.id)
    }

    /// Dispatch to the vendor module that owns this spec's wire format. Auth, endpoint and body shape
    /// stay inside that module; nothing here knows a vendor field.
    async fn complete(
        &self,
        request: CompletionRequest,
        deps: ProviderDeps,
    ) -> Result<String, ProviderError> {
        let (key, _source) = Credentials::new(Arc::clone(&deps.secrets)).key(&self.spec.id)?;
        match self.spec.wire {
            Wire::Anthropic => anthropic::complete(&deps.http, &self.spec, &key, &request).await,
            Wire::OpenAi => openai::complete(&deps.http, &self.spec, &key, &request).await,
            Wire::Google => google::complete(&deps.http, &self.spec, &key, &request).await,
        }
    }

    async fn start(
        &self,
        ctx: StartContext,
        deps: ProviderDeps,
    ) -> Result<Box<dyn AgentSession>, ProviderError> {
        let credentials = Credentials::new(Arc::clone(&deps.secrets));
        // Resolve the key now: a session that starts and only then discovers it has no credential
        // fails after the user typed something.
        let (key, _source) = credentials.key(&self.spec.id)?;
        let model = self.spec.model_or_default(ctx.model.as_deref());
        let max_steps = ctx
            .config
            .get("maxSteps")
            .and_then(Value::as_u64)
            .map(|steps| steps.clamp(1, u64::from(MAX_STEPS)) as u32)
            .unwrap_or(MAX_STEPS);
        let thinking = ctx
            .config
            .get("thinkingLevel")
            .and_then(Value::as_str)
            .map(ThinkingLevel::parse)
            .unwrap_or_default();
        let system = system_prompt(&ctx);
        let mut history: Vec<Message> = Vec::new();
        if let Some(prompt) = ctx
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            history.push(Message::user(format!(
                "Earlier in this conversation:\n\n{prompt}\n\nContinue from there."
            )));
        }
        let session = AgentSessionImpl::new(
            self.spec.clone(),
            deps.http,
            key,
            ctx.cwd,
            model,
            system,
            history,
            max_steps,
            ctx.session_id,
        );
        *session.thinking.lock().await = thinking;
        Ok(Box::new(session))
    }
}

/// Every provider this build ships, wrapped as a runtime.
pub fn registry() -> Vec<Arc<dyn AgentProvider>> {
    catalog::builtin()
        .into_iter()
        .map(|spec| Arc::new(WireProvider::new(spec)) as Arc<dyn AgentProvider>)
        .collect()
}

struct TurnOutput {
    uses: Vec<MsgBlock>,
    stop: StopReason,
}

/// Attach a gateway's conversation header when the provider declares one. A provider that does not
/// declare a header never sees one, so a session id does not leak to vendors that have no use for it.
fn with_session_header(
    request: reqwest::RequestBuilder,
    header: Option<&str>,
    session_id: &str,
) -> reqwest::RequestBuilder {
    match header {
        Some(header) => request.header(header, session_id),
        None => request,
    }
}

pub struct AgentSessionImpl {
    spec: ProviderSpec,
    http: reqwest::Client,
    key: String,
    cwd: PathBuf,
    model: String,
    system: String,
    /// The session row id, sent to a gateway that routes by conversation.
    session_id: String,
    history: Arc<Mutex<Vec<Message>>>,
    events: Option<mpsc::Receiver<SessionEvent>>,
    tx: mpsc::Sender<SessionEvent>,
    /// Requests waiting for a decision, keyed by request id.
    permits: Arc<Mutex<HashMap<String, oneshot::Sender<Decision>>>>,
    /// Structured questions waiting for an answer, keyed by request id.
    questions: Arc<Mutex<HashMap<String, oneshot::Sender<Vec<QuestionAnswer>>>>>,
    /// Reasoning level applied to the next provider request.
    thinking: Arc<Mutex<ThinkingLevel>>,
    /// Serializes turns. `send` returns as soon as the turn is queued.
    gate: Arc<Mutex<()>>,
    turn: Option<JoinHandle<()>>,
    max_steps: u32,
    /// Session-unique suffix for provider tool-call ids. Google numbers calls per response, so the
    /// same `call_0` returns every turn; the transcript and a worker key need one namespace.
    tool_seq: Arc<AtomicU64>,
    /// The tools this conversation offers. The top-level agent gets everything; a worker gets the
    /// same list minus `subagent`, so delegation cannot recurse.
    tools: Vec<tools::ToolSpec>,
}

impl AgentSessionImpl {
    #[allow(clippy::too_many_arguments)]
    fn new(
        spec: ProviderSpec,
        http: reqwest::Client,
        key: String,
        cwd: PathBuf,
        model: String,
        system: String,
        history: Vec<Message>,
        max_steps: u32,
        session_id: String,
    ) -> Self {
        let (tx, rx) = mpsc::channel(EVENT_BUFFER);
        Self {
            spec,
            http,
            key,
            cwd,
            model,
            system,
            session_id,
            history: Arc::new(Mutex::new(history)),
            events: Some(rx),
            tx,
            permits: Arc::new(Mutex::new(HashMap::new())),
            questions: Arc::new(Mutex::new(HashMap::new())),
            thinking: Arc::new(Mutex::new(ThinkingLevel::Off)),
            gate: Arc::new(Mutex::new(())),
            turn: None,
            max_steps,
            tool_seq: Arc::new(AtomicU64::new(0)),
            tools: tools::specs(),
        }
    }

    fn emit(&self, kind: EventKind) {
        // A closed receiver means the session is being torn down; the event is simply dropped.
        let _ = self.tx.try_send(SessionEvent::new(kind));
    }

    /// Ask the provider for one turn and stream it into the transcript.
    async fn request_turn(&self) -> Result<TurnOutput, ProviderError> {
        let history = self.history.lock().await.clone();
        let thinking = *self.thinking.lock().await;
        let specs = &self.tools;
        let request =
            with_session_header(
                match self.spec.wire {
                    Wire::Anthropic => {
                        let url = anthropic::endpoint(&self.spec);
                        authorize(self.http.post(url), self.spec.wire, &self.key).json(
                            &anthropic::body(&self.model, &self.system, &history, specs, thinking),
                        )
                    }
                    Wire::OpenAi => {
                        let url = openai::endpoint(&self.spec);
                        authorize(self.http.post(url), self.spec.wire, &self.key).json(
                            &openai::body(&self.model, &self.system, &history, specs, thinking),
                        )
                    }
                    Wire::Google => {
                        let url = google::endpoint(&self.spec, &self.model);
                        authorize(self.http.post(url), self.spec.wire, &self.key)
                            .json(&google::body(&self.system, &history, specs, thinking))
                    }
                },
                catalog::transport(&self.spec.id).session_header,
                &self.session_id,
            );

        let response = request.send().await.map_err(|error| {
            ProviderError::Api(format!("cannot reach {}: {error}", self.spec.name))
        })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(api_error(status.as_u16(), &body));
        }

        let mut decoder: Box<dyn StreamDecoder> = match self.spec.wire {
            Wire::Anthropic => Box::new(anthropic::Decoder::new()),
            Wire::OpenAi => Box::new(openai::Decoder::new()),
            Wire::Google => Box::new(google::Decoder::new()),
        };
        let mut framer = SseFramer::new();
        let mut utf8 = Utf8Tail::default();
        let mut text = String::new();
        let mut uses: Vec<MsgBlock> = Vec::new();
        let mut stop = StopReason::EndTurn;

        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|error| ProviderError::Api(format!("stream broke: {error}")))?;
            for payload in framer.push(&utf8.push(&chunk)) {
                for event in decoder.decode(&payload)? {
                    match event {
                        StreamEvent::Text(delta) => {
                            // Some OpenAI-compatible gateways send empty content chunks around tool
                            // calls and usage. Persisting them splits a reply into a preview row
                            // and a final message, so the transcript would show it twice.
                            if delta.is_empty() {
                                continue;
                            }
                            text.push_str(&delta);
                            self.emit(EventKind::Delta { text: delta });
                        }
                        StreamEvent::ToolUse { id, name, input } => {
                            // Google's decoder numbers calls per response (`call_0`, `call_1`), and a
                            // session spans many responses. Qualify every id before it reaches the
                            // history, the event log, or a worker's forwarding prefix so a repeated
                            // response-local id cannot collide across turns.
                            let seq = self.tool_seq.fetch_add(1, Ordering::Relaxed) + 1;
                            uses.push(MsgBlock::ToolUse {
                                id: format!("{id}-{seq}"),
                                name,
                                input,
                            });
                        }
                        StreamEvent::Usage {
                            input_tokens,
                            output_tokens,
                        } => {
                            if input_tokens > 0 || output_tokens > 0 {
                                self.emit(EventKind::Usage {
                                    input_tokens,
                                    output_tokens,
                                });
                            }
                        }
                        StreamEvent::Done { stop: reason } => {
                            stop = reason;
                        }
                    }
                }
            }
        }

        if !text.trim().is_empty() {
            self.emit(EventKind::Message {
                role: Role::Assistant,
                blocks: vec![Block::Text { text: text.clone() }],
            });
        }
        // The assistant turn is part of the conversation even when it was only tool calls.
        let mut blocks: Vec<MsgBlock> = Vec::new();
        if !text.is_empty() {
            blocks.push(MsgBlock::Text { text: text.clone() });
        }
        blocks.extend(uses.iter().cloned());
        if !blocks.is_empty() {
            self.history.lock().await.push(Message::assistant(blocks));
        }
        if uses.is_empty() {
            stop = StopReason::EndTurn;
        }
        Ok(TurnOutput { uses, stop })
    }

    /// Park the turn on a structured question and return the user's answers as the tool result. The
    /// generic tool-call/result rows are intentionally not emitted: the question card is the row.
    async fn ask_user(&self, call_id: &str, input: &Value) -> MsgBlock {
        let ask: AskUserInput = match serde_json::from_value(input.clone()) {
            Ok(ask) => ask,
            Err(error) => {
                return MsgBlock::ToolResult {
                    id: call_id.to_string(),
                    content: format!("invalid ask_user input: {error}"),
                    is_error: true,
                };
            }
        };
        if ask.questions.is_empty()
            || ask.questions.iter().any(|question| {
                question.id.trim().is_empty()
                    || question.title.trim().is_empty()
                    || question.options.is_empty()
            })
        {
            return MsgBlock::ToolResult {
                id: call_id.to_string(),
                content: "ask_user needs at least one question with a title and options".into(),
                is_error: true,
            };
        }

        let request_id = format!("question-{call_id}");
        let (sender, receiver) = oneshot::channel();
        self.questions
            .lock()
            .await
            .insert(request_id.clone(), sender);
        self.emit(EventKind::QuestionRequest {
            request_id: request_id.clone(),
            questions: ask.questions,
        });
        let answers: Vec<QuestionAnswer> = receiver.await.unwrap_or_default();
        self.questions.lock().await.remove(&request_id);
        self.emit(EventKind::QuestionResolved {
            request_id,
            answers: answers.clone(),
        });
        if answers.is_empty() {
            return MsgBlock::ToolResult {
                id: call_id.to_string(),
                content: "the question was cancelled before it was answered".into(),
                is_error: true,
            };
        }
        MsgBlock::ToolResult {
            id: call_id.to_string(),
            content: serde_json::json!({ "answers": answers }).to_string(),
            is_error: false,
        }
    }

    /// Execute one tool call, asking for approval first when it mutates the machine.
    async fn run_tool(&self, call: &MsgBlock) -> MsgBlock {
        let MsgBlock::ToolUse { id, name, input } = call else {
            return MsgBlock::ToolResult {
                id: String::new(),
                content: "internal error: not a tool call".into(),
                is_error: true,
            };
        };
        let call_id = id.clone();
        if name == "ask_user" {
            return self.ask_user(&call_id, input).await;
        }
        self.emit(EventKind::ToolCall {
            call_id: call_id.clone(),
            name: name.clone(),
            title: tools::title(call),
        });

        let mut allowed = true;
        if tools::needs_approval(name) {
            let request_id = format!("perm-{call_id}");
            let (sender, receiver) = oneshot::channel();
            self.permits.lock().await.insert(request_id.clone(), sender);
            self.emit(EventKind::PermissionRequest {
                request_id: request_id.clone(),
                title: tools::title(call),
                detail: tools::detail(call),
            });
            let decision = receiver.await.unwrap_or(Decision::Deny);
            self.permits.lock().await.remove(&request_id);
            self.emit(EventKind::PermissionResolved {
                request_id,
                decision,
            });
            allowed = decision == Decision::Approve;
        }

        let outcome = if name == "subagent" && self.tools.iter().any(|tool| tool.name == "subagent")
        {
            // The worker re-enters the runtime (`run_turn` -> `run_tool`), so the nested future is
            // boxed to keep the async recursion finite.
            Box::pin(self.subagent(&call_id, input)).await
        } else {
            tools::execute(&self.cwd, call, allowed).await
        };
        if let Some(change) = &outcome.diff {
            self.emit(EventKind::Diff {
                path: change.path.clone(),
                patch: change.patch.clone(),
                added: change.added,
                removed: change.removed,
            });
        }
        self.emit(EventKind::ToolResult {
            call_id: call_id.clone(),
            status: if outcome.is_error {
                ToolStatus::Error
            } else {
                ToolStatus::Ok
            },
            summary: summarize(&outcome.content),
        });
        MsgBlock::ToolResult {
            id: call_id,
            content: outcome.content,
            is_error: outcome.is_error,
        }
    }

    async fn run_turn(&self) {
        let _guard = self.gate.lock().await;
        self.emit(EventKind::Status {
            state: SessionState::Busy,
        });
        for step in 0..self.max_steps {
            let output = match self.request_turn().await {
                Ok(output) => output,
                Err(error) => {
                    self.emit(EventKind::Error {
                        message: error.to_string(),
                        retryable: is_retryable(&error),
                    });
                    self.emit(EventKind::Status {
                        state: SessionState::Idle,
                    });
                    return;
                }
            };
            if output.uses.is_empty() {
                break;
            }
            let mut results: Vec<MsgBlock> = Vec::with_capacity(output.uses.len());
            for call in &output.uses {
                results.push(self.run_tool(call).await);
            }
            self.history.lock().await.push(Message {
                role: MsgRole::Tool,
                blocks: results,
            });
            if step + 1 == self.max_steps {
                self.emit(EventKind::Error {
                    message: format!(
                        "Stopped after {} tool steps without a final answer.",
                        self.max_steps
                    ),
                    retryable: true,
                });
            }
            if output.stop == StopReason::MaxTokens {
                break;
            }
        }
        self.emit(EventKind::Status {
            state: SessionState::Idle,
        });
    }

    /// Run one worker agent to completion and return its final report as this call's result.
    ///
    /// The worker shares the provider, model, key and project directory but starts a fresh
    /// conversation. Its mutating calls still come back through this session's approval cards, and
    /// its tool rows and diffs are forwarded so the work stays visible.
    async fn subagent(&self, call_id: &str, input: &Value) -> tools::ToolOutcome {
        let ask: SubagentInput = match serde_json::from_value(input.clone()) {
            Ok(ask) => ask,
            Err(error) => {
                return tools::ToolOutcome::error(format!("invalid subagent input: {error}"))
            }
        };
        let prompt = ask.prompt.trim();
        if prompt.is_empty() {
            return tools::ToolOutcome::error("subagent needs a non-empty prompt");
        }
        let description = ask.description.trim();
        let label = if description.is_empty() {
            "task"
        } else {
            description
        };
        let thinking = *self.thinking.lock().await;
        let system = format!(
            "{}\n\n## Worker subagent\n\
             You are a worker subagent. Work only on the task in the next message. You have the \
             project tools except `subagent`, so you cannot delegate again. Inspect before changing, \
             follow the project's conventions, and verify what you change. Finish with a concise \
             report: what you found or changed, where, and what you verified.\n",
            self.system
        );
        let (worker, events) = self.worker_session(system, thinking);
        worker.history.lock().await.push(Message::user(prompt));
        match self.drive_worker(&worker, events, call_id).await {
            Ok(report) => {
                tools::ToolOutcome::ok(format!("subagent \"{label}\" finished.\n\n{report}"))
            }
            Err(error) => {
                tools::ToolOutcome::error(format!("subagent \"{label}\" failed: {error}"))
            }
        }
    }

    /// A worker session: same dependencies and limits as this one, but a fresh conversation and the
    /// no-recursion tool set. It gets its own event channel; the parent forwards what matters.
    fn worker_session(
        &self,
        system: String,
        thinking: ThinkingLevel,
    ) -> (Self, mpsc::Receiver<SessionEvent>) {
        let (tx, rx) = mpsc::channel(EVENT_BUFFER);
        let session = Self {
            spec: self.spec.clone(),
            http: self.http.clone(),
            key: self.key.clone(),
            cwd: self.cwd.clone(),
            model: self.model.clone(),
            system,
            // A fresh conversation gets its own gateway session key; the parent's transcript is not
            // sent, so sharing the key would only mix two unrelated histories. A UUID keeps the
            // header valid without embedding a provider-supplied id.
            session_id: format!("{}-sub-{}", self.session_id, uuid::Uuid::new_v4()),
            history: Arc::new(Mutex::new(Vec::new())),
            events: None,
            tx,
            permits: Arc::new(Mutex::new(HashMap::new())),
            questions: Arc::new(Mutex::new(HashMap::new())),
            thinking: Arc::new(Mutex::new(thinking)),
            gate: Arc::new(Mutex::new(())),
            turn: None,
            max_steps: self.max_steps,
            tool_seq: Arc::new(AtomicU64::new(0)),
            tools: tools::worker_specs(),
        };
        (session, rx)
    }

    /// Drive the worker's turn while translating its events into the parent transcript. Requests for
    /// approval or an answer are re-emitted under a `sub-` id and parked in the parent's maps, so the
    /// UI answers them through the same commands as the parent's own cards.
    async fn drive_worker(
        &self,
        worker: &AgentSessionImpl,
        mut events: mpsc::Receiver<SessionEvent>,
        call_id: &str,
    ) -> Result<String, String> {
        let prefix = format!("sub-{call_id}");
        let mut errors: Vec<String> = Vec::new();
        let turn = worker.run_turn();
        tokio::pin!(turn);
        loop {
            tokio::select! {
                _ = &mut turn => break,
                event = events.recv() => {
                    let Some(event) = event else { break };
                    self.forward_worker_event(worker, &mut errors, event, &prefix, true)
                        .await;
                }
            }
        }
        // The turn future can win the select with events still buffered. Nothing can be waiting on
        // the user at this point (the turn could not have finished), so only rows are forwarded.
        while let Ok(event) = events.try_recv() {
            self.forward_worker_event(worker, &mut errors, event, &prefix, false)
                .await;
        }
        let report = worker
            .history
            .lock()
            .await
            .iter()
            .rev()
            .find_map(|message| {
                if message.role != MsgRole::Assistant {
                    return None;
                }
                let text = message
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        MsgBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                (!text.trim().is_empty()).then_some(text)
            });
        match report {
            Some(report) => {
                let mut report = cap_report(&report);
                if !errors.is_empty() {
                    report.push_str(&format!("\n\n[subagent warning: {}]", errors.join("; ")));
                }
                Ok(report)
            }
            None if errors.is_empty() => Err("the worker finished without a report".into()),
            None => Err(errors.join("; ")),
        }
    }

    async fn forward_worker_event(
        &self,
        worker: &AgentSessionImpl,
        errors: &mut Vec<String>,
        event: SessionEvent,
        prefix: &str,
        allow_requests: bool,
    ) {
        match event.kind {
            EventKind::ToolCall {
                call_id,
                name,
                title,
            } => {
                self.emit(EventKind::ToolCall {
                    call_id: format!("{prefix}-{call_id}"),
                    name,
                    title: format!("subagent · {title}"),
                });
            }
            EventKind::ToolResult {
                call_id,
                status,
                summary,
            } => {
                self.emit(EventKind::ToolResult {
                    call_id: format!("{prefix}-{call_id}"),
                    status,
                    summary,
                });
            }
            EventKind::Diff {
                path,
                patch,
                added,
                removed,
            } => {
                self.emit(EventKind::Diff {
                    path,
                    patch,
                    added,
                    removed,
                });
            }
            EventKind::Usage {
                input_tokens,
                output_tokens,
            } => {
                self.emit(EventKind::Usage {
                    input_tokens,
                    output_tokens,
                });
            }
            EventKind::Error { message, .. } => errors.push(message),
            EventKind::PermissionRequest {
                request_id,
                title,
                detail,
            } if allow_requests => {
                let forwarded = format!("{prefix}-{request_id}");
                let (sender, receiver) = oneshot::channel();
                self.permits.lock().await.insert(forwarded.clone(), sender);
                self.emit(EventKind::PermissionRequest {
                    request_id: forwarded.clone(),
                    title: format!("subagent · {title}"),
                    detail: format!("A subagent is asking to run this.\n{detail}"),
                });
                let decision = receiver.await.unwrap_or(Decision::Deny);
                self.permits.lock().await.remove(&forwarded);
                self.emit(EventKind::PermissionResolved {
                    request_id: forwarded,
                    decision,
                });
                let _ = worker.answer_permission(&request_id, decision).await;
            }
            EventKind::QuestionRequest {
                request_id,
                questions,
            } if allow_requests => {
                let forwarded = format!("{prefix}-{request_id}");
                let questions = questions
                    .into_iter()
                    .map(|mut question| {
                        question.title = format!("Subagent: {}", question.title);
                        question
                    })
                    .collect::<Vec<_>>();
                let (sender, receiver) = oneshot::channel();
                self.questions
                    .lock()
                    .await
                    .insert(forwarded.clone(), sender);
                self.emit(EventKind::QuestionRequest {
                    request_id: forwarded.clone(),
                    questions,
                });
                let answers = receiver.await.unwrap_or_default();
                self.questions.lock().await.remove(&forwarded);
                self.emit(EventKind::QuestionResolved {
                    request_id: forwarded,
                    answers: answers.clone(),
                });
                let _ = worker.answer_question(&request_id, answers).await;
            }
            _ => {}
        }
    }

    async fn answer_permission(
        &self,
        request_id: &str,
        decision: Decision,
    ) -> Result<(), ProviderError> {
        let sender = self.permits.lock().await.remove(request_id);
        match sender {
            Some(sender) => {
                // The receiver being gone means the turn was cancelled; the answer is moot.
                let _ = sender.send(decision);
                Ok(())
            }
            None => Err(ProviderError::Other(format!(
                "no tool approval is waiting under {request_id}"
            ))),
        }
    }

    async fn answer_question(
        &self,
        request_id: &str,
        answers: Vec<QuestionAnswer>,
    ) -> Result<(), ProviderError> {
        let sender = self.questions.lock().await.remove(request_id);
        match sender {
            Some(sender) => {
                let _ = sender.send(answers);
                Ok(())
            }
            None => Err(ProviderError::Other(format!(
                "no question is waiting under {request_id}"
            ))),
        }
    }
}

#[async_trait::async_trait]
impl AgentSession for AgentSessionImpl {
    fn take_events(&mut self) -> mpsc::Receiver<SessionEvent> {
        self.events
            .take()
            .expect("take_events is called exactly once")
    }

    async fn send(&mut self, input: UserInput) -> Result<(), ProviderError> {
        let text = input.text.trim().to_string();
        if text.is_empty() {
            return Err(ProviderError::Other("message is empty".into()));
        }
        self.history.lock().await.push(Message::user(text));

        // Clone the handles the turn needs so the task owns nothing that outlives the session by
        // reference; shutdown aborts it.
        let spec = self.spec.clone();
        let http = self.http.clone();
        let key = self.key.clone();
        let cwd = self.cwd.clone();
        let model = self.model.clone();
        let system = self.system.clone();
        let session_id = self.session_id.clone();
        let history = Arc::clone(&self.history);
        let tx = self.tx.clone();
        let permits = Arc::clone(&self.permits);
        let questions = Arc::clone(&self.questions);
        let thinking = Arc::clone(&self.thinking);
        let gate = Arc::clone(&self.gate);
        let max_steps = self.max_steps;
        let tool_seq = Arc::clone(&self.tool_seq);
        let tools = self.tools.clone();
        let turn = tokio::spawn(async move {
            let session = AgentSessionImpl {
                spec,
                http,
                key,
                cwd,
                model,
                system,
                session_id,
                history,
                events: None,
                tx,
                permits,
                questions,
                thinking,
                gate,
                turn: None,
                max_steps,
                tool_seq,
                tools,
            };
            session.run_turn().await;
        });
        // A previous turn that already finished leaves a stale handle; dropping it is enough.
        self.turn = Some(turn);
        Ok(())
    }

    async fn set_model(&mut self, model: String) -> Result<(), ProviderError> {
        let model = model.trim();
        if model.is_empty() {
            return Err(ProviderError::Other("model is empty".into()));
        }
        // `send` clones the model into the spawned turn, so the turn already in flight keeps the
        // model it started with and only the next one sees this assignment.
        self.model = model.to_string();
        Ok(())
    }

    async fn respond_permission(
        &mut self,
        request_id: &str,
        decision: Decision,
    ) -> Result<(), ProviderError> {
        self.answer_permission(request_id, decision).await
    }

    async fn respond_question(
        &mut self,
        request_id: &str,
        answers: Vec<QuestionAnswer>,
    ) -> Result<(), ProviderError> {
        self.answer_question(request_id, answers).await
    }

    async fn set_thinking(&mut self, level: ThinkingLevel) -> Result<(), ProviderError> {
        *self.thinking.lock().await = level;
        Ok(())
    }

    async fn cancel(&mut self) -> Result<(), ProviderError> {
        if let Some(turn) = self.turn.take() {
            turn.abort();
        }
        // Every parked approval is denied, so nothing resumes behind the cancelled turn.
        let mut permits = self.permits.lock().await;
        for (_, sender) in permits.drain() {
            let _ = sender.send(Decision::Deny);
        }
        drop(permits);
        // A question has no meaningful default answer; resolve it as empty so its card closes.
        let mut questions = self.questions.lock().await;
        for (request_id, sender) in questions.drain() {
            let _ = sender.send(Vec::new());
            let _ = self
                .tx
                .try_send(SessionEvent::new(EventKind::QuestionResolved {
                    request_id,
                    answers: Vec::new(),
                }));
        }
        drop(questions);
        let _ = self.tx.try_send(SessionEvent::new(EventKind::Status {
            state: SessionState::Idle,
        }));
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), ProviderError> {
        if let Some(turn) = self.turn.take() {
            turn.abort();
        }
        let mut questions = self.questions.lock().await;
        for (request_id, sender) in questions.drain() {
            let _ = sender.send(Vec::new());
            let _ = self
                .tx
                .try_send(SessionEvent::new(EventKind::QuestionResolved {
                    request_id,
                    answers: Vec::new(),
                }));
        }
        Ok(())
    }

    fn resume_token(&self) -> Option<String> {
        // The conversation lives in this process's memory; the manager replays the transcript tail on
        // resume instead (see `capabilities.resume == false`).
        None
    }
}

/// Bytes may split a multi-byte character; keep the incomplete tail for the next chunk instead of
/// emitting a replacement character into the model's text.
#[derive(Default)]
struct Utf8Tail {
    pending: Vec<u8>,
}

impl Utf8Tail {
    fn push(&mut self, chunk: &[u8]) -> String {
        self.pending.extend_from_slice(chunk);
        match std::str::from_utf8(&self.pending) {
            Ok(text) => {
                let text = text.to_string();
                self.pending.clear();
                text
            }
            Err(error) => {
                let valid = error.valid_up_to();
                let text = String::from_utf8_lossy(&self.pending[..valid]).to_string();
                self.pending.drain(..valid);
                text
            }
        }
    }
}

fn is_retryable(error: &ProviderError) -> bool {
    match error {
        ProviderError::Api(message) => {
            message.contains("rate limited") || message.starts_with("HTTP 5")
        }
        ProviderError::Unavailable(_) => false,
        ProviderError::Unsupported(_) => false,
        _ => true,
    }
}

fn summarize(content: &str) -> String {
    let first = content.trim();
    if first.chars().count() <= TOOL_SUMMARY_CHARS {
        return first.to_string();
    }
    let kept: String = first.chars().take(TOOL_SUMMARY_CHARS).collect();
    format!("{kept}…")
}

/// Keep a worker's report from blowing the parent's context. The marker makes the cut explicit to the
/// model instead of silently dropping the tail.
fn cap_report(report: &str) -> String {
    if report.chars().count() <= SUBAGENT_REPORT_CHARS {
        return report.to_string();
    }
    let kept: String = report.chars().take(SUBAGENT_REPORT_CHARS).collect();
    format!(
        "{kept}\n[truncated: {} of {} characters shown]",
        SUBAGENT_REPORT_CHARS,
        report.chars().count()
    )
}

/// The system prompt: what the agent is, where it is, and what the user pinned to this session.
fn system_prompt(ctx: &StartContext) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "You are Grove's coding agent, working inside one project on the user's machine.\n\
         Use the tools to inspect and change the project; do not guess at file contents.\n\
         Paths are relative to the project root. Prefer edit_file over rewriting a whole file.\n\
         Some tools ask the user for approval: if a request is denied, adapt instead of retrying it.\n",
    );
    prompt.push_str(&format!("\nProject root: {}\n", ctx.cwd.display()));
    for ContextDoc { title, body, .. } in &ctx.context_files {
        prompt.push_str(&format!("\n## {title}\n\n{body}\n"));
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecretStore;

    #[test]
    fn the_registry_has_one_entry_per_catalogued_provider() {
        let registry = registry();
        assert_eq!(registry.len(), catalog::builtin().len());
        let ids: Vec<&str> = registry.iter().map(|provider| provider.id()).collect();
        assert_eq!(
            ids,
            vec!["anthropic", "openai", "google", "openrouter", "opencode-go"]
        );
    }

    #[test]
    fn the_system_prompt_carries_the_root_and_the_pinned_documents() {
        let ctx = StartContext {
            cwd: PathBuf::from("C:/work/project"),
            model: None,
            session_id: "test-session".into(),
            prompt: None,
            resume_token: None,
            config: Value::Null,
            context_files: vec![ContextDoc {
                id: "grove-architecture".into(),
                title: "Grove — architecture".into(),
                body: "the design".into(),
            }],
        };
        let prompt = system_prompt(&ctx);
        assert!(prompt.contains("C:/work/project"));
        assert!(prompt.contains("## Grove — architecture"));
        assert!(prompt.contains("the design"));
    }

    #[test]
    fn a_utf8_character_split_across_chunks_is_reassembled() {
        // "é" is two bytes; feed it one byte at a time.
        let mut tail = Utf8Tail::default();
        assert_eq!(tail.push(&[0xC3]), "");
        assert_eq!(tail.push(&[0xA9]), "é");
    }

    #[test]
    fn a_long_tool_result_is_summarised_for_the_row_but_kept_for_the_model() {
        let long = "a".repeat(TOOL_SUMMARY_CHARS + 50);
        let summary = summarize(&long);
        assert!(summary.ends_with('…'));
        assert_eq!(summary.chars().count(), TOOL_SUMMARY_CHARS + 1);
    }

    #[tokio::test]
    async fn starting_without_a_key_fails_before_any_request() {
        let provider = &registry()[0];
        let deps = ProviderDeps {
            paths: crate::paths::AppPaths::new(std::env::temp_dir().join("grove-agent-tests")),
            http: reqwest::Client::new(),
            secrets: Arc::new(MemorySecretStore::new()),
        };
        let error = provider
            .start(
                StartContext {
                    cwd: std::env::temp_dir(),
                    model: None,
                    session_id: "test-session".into(),
                    prompt: None,
                    resume_token: None,
                    config: Value::Null,
                    context_files: Vec::new(),
                },
                deps,
            )
            .await
            .err()
            .expect("no key, no session");
        assert!(matches!(error, ProviderError::Unavailable(_)), "{error}");
        assert!(error.to_string().contains("Settings → Providers"));
    }

    #[tokio::test]
    async fn a_one_shot_completion_dispatches_to_the_vendors_wire_format() {
        use crate::secrets::SecretStore as _;
        use serde_json::json;

        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "content": [{ "type": "text", "text": "Fix the login bug" }]
            })))
            .mount(&server)
            .await;
        let mut spec = catalog::find("anthropic").unwrap();
        spec.base_url = server.uri();
        let secrets = Arc::new(MemorySecretStore::new());
        secrets
            .set("provider.anthropic.api_key", "test-key")
            .unwrap();
        let provider = WireProvider::new(spec);

        let answer = provider
            .complete(
                CompletionRequest {
                    model: "claude-haiku-4-5".into(),
                    system: "You name conversations.".into(),
                    prompt: "First exchange".into(),
                    max_tokens: 64,
                },
                ProviderDeps {
                    paths: crate::paths::AppPaths::new(
                        std::env::temp_dir().join("grove-agent-tests"),
                    ),
                    http: reqwest::Client::new(),
                    secrets,
                },
            )
            .await
            .expect("the anthropic spec goes through the anthropic client");

        assert_eq!(answer, "Fix the login bug");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn answering_an_unknown_permission_request_is_an_error_not_a_hang() {
        let mut session = AgentSessionImpl::new(
            catalog::find("anthropic").unwrap(),
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "claude-sonnet-4-5".into(),
            "system".into(),
            Vec::new(),
            MAX_STEPS,
            "test-session".into(),
        );
        let error = session
            .respond_permission("perm-nope", Decision::Approve)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("no tool approval"));
    }

    #[tokio::test]
    async fn answering_an_unknown_question_is_an_error_not_a_hang() {
        let mut session = AgentSessionImpl::new(
            catalog::find("anthropic").unwrap(),
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "claude-sonnet-4-5".into(),
            "system".into(),
            Vec::new(),
            MAX_STEPS,
            "test-session".into(),
        );
        let error = session
            .respond_question("question-nope", Vec::new())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("no question"));
    }

    #[tokio::test]
    async fn ask_user_parks_the_call_and_returns_the_answers() {
        use crate::sessions::event::QuestionChoice;

        let mut session = AgentSessionImpl::new(
            catalog::find("anthropic").unwrap(),
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "claude-sonnet-4-5".into(),
            "system".into(),
            Vec::new(),
            MAX_STEPS,
            "test-session".into(),
        );
        let mut events = session.take_events();
        let questions = Arc::clone(&session.questions);
        let call = MsgBlock::ToolUse {
            id: "call-1".into(),
            name: "ask_user".into(),
            input: serde_json::json!({
                "questions": [{
                    "id": "scope",
                    "title": "Which scope?",
                    "options": [{ "label": "Small", "description": "Keep it tight." }]
                }]
            }),
        };
        let turn = tokio::spawn(async move { session.run_tool(&call).await });

        let request = events.recv().await.expect("a question request");
        let request_id = match &request.kind {
            EventKind::QuestionRequest { request_id, .. } => request_id.clone(),
            other => panic!("expected a question_request, got {other:?}"),
        };
        let sender = questions
            .lock()
            .await
            .remove(&request_id)
            .expect("the question is parked");
        sender
            .send(vec![QuestionAnswer {
                question_id: "scope".into(),
                answers: vec![QuestionChoice {
                    label: "Small".into(),
                    note: None,
                }],
            }])
            .unwrap();

        let result = turn.await.unwrap();
        let MsgBlock::ToolResult {
            content, is_error, ..
        } = result
        else {
            panic!("ask_user must return a tool result");
        };
        assert!(!is_error);
        assert!(content.contains("Small"), "{content}");

        let resolved = events.recv().await.expect("a question resolution");
        assert!(matches!(resolved.kind, EventKind::QuestionResolved { .. }));
    }

    #[tokio::test]
    async fn an_empty_message_is_refused() {
        let mut session = AgentSessionImpl::new(
            catalog::find("anthropic").unwrap(),
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "claude-sonnet-4-5".into(),
            "system".into(),
            Vec::new(),
            MAX_STEPS,
            "test-session".into(),
        );
        let _ = session.take_events();
        assert!(session.send(UserInput::text("   ")).await.is_err());
    }

    #[tokio::test]
    async fn an_opencode_session_sends_the_session_header() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(
                        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
                    ),
            )
            .mount(&server)
            .await;
        let mut spec = catalog::find("opencode-go").unwrap();
        spec.base_url = format!("{}/openai/v1", server.uri());

        let session = AgentSessionImpl::new(
            spec,
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "kimi-k3".into(),
            "system".into(),
            vec![Message::user("hi")],
            MAX_STEPS,
            "session-42".into(),
        );
        session.request_turn().await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0]
                .headers
                .get("x-opencode-session")
                .and_then(|value| value.to_str().ok()),
            Some("session-42")
        );
    }

    #[tokio::test]
    async fn empty_text_chunks_do_not_become_delta_events() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(
                        concat!(
                            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"\"},\"finish_reason\":null}]}\n\n",
                            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
                            "data: [DONE]\n\n",
                        ),
                    ),
            )
            .mount(&server)
            .await;
        let mut spec = catalog::find("openai").unwrap();
        spec.base_url = server.uri();

        let mut session = AgentSessionImpl::new(
            spec,
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "gpt-5".into(),
            "system".into(),
            vec![Message::user("hi")],
            MAX_STEPS,
            "session-42".into(),
        );
        let mut events = session.take_events();
        session.request_turn().await.unwrap();

        let deltas: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
            .filter_map(|event| match event.kind {
                EventKind::Delta { text } => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["ok"]);
    }

    #[tokio::test]
    async fn changing_the_model_applies_to_the_next_turn_and_keeps_history() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(
                        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
                    ),
            )
            .mount(&server)
            .await;
        let mut spec = catalog::find("openai").unwrap();
        spec.base_url = server.uri();

        let mut session = AgentSessionImpl::new(
            spec,
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "gpt-5".into(),
            "system".into(),
            vec![Message::user("first")],
            MAX_STEPS,
            "session-model".into(),
        );
        let mut events = session.take_events();

        session.send(UserInput::text("second")).await.unwrap();
        wait_until_idle(&mut events).await;

        session.set_model("gpt-5-mini".into()).await.unwrap();
        session.send(UserInput::text("third")).await.unwrap();
        wait_until_idle(&mut events).await;

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2, "neither turn restarted the session");
        let first: Value = serde_json::from_slice(&requests[0].body).unwrap();
        let second: Value = serde_json::from_slice(&requests[1].body).unwrap();
        assert_eq!(first["model"], "gpt-5", "the running turn keeps its model");
        assert_eq!(
            second["model"], "gpt-5-mini",
            "the next turn uses the new model"
        );

        let history = second["messages"].to_string();
        for text in ["first", "second", "third"] {
            assert!(
                history.contains(text),
                "the history lost {text:?}: {history}"
            );
        }
    }

    /// Drain the event stream until the turn reports it is done.
    async fn wait_until_idle(events: &mut mpsc::Receiver<SessionEvent>) {
        while let Some(event) = events.recv().await {
            if matches!(
                event.kind,
                EventKind::Status {
                    state: SessionState::Idle
                }
            ) {
                return;
            }
        }
        panic!("the turn ended without an idle status");
    }

    #[tokio::test]
    async fn changing_thinking_level_reaches_the_next_request() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(
                        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
                    ),
            )
            .mount(&server)
            .await;
        let mut spec = catalog::find("openai").unwrap();
        spec.base_url = server.uri();

        let mut session = AgentSessionImpl::new(
            spec,
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "gpt-5".into(),
            "system".into(),
            vec![Message::user("hi")],
            MAX_STEPS,
            "session-42".into(),
        );
        session.set_thinking(ThinkingLevel::High).await.unwrap();
        session.request_turn().await.unwrap();

        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["reasoning_effort"], "high");
    }

    #[tokio::test]
    async fn a_repeated_provider_call_id_gets_a_session_suffix() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        fn streamed_call(id: &str, target: &str) -> ResponseTemplate {
            let body = serde_json::json!({
                "choices": [{
                    "index": 0,
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": "read_file",
                                "arguments": serde_json::json!({ "path": target }).to_string(),
                            },
                        }],
                    },
                    "finish_reason": "tool_calls",
                }],
            });
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(format!("data: {body}\n\ndata: [DONE]\n\n"))
        }

        let server = MockServer::start().await;
        // Google numbers tool calls per response, so both turns arrive as `call_0`.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(streamed_call("call_0", "a"))
            .mount(&server)
            .await;

        let mut spec = catalog::find("openai").unwrap();
        spec.base_url = server.uri();
        let session = AgentSessionImpl::new(
            spec,
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "gpt-5".into(),
            "system".into(),
            vec![Message::user("hi")],
            MAX_STEPS,
            "session-ids".into(),
        );
        session.request_turn().await.unwrap();
        session.request_turn().await.unwrap();

        let history = session.history.lock().await;
        let ids: Vec<String> = history
            .iter()
            .flat_map(|message| message.blocks.iter())
            .filter_map(|block| match block {
                MsgBlock::ToolUse { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["call_0-1", "call_0-2"]);
    }

    #[tokio::test]
    async fn a_provider_without_a_session_header_sends_none() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(
                        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
                    ),
            )
            .mount(&server)
            .await;
        let mut spec = catalog::find("openai").unwrap();
        spec.base_url = server.uri();

        let session = AgentSessionImpl::new(
            spec,
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "gpt-5".into(),
            "system".into(),
            vec![Message::user("hi")],
            MAX_STEPS,
            "session-42".into(),
        );
        session.request_turn().await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert!(requests[0].headers.get("x-opencode-session").is_none());
    }

    #[tokio::test]
    async fn a_subagent_runs_with_the_parents_approval_and_returns_its_report() {
        use wiremock::matchers::{body_string_contains, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        fn sse(body: &str) -> ResponseTemplate {
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
        }

        let server = MockServer::start().await;
        // The parent delegates. Each response is used once, so the follow-up request (which still
        // contains the earlier messages) cannot match an earlier step.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_string_contains("delegate the write"))
            .respond_with(sse(
                r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call-sub","type":"function","function":{"name":"subagent","arguments":"{\"description\":\"write out.txt\",\"prompt\":\"WROTE-MARKER-PROMPT: write out.txt with hi.\"}"}}]},"finish_reason":"tool_calls"}]}

data: [DONE]

"#,
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        // The worker asks for a mutating tool; the parent must approve it.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_string_contains("WROTE-MARKER-PROMPT"))
            .respond_with(sse(
                r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call-write","type":"function","function":{"name":"write_file","arguments":"{\"path\":\"out.txt\",\"content\":\"hi\"}"}}]},"finish_reason":"tool_calls"}]}

data: [DONE]

"#,
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        // The worker sees the write result and reports.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_string_contains("wrote out.txt"))
            .respond_with(sse(
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"WORKER-REPORT\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        // The parent sees the worker's report and answers.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_string_contains("WORKER-REPORT"))
            .respond_with(sse(
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let mut spec = catalog::find("openai").unwrap();
        spec.base_url = server.uri();
        let mut session = AgentSessionImpl::new(
            spec,
            reqwest::Client::new(),
            "key".into(),
            dir.path().to_path_buf(),
            "gpt-5".into(),
            "system".into(),
            Vec::new(),
            MAX_STEPS,
            "session-sub".into(),
        );
        let mut events = session.take_events();
        session
            .send(UserInput::text("delegate the write"))
            .await
            .unwrap();

        let mut approvals = 0;
        let mut worker_row = false;
        let mut worker_id = None;
        loop {
            let event = events.recv().await.expect("an event");
            match event.kind {
                EventKind::ToolCall { call_id, name, .. } if name == "subagent" => {
                    worker_id = Some(call_id);
                }
                EventKind::ToolCall { title, .. } if title.starts_with("subagent · write_file") => {
                    worker_row = true;
                }
                EventKind::PermissionRequest {
                    request_id, title, ..
                } => {
                    assert!(title.starts_with("subagent · "), "{title}");
                    approvals += 1;
                    session
                        .respond_permission(&request_id, Decision::Approve)
                        .await
                        .unwrap();
                }
                EventKind::Status {
                    state: SessionState::Idle,
                } => break,
                EventKind::Error { message, .. } => panic!("unexpected error: {message}"),
                _ => {}
            }
        }
        assert_eq!(approvals, 1, "the worker's write asked exactly once");
        assert!(worker_row, "the worker's tool call is forwarded");
        let worker_id = worker_id.expect("a subagent tool call");
        assert_ne!(
            worker_id, "call-sub",
            "the provider's response-local id is not the worker key"
        );
        assert!(worker_id.starts_with("call-sub-"), "{worker_id}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("out.txt")).unwrap(),
            "hi"
        );

        // The parent was offered `subagent`; the worker's request was not, so it cannot recurse.
        let requests = server.received_requests().await.unwrap();
        let parent_body = String::from_utf8_lossy(&requests[0].body);
        assert!(
            parent_body.contains("\"name\":\"subagent\""),
            "the parent is offered the subagent tool"
        );
        let worker_body = requests
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body))
            .find(|body| body.contains("WROTE-MARKER-PROMPT"))
            .expect("the worker's first request");
        assert!(
            !worker_body.contains("\"name\":\"subagent\""),
            "the worker cannot delegate again"
        );

        let history = session.history.lock().await;
        let final_text = history.iter().rev().find_map(|message| {
            if message.role != MsgRole::Assistant {
                return None;
            }
            let text = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    MsgBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then_some(text)
        });
        assert_eq!(final_text.as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn a_workers_question_reaches_the_parent_and_the_answers_go_back() {
        use crate::sessions::event::{QuestionChoice, QuestionOption};

        let mut parent = AgentSessionImpl::new(
            catalog::find("anthropic").unwrap(),
            reqwest::Client::new(),
            "key".into(),
            std::env::temp_dir(),
            "claude-sonnet-4-5".into(),
            "system".into(),
            Vec::new(),
            MAX_STEPS,
            "session-parent".into(),
        );
        let mut events = parent.take_events();
        let (worker, _worker_events) = parent.worker_session("system".into(), ThinkingLevel::Off);
        // Park the worker's side of the question so the relay has somewhere to land.
        let (sender, receiver) = oneshot::channel();
        worker
            .questions
            .lock()
            .await
            .insert("question-call-1".into(), sender);

        let event = SessionEvent::new(EventKind::QuestionRequest {
            request_id: "question-call-1".into(),
            questions: vec![Question {
                id: "scope".into(),
                title: "Which scope?".into(),
                hint: String::new(),
                multi: false,
                other: true,
                options: vec![QuestionOption {
                    label: "Small".into(),
                    description: String::new(),
                }],
            }],
        });
        let mut errors = Vec::new();
        let forward = parent.forward_worker_event(&worker, &mut errors, event, "sub-call-1", true);
        let respond = async {
            let event = events.recv().await.expect("the forwarded question");
            let request_id = match event.kind {
                EventKind::QuestionRequest {
                    request_id,
                    questions,
                } => {
                    assert!(request_id.starts_with("sub-call-1-"), "{request_id}");
                    assert_eq!(questions[0].title, "Subagent: Which scope?");
                    request_id
                }
                other => panic!("expected a question_request, got {other:?}"),
            };
            let answers = vec![QuestionAnswer {
                question_id: "scope".into(),
                answers: vec![QuestionChoice {
                    label: "Small".into(),
                    note: None,
                }],
            }];
            parent
                .answer_question(&request_id, answers.clone())
                .await
                .unwrap();
            answers
        };
        let (_, answers) = tokio::join!(forward, respond);

        assert_eq!(
            receiver.await.expect("the worker receives the answers"),
            answers
        );
        let resolved = events.recv().await.expect("a question resolution");
        assert!(matches!(resolved.kind, EventKind::QuestionResolved { .. }));
        assert!(errors.is_empty());
    }
}
