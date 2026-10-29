//! An offline provider for tests and previews.
//!
//! It speaks the same runtime as a real provider but answers from a script: chunked deltas, one
//! assistant message, one tool call and a usage row. That keeps the session pump, the transcript
//! folding and the cancellation path testable without a key, a network or a vendor's quirks.
//!
//! It is deliberately **not** in [`super::catalog`]: a shipped build offers real providers only, so a
//! user can never pick "offline" and mistake it for a model.

use super::{
    AgentProvider, AgentSession, AuthMethod, Capabilities, CompletionRequest, Detection,
    ProviderDeps, ProviderError, ProviderSpec, StartContext, UserInput, Wire,
};
use crate::sessions::event::{
    Block, Decision, EventKind, Role, SessionEvent, SessionState, ToolStatus,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

/// The offline provider's id. Tests name it directly.
pub const ID: &str = "mock";

/// How many characters each delta carries.
const CHUNK_CHARS: usize = 4;

pub fn spec() -> ProviderSpec {
    ProviderSpec {
        id: ID.into(),
        name: "Offline".into(),
        short: "OF".into(),
        tint: "#8a8f98".into(),
        wire: Wire::OpenAi,
        base_url: "offline://mock".into(),
        auth: AuthMethod::ApiKey,
        key_url: "https://example.invalid".into(),
        env_var: "GROVE_OFFLINE_PROVIDER".into(),
        models: vec!["mock-1".into()],
        default_model: "mock-1".into(),
        // Unlike a real provider, the offline one claims resume: it has a token to hand back, and the
        // restart test uses that to prove the manager persists and replays one.
        capabilities: Capabilities {
            resume: true,
            ..Capabilities::agent()
        },
    }
}

/// The registry a preview or a test core gets: the offline provider alone.
pub fn registry() -> Vec<Arc<dyn AgentProvider>> {
    vec![Arc::new(OfflineProvider)]
}

pub struct OfflineProvider;

#[async_trait::async_trait]
impl AgentProvider for OfflineProvider {
    fn id(&self) -> &str {
        ID
    }

    fn spec(&self) -> &ProviderSpec {
        // Built once; `spec()` is a hot path for the picker only, and this keeps the trait signature
        // honest without leaking a spec per call.
        static SPEC: std::sync::OnceLock<ProviderSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(spec)
    }

    /// Always ready: there is no credential to configure.
    fn detect(&self, _deps: &ProviderDeps) -> Detection {
        Detection::ready(AuthMethod::ApiKey, spec().default_model)
    }

    /// A deterministic title for tests and previews: the same script every time, with no network.
    async fn complete(
        &self,
        _request: CompletionRequest,
        _deps: ProviderDeps,
    ) -> Result<String, ProviderError> {
        Ok("Offline session".to_string())
    }

    async fn start(
        &self,
        ctx: StartContext,
        _deps: ProviderDeps,
    ) -> Result<Box<dyn AgentSession>, ProviderError> {
        let delay = ctx
            .config
            .get("chunkDelayMs")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let model = ctx
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| spec().default_model);
        let session = OfflineSession::new(ctx.cwd, model, ctx.resume_token.is_some(), delay);
        session
            .emit(EventKind::SessionStarted {
                provider: ID.to_string(),
                model: Some(session.model.clone()),
                resumed: session.resumed,
            })
            .await;
        session
            .emit(EventKind::Status {
                state: SessionState::Ready,
            })
            .await;
        Ok(Box::new(session))
    }
}

struct OfflineSession {
    cwd: PathBuf,
    /// The model the next turn reports; a hot swap only changes this field.
    model: String,
    resumed: bool,
    delay_ms: u64,
    events: Option<mpsc::Receiver<SessionEvent>>,
    tx: mpsc::Sender<SessionEvent>,
    turn: Mutex<Option<JoinHandle<()>>>,
}

impl OfflineSession {
    fn new(cwd: PathBuf, model: String, resumed: bool, delay_ms: u64) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            cwd,
            model,
            resumed,
            delay_ms,
            events: Some(rx),
            tx,
            turn: Mutex::new(None),
        }
    }

    async fn emit(&self, kind: EventKind) {
        let _ = self.tx.send(SessionEvent::new(kind)).await;
    }
}

#[async_trait::async_trait]
impl AgentSession for OfflineSession {
    fn take_events(&mut self) -> mpsc::Receiver<SessionEvent> {
        self.events.take().expect("take_events is called once")
    }

