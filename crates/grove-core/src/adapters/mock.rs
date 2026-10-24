//! The built-in mock harness: no process, no network, fully deterministic.
//!
//! It exists so sessions can be built and tested end to end before a real harness is wired, and it
//! stays registered afterwards as the "does Grove itself work?" harness. It streams the documented
//! shape — `delta` → `message` → `tool_call`/`tool_result` → `status idle` — honours cancel, and
//! exposes a resume token that survives a restart.

use super::{
    AdapterDeps, AdapterError, Capabilities, Detection, HarnessAdapter, HarnessId, HarnessSession,
    StartContext, UserInput,
};
use crate::sessions::event::{
    Block, Decision, EventKind, HarnessEvent, Role, SessionState, ToolStatus,
};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::mpsc;

/// Delay between streamed chunks. Adapter config `chunkDelayMs` overrides it, so tests control the
/// cancel window instead of racing the clock.
pub const DEFAULT_CHUNK_DELAY_MS: u64 = 20;

pub struct MockAdapter;

#[async_trait::async_trait]
impl HarnessAdapter for MockAdapter {
    fn id(&self) -> HarnessId {
        HarnessId("mock")
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            streaming_deltas: true,
            tool_calls: true,
            diffs: false,
            permissions: false,
            resume: true,
            images: false,
            mcp: false,
            usage: true,
        }
    }

    fn detect(&self, _deps: &AdapterDeps) -> Detection {
        Detection::installed(env!("CARGO_PKG_VERSION"), "builtin:mock")
    }

    async fn start(
        &self,
        ctx: StartContext,
        _deps: AdapterDeps,
    ) -> Result<Box<dyn HarnessSession>, AdapterError> {
        let resumed = ctx.resume_token.is_some();
        let token = ctx
            .resume_token
            .clone()
            .unwrap_or_else(|| format!("mock-{}", uuid::Uuid::new_v4()));
        let delay = Duration::from_millis(
            ctx.config
                .get("chunkDelayMs")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(DEFAULT_CHUNK_DELAY_MS),
        );

        let (events, event_rx) = mpsc::channel(256);
        let (commands, command_rx) = mpsc::unbounded_channel();
        tokio::spawn(run(ctx, resumed, delay, events, command_rx));

        Ok(Box::new(MockSession {
            events: Some(event_rx),
            commands,
            token,
        }))
    }
}

enum MockCommand {
    Send(String),
    Cancel,
    Shutdown,
}

struct MockSession {
    events: Option<mpsc::Receiver<HarnessEvent>>,
    commands: mpsc::UnboundedSender<MockCommand>,
    token: String,
}

#[async_trait::async_trait]
impl HarnessSession for MockSession {
    fn take_events(&mut self) -> mpsc::Receiver<HarnessEvent> {
        self.events
            .take()
            .expect("take_events is called exactly once by the session manager")
    }

    async fn send(&mut self, input: UserInput) -> Result<(), AdapterError> {
        self.commands
            .send(MockCommand::Send(input.text))
            .map_err(|_| AdapterError::Other("mock session is closed".into()))
    }

    async fn respond_permission(
        &mut self,
        _request_id: &str,
        _decision: Decision,
    ) -> Result<(), AdapterError> {
        Err(AdapterError::Unsupported(
            "the mock harness never requests permissions".into(),
        ))
    }

    async fn cancel(&mut self) -> Result<(), AdapterError> {
        self.commands
            .send(MockCommand::Cancel)
            .map_err(|_| AdapterError::Other("mock session is closed".into()))
    }

    async fn shutdown(&mut self) -> Result<(), AdapterError> {
        // Shutdown ends the actor quietly: the manager owns the final "exited" transition so it is
        // recorded once, whether the session was closed, restarted, or crashed.
        let _ = self.commands.send(MockCommand::Shutdown);
        Ok(())
    }

    fn resume_token(&self) -> Option<String> {
        Some(self.token.clone())
    }
}