    async fn send(&mut self, input: UserInput) -> Result<(), ProviderError> {
        if input.text.trim().is_empty() {
            return Err(ProviderError::Other("message is empty".into()));
        }
        // A cancelled turn may still be parked here; a second send replaces it.
        if let Some(previous) = self.turn.lock().await.take() {
            previous.abort();
        }
        let reply = format!("Mock reply: {}", input.text.trim());
        let tx = self.tx.clone();
        let delay = self.delay_ms;
        let cwd = self.cwd.clone();
        let turn = tokio::spawn(async move {
            let emit = |kind: EventKind| {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(SessionEvent::new(kind)).await;
                }
            };

            emit(EventKind::Status {
                state: SessionState::Busy,
            })
            .await;

            let chars: Vec<char> = reply.chars().collect();
            for chunk in chars.chunks(CHUNK_CHARS) {
                if delay > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
                emit(EventKind::Delta {
                    text: chunk.iter().collect(),
                })
                .await;
            }

            emit(EventKind::Message {
                role: Role::Assistant,
                blocks: vec![Block::Text { text: reply }],
            })
            .await;

            // One tool call, so the transcript exercises a tool card end to end.
            let call_id = "call-1".to_string();
            emit(EventKind::ToolCall {
                call_id: call_id.clone(),
                name: "echo".into(),
                title: format!("echo in {}", cwd.display()),
            })
            .await;
            emit(EventKind::ToolResult {
                call_id,
                status: ToolStatus::Ok,
                summary: "echoed".into(),
            })
            .await;
            emit(EventKind::Usage {
                input_tokens: 12,
                output_tokens: 8,
            })
            .await;
            emit(EventKind::Status {
                state: SessionState::Idle,
            })
            .await;
        });
        *self.turn.lock().await = Some(turn);
        Ok(())
    }

    async fn respond_permission(
        &mut self,
        _request_id: &str,
        _decision: Decision,
    ) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn respond_question(
        &mut self,
        _request_id: &str,
        _answers: Vec<crate::sessions::event::QuestionAnswer>,
    ) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn set_thinking(
        &mut self,
        _level: crate::providers::ThinkingLevel,
    ) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn set_model(&mut self, model: String) -> Result<(), ProviderError> {
        self.model = model;
        Ok(())
    }

    async fn cancel(&mut self) -> Result<(), ProviderError> {
        if let Some(turn) = self.turn.lock().await.take() {
            turn.abort();
        }
        self.emit(EventKind::Status {
            state: SessionState::Idle,
        })
        .await;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), ProviderError> {
        if let Some(turn) = self.turn.lock().await.take() {
            turn.abort();
        }
        Ok(())
    }

    fn resume_token(&self) -> Option<String> {
        // A stable handle: the manager persists it, and a later resume hands it back, which is what
        // the restart test checks.
        Some("offline-token".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offline_provider_is_not_shipped_in_the_catalog() {
        assert!(crate::providers::catalog::find(ID).is_none());
        assert_eq!(registry().len(), 1);
        assert_eq!(registry()[0].id(), ID);
    }

    #[tokio::test]
    async fn changing_the_model_updates_the_offline_session() {
        let mut session = OfflineSession::new(std::env::temp_dir(), "mock-1".into(), false, 0);
        session.set_model("mock-2".into()).await.unwrap();
        assert_eq!(session.model, "mock-2");
    }

    #[tokio::test]
    async fn a_one_shot_completion_is_deterministic() {
        let provider = OfflineProvider;
        let answer = provider
            .complete(
                CompletionRequest {
                    model: "mock-1".into(),
                    system: "system".into(),
                    prompt: "prompt".into(),
                    max_tokens: 64,
                },
                ProviderDeps {
                    paths: crate::paths::AppPaths::new(std::env::temp_dir().join("grove-offline")),
                    http: reqwest::Client::new(),
                    secrets: Arc::new(crate::secrets::MemorySecretStore::new()),
                },
            )
            .await
            .unwrap();
        assert_eq!(answer, "Offline session");
    }

    #[tokio::test]
    async fn a_turn_streams_then_ends_idle() {
        let provider = OfflineProvider;
        let mut session = provider
            .start(
                StartContext {
                    cwd: std::env::temp_dir(),
                    model: None,
                    session_id: "offline-test-session".into(),
                    prompt: None,
                    resume_token: None,
                    config: serde_json::json!({}),
                    context_files: Vec::new(),
                    history: Vec::new(),
                },
                ProviderDeps {
                    paths: crate::paths::AppPaths::new(std::env::temp_dir().join("grove-offline")),
                    http: reqwest::Client::new(),
                    secrets: Arc::new(crate::secrets::MemorySecretStore::new()),
                },
            )
            .await
            .expect("offline always starts");
        let mut events = session.take_events();
        session.send(UserInput::text("hi")).await.unwrap();

        let mut tags = Vec::new();
        while let Some(event) = events.recv().await {
            let idle = matches!(
                event.kind,
                EventKind::Status {
                    state: SessionState::Idle
                }
            );
            tags.push(event.kind.tag());
            if idle {
                break;
            }
        }
        for required in ["delta", "message", "tool_call", "tool_result", "usage"] {
            assert!(tags.contains(&required), "missing {required} in {tags:?}");
        }
    }
}