async fn run(
    ctx: StartContext,
    resumed: bool,
    delay: Duration,
    events: mpsc::Sender<HarnessEvent>,
    mut commands: mpsc::UnboundedReceiver<MockCommand>,
) {
    emit(
        &events,
        EventKind::SessionStarted {
            harness: "mock".into(),
            model: ctx.model.clone(),
            resumed,
        },
    )
    .await;
    emit(
        &events,
        EventKind::Status {
            state: SessionState::Starting,
        },
    )
    .await;
    emit(
        &events,
        EventKind::Status {
            state: SessionState::Ready,
        },
    )
    .await;

    let mut pending: VecDeque<MockCommand> = VecDeque::new();
    let mut turn = 0u64;
    loop {
        let command = match pending.pop_front() {
            Some(command) => command,
            None => match commands.recv().await {
                Some(command) => command,
                None => break,
            },
        };
        match command {
            MockCommand::Send(text) => {
                turn += 1;
                stream_reply(&events, &mut commands, &mut pending, &text, turn, delay).await;
            }
            MockCommand::Cancel => {
                emit(
                    &events,
                    EventKind::Status {
                        state: SessionState::Idle,
                    },
                )
                .await;
            }
            MockCommand::Shutdown => break,
        }
    }
}

/// Stream one reply. Returns `true` when a cancel cut it short.
async fn stream_reply(
    events: &mpsc::Sender<HarnessEvent>,
    commands: &mut mpsc::UnboundedReceiver<MockCommand>,
    pending: &mut VecDeque<MockCommand>,
    text: &str,
    turn: u64,
    delay: Duration,
) -> bool {
    emit(
        events,
        EventKind::Status {
            state: SessionState::Busy,
        },
    )
    .await;

    let reply = format!("Mock reply: {text}");
    let mut assembled = String::new();
    let mut cancelled = false;
    for word in reply.split_whitespace() {
        if !assembled.is_empty() {
            assembled.push(' ');
        }
        assembled.push_str(word);
        let chunk = format!("{word} ");

        tokio::select! {
            _ = tokio::time::sleep(delay) => {
                emit(events, EventKind::Delta { text: chunk }).await;
            }
            received = commands.recv() => {
                match received {
                    Some(MockCommand::Cancel) | None => {
                        cancelled = true;
                        break;
                    }
                    Some(MockCommand::Shutdown) => {
                        pending.push_back(MockCommand::Shutdown);
                        cancelled = true;
                        break;
                    }
                    // The manager serializes commands, but a send queued mid-stream is legal: it runs
                    // as the next turn instead of interleaving with this one.
                    Some(other) => pending.push_back(other),
                }
            }
        }
    }

    if cancelled {
        emit(
            events,
            EventKind::Status {
                state: SessionState::Idle,
            },
        )
        .await;
        return true;
    }

    let call_id = format!("mock-call-{turn}");
    let summary = format!("echoed {} chars", text.chars().count());
    emit(
        events,
        EventKind::Message {
            role: Role::Assistant,
            blocks: vec![Block::Text {
                text: assembled.clone(),
            }],
        },
    )
    .await;
    emit(
        events,
        EventKind::ToolCall {
            call_id: call_id.clone(),
            name: "echo".into(),
            title: format!("echo {text}"),
        },
    )
    .await;
    emit(
        events,
        EventKind::ToolResult {
            call_id,
            status: ToolStatus::Ok,
            summary,
        },
    )
    .await;
    emit(
        events,
        EventKind::Usage {
            input_tokens: text.split_whitespace().count() as u64,
            output_tokens: assembled.split_whitespace().count() as u64,
        },
    )
    .await;
    emit(
        events,
        EventKind::Status {
            state: SessionState::Idle,
        },
    )
    .await;

    false
}

async fn emit(events: &mpsc::Sender<HarnessEvent>, kind: EventKind) {
    // The manager's pump owns the receiver for the life of the session; a closed channel means the
    // session is gone, so the actor simply stops.
    let _ = events.send(HarnessEvent::new(kind)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::registry;
    use crate::paths::AppPaths;
    use crate::secrets::MemorySecretStore;
    use std::sync::Arc;
    use std::time::Duration;

    fn deps() -> AdapterDeps {
        AdapterDeps {
            paths: AppPaths::new(std::env::temp_dir().join("grove-mock-tests")),
            http: reqwest::Client::new(),
            secrets: Arc::new(MemorySecretStore::new()),
        }
    }

    fn context(config: serde_json::Value) -> StartContext {
        StartContext {
            cwd: std::env::temp_dir(),
            model: Some("mock-1".into()),
            prompt: None,
            resume_token: None,
            config,
            context_files: Vec::new(),
        }
    }

    async fn next(rx: &mut mpsc::Receiver<HarnessEvent>) -> HarnessEvent {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("mock harness produced an event in time")
            .expect("mock harness is still running")
    }

    #[test]
    fn mock_is_registered() {
        let ids: Vec<_> = registry::builtin_registry()
            .iter()
            .map(|adapter| adapter.id().as_str())
            .collect();
        assert_eq!(ids, vec!["mock"]);
    }

    #[tokio::test]
    async fn streams_the_documented_sequence() {
        let adapter = MockAdapter;
        let mut session = adapter
            .start(context(serde_json::json!({ "chunkDelayMs": 1 })), deps())
            .await
            .expect("mock starts");
        let mut rx = session.take_events();
        session.send(UserInput::text("hello")).await.unwrap();

        let mut tags = Vec::new();
        loop {
            let event = next(&mut rx).await;
            tags.push(event.kind.tag());
            let idle = event.kind
                == (EventKind::Status {
                    state: SessionState::Idle,
                });
            if idle {
                break;
            }
        }

        let first_delta = tags.iter().position(|tag| *tag == "delta").expect("deltas");
        let message = tags
            .iter()
            .position(|tag| *tag == "message")
            .expect("message");
        let tool_call = tags
            .iter()
            .position(|tag| *tag == "tool_call")
            .expect("tool_call");
        let tool_result = tags
            .iter()
            .position(|tag| *tag == "tool_result")
            .expect("tool_result");
        assert!(
            first_delta < message,
            "deltas precede the message: {tags:?}"
        );
        assert!(message < tool_call, "message precedes tool_call: {tags:?}");
        assert!(tool_call < tool_result, "call precedes result: {tags:?}");
        assert_eq!(tags.first(), Some(&"session_started"));
        assert_eq!(tags.last(), Some(&"status"));
    }

    #[tokio::test]
    async fn cancel_cuts_a_stream_short_and_returns_to_idle() {
        let adapter = MockAdapter;
        let mut session = adapter
            .start(context(serde_json::json!({ "chunkDelayMs": 100 })), deps())
            .await
            .expect("mock starts");
        let mut rx = session.take_events();
        session
            .send(UserInput::text("a longer message"))
            .await
            .unwrap();

        // Drain startup events, then the first delta of the reply.
        loop {
            let event = next(&mut rx).await;
            if event.kind.tag() == "delta" {
                break;
            }
        }
        session.cancel().await.unwrap();

        let mut saw_assistant_message = false;
        loop {
            let event = next(&mut rx).await;
            match &event.kind {
                EventKind::Message { role, .. } if *role == Role::Assistant => {
                    saw_assistant_message = true
                }
                EventKind::Status {
                    state: SessionState::Idle,
                } => break,
                _ => {}
            }
        }
        assert!(
            !saw_assistant_message,
            "a cancelled reply does not produce a message event"
        );
    }

    #[tokio::test]
    async fn resume_token_is_stable_across_a_resume() {
        let adapter = MockAdapter;
        let first = adapter
            .start(context(serde_json::json!({})), deps())
            .await
            .unwrap();
        let token = first.resume_token().expect("mock exposes a resume token");

        let mut resume = context(serde_json::json!({}));
        resume.resume_token = Some(token.clone());
        let mut second = adapter.start(resume, deps()).await.unwrap();
        assert_eq!(second.resume_token().as_deref(), Some(token.as_str()));

        let mut rx = second.take_events();
        assert_eq!(
            first_event(&mut rx).await,
            EventKind::SessionStarted {
                harness: "mock".into(),
                model: Some("mock-1".into()),
                resumed: true,
            }
        );
    }

    async fn first_event(rx: &mut mpsc::Receiver<HarnessEvent>) -> EventKind {
        next(rx).await.kind
    }
}
