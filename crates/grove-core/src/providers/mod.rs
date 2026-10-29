//! Model providers: the only place a vendor's API is known.
//!
//! Grove runs the agent itself. A session picks a **provider** (Anthropic, OpenAI, Google,
//! OpenRouter), a model, and a credential; the [`agent`] runtime drives the conversation, streams
//! tokens, and executes tools locally. There is no provider CLI to install and no adapter process to
//! spawn, so "is it installed?" is replaced by "is a credential configured?".
//!
//! A provider module owns one vendor's wire format and nothing else: it must not import another
//! provider, `tauri`, or UI code. Adding another vendor is one module plus one line in
//! [`catalog::builtin`].

pub mod agent;
pub mod anthropic;
pub mod catalog;
pub mod credentials;
pub mod google;
pub mod model_catalog;
pub mod offline;
pub mod openai;
pub mod probe;
pub mod tools;
pub mod wire;

use crate::paths::AppPaths;
use crate::secrets::SecretStore;
use crate::sessions::event::SessionEvent;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Stable provider name. It is what the UI stores, what settings key on, and what
/// `sessions.provider` holds.
pub type ProviderId = String;

/// How a vendor's HTTP API is shaped. Three of the five ship an OpenAI-compatible surface, so this
/// is the axis that decides which client encodes the request — never the provider id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wire {
    /// Anthropic Messages API (`/v1/messages`, `x-api-key`, `anthropic-version`).
    Anthropic,
    /// OpenAI Chat Completions (`/chat/completions`, `Authorization: Bearer`). OpenRouter speaks it.
    OpenAi,
    /// Google Generative Language (`/v1beta/models/{model}:streamGenerateContent`).
    Google,
}

/// How a user proves who they are to the vendor.
///
/// Grove offers an API key for every shipped provider. Account sign-in (OAuth) is only offered where
/// the vendor licenses third-party clients to use it: Anthropic, OpenAI and Google all restrict
/// subscription OAuth to their own first-party apps, so offering it here would get the user's account
/// suspended. `SignIn` exists for a provider whose vendor does permit it, and [`ProviderSpec::auth`]
/// is what the settings pane reads — the UI never hard-codes "API key".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    ApiKey,
    SignIn,
}

impl AuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthMethod::ApiKey => "api_key",
            AuthMethod::SignIn => "sign_in",
        }
    }
}

/// How much hidden reasoning to ask a model for. The exact list is model-specific and comes from the
/// public models.dev catalogue; `off` omits the vendor field entirely, while the effort levels map to
/// the closest native knob (Anthropic thinking budget, OpenAI-compatible `reasoning_effort`, or
/// Google thinking budget). The ordering is intentional: it is the display order and the fallback
/// ranking when a saved level is not supported by a newly selected model.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    #[default]
    Off,
    /// Explicitly disabled, for a model that accepts `reasoning_effort: "none"` rather than
    /// inferring "off" from a missing field. The UI labels this Off like [`ThinkingLevel::Off`].
    None,
    Minimal,
    Low,
    Medium,
    High,
    // `snake_case` would split this acronym into `x_high`; the wire name is the effort value.
    #[serde(rename = "xhigh", alias = "x_high")]
    XHigh,
    Max,
}

impl ThinkingLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            ThinkingLevel::Off => "off",
            ThinkingLevel::None => "none",
            ThinkingLevel::Minimal => "minimal",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
            ThinkingLevel::XHigh => "xhigh",
            ThinkingLevel::Max => "max",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "none" => ThinkingLevel::None,
            "minimal" => ThinkingLevel::Minimal,
            "low" => ThinkingLevel::Low,
            "medium" => ThinkingLevel::Medium,
            "high" => ThinkingLevel::High,
            "xhigh" => ThinkingLevel::XHigh,
            "max" => ThinkingLevel::Max,
            _ => ThinkingLevel::Off,
        }
    }
}

/// What a provider's models can actually do. Capability honesty: a flag is `true` only when the
/// agent runtime really emits that event kind for this provider, and the UI degrades from here
/// rather than from a model name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub streaming_deltas: bool,
    pub tool_calls: bool,
    pub diffs: bool,
    pub permissions: bool,
    pub resume: bool,
    pub images: bool,
    pub mcp: bool,
    pub usage: bool,
}

impl Capabilities {
    /// What the built-in agent runtime provides for every shipped provider.
    pub fn agent() -> Self {
        Self {
            streaming_deltas: true,
            tool_calls: true,
            diffs: true,
            permissions: true,
            resume: false,
            images: false,
            mcp: false,
            usage: true,
        }
    }
}

/// One provider Grove can talk to. Everything the settings pane and the model picker need, so no UI
/// file keeps a second list of vendors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSpec {
    pub id: String,
    pub name: String,
    /// Two-letter mark for the row and the title bar.
    pub short: String,
    /// Brand tint for the mark, as a CSS colour.
    pub tint: String,
    pub wire: Wire,
    pub base_url: String,
    pub auth: AuthMethod,
    /// Where a user creates the key. Opened in the browser from the settings pane.
    pub key_url: String,
    /// Environment variable the vendor's own SDK reads, shown as a hint.
    pub env_var: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub capabilities: Capabilities,
}

impl ProviderSpec {
    pub fn model_or_default(&self, model: Option<&str>) -> String {
        model
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_model.clone())
    }
}

/// The HTTP client every provider request uses. A gateway sees this User-Agent; OpenCode Go asks
/// clients to identify themselves rather than posing as a generic SDK, and the MCP and skill clients
/// already send the same one.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(format!("Grove/{}", crate::VERSION))
        .build()
        .unwrap_or_default()
}

/// A document handed to the agent at start: a pinned spec or the current plan, already rendered to
/// markdown. Assembled by `sessions::context` under the session's character budgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDoc {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// Everything the agent needs to launch. `config` is provider-specific and comes from settings; the
/// one key the runtime always reads is `maxSteps`, the tool-loop ceiling.
#[derive(Debug, Clone)]
pub struct StartContext {
    pub cwd: PathBuf,
    pub model: Option<String>,
    /// The session row id. It is stable across turns and resumes, and is what a gateway that
    /// routes by conversation (OpenCode Go) receives in its session header.
    pub session_id: String,
    /// The replayed conversation tail when a conversation is resumed in a new process.
    pub prompt: Option<String>,
    pub resume_token: Option<String>,
    pub config: serde_json::Value,
    pub context_files: Vec<ContextDoc>,
}

/// The services the agent runtime may use. Sessions require the network: a provider call is the
/// session. `http` is shared so connections are reused across sessions.
#[derive(Clone)]
pub struct ProviderDeps {
    pub paths: AppPaths,
    pub http: reqwest::Client,
    pub secrets: Arc<dyn SecretStore>,
}

/// What the settings pane reports about a provider's readiness. Replaces the old binary doctor: there
/// is nothing to install, only a credential to configure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    /// A credential is configured for this provider.
    pub installed: bool,
    /// How it authenticates, for the row's label.
    pub method: String,
    /// The model a new session would start with.
    pub default_model: String,
    /// Human-readable reason when something is missing.
    pub detail: Option<String>,
}

impl Detection {
    pub fn ready(method: AuthMethod, default_model: impl Into<String>) -> Self {
        Self {
            installed: true,
            method: method.as_str().to_string(),
            default_model: default_model.into(),
            detail: None,
        }
    }

    pub fn missing(
        method: AuthMethod,
        default_model: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            installed: false,
            method: method.as_str().to_string(),
            default_model: default_model.into(),
            detail: Some(detail.into()),
        }
    }
}

/// The one thing a user can say to the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInput {
    pub text: String,
}

impl UserInput {
    pub fn text(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// No credential configured, or it was rejected.
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    /// The provider answered with something this build cannot parse.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// The vendor rejected the request; the message is theirs, already user-readable.
    #[error("{0}")]
    Api(String),
    #[error("provider io: {0}")]
    Io(#[from] std::io::Error),
    /// The provider (or its running session) cannot perform this operation.
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
}

/// A handle to the conversation id a provider session can resume with, shared by the runtime that
/// learns it and the session pump that persists it. The in-process agent keeps the conversation in
/// its own history, so this stays `None` today and exists for a provider that grows server-side
/// conversation state.
pub type ResumeToken = Arc<Mutex<Option<String>>>;

/// One provider, with whatever launch machinery its wire format needs.
#[async_trait::async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;

    /// The spec this provider was built from: name, models, auth. The settings pane and the picker
    /// read it, so neither keeps a list of vendors.
    fn spec(&self) -> &ProviderSpec;

    fn capabilities(&self) -> Capabilities {
        self.spec().capabilities.clone()
    }

    /// Whether a usable credential exists right now.
    fn detect(&self, deps: &ProviderDeps) -> Detection;

    async fn start(
        &self,
        ctx: StartContext,
        deps: ProviderDeps,
    ) -> Result<Box<dyn AgentSession>, ProviderError>;
}

/// One running conversation. The runtime owns the agent loop; the manager only forwards commands.
///
/// Commands are serialized by the session manager, so a session never has two turns in flight.
/// Events flow the other way on the receiver returned by [`AgentSession::take_events`], which is
/// called exactly once.
#[async_trait::async_trait]
pub trait AgentSession: Send {
    fn take_events(&mut self) -> tokio::sync::mpsc::Receiver<SessionEvent>;
    async fn send(&mut self, input: UserInput) -> Result<(), ProviderError>;
    /// Change the model used by the next turn. The running turn keeps the model it started with.
    ///
    /// Most providers can hot-swap a model because their wire client is stateless per turn; one that
    /// cannot says so instead of pretending the change landed.
    async fn set_model(&mut self, _model: String) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported(
            "this provider cannot change models mid-session".into(),
        ))
    }
    /// Answer a tool permission request. Until it is answered the tool call is parked.
    async fn respond_permission(
        &mut self,
        request_id: &str,
        decision: crate::sessions::event::Decision,
    ) -> Result<(), ProviderError>;
    /// Answer a structured question. Until it is answered the tool call is parked.
    async fn respond_question(
        &mut self,
        request_id: &str,
        answers: Vec<crate::sessions::event::QuestionAnswer>,
    ) -> Result<(), ProviderError>;
    /// Change the reasoning level used by the next turn.
    async fn set_thinking(
        &mut self,
        level: crate::providers::ThinkingLevel,
    ) -> Result<(), ProviderError>;
    /// Stop the current turn. The conversation survives and stays resumable.
    async fn cancel(&mut self) -> Result<(), ProviderError>;
    async fn shutdown(&mut self) -> Result<(), ProviderError>;
    fn resume_token(&self) -> Option<String>;
    fn resume_token_slot(&self) -> Option<ResumeToken> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{catalog, AgentSession as _, ThinkingLevel};

    /// A session that implements only the required methods, so the default `set_model` is exercised.
    struct PlainSession;

    #[async_trait::async_trait]
    impl super::AgentSession for PlainSession {
        fn take_events(
            &mut self,
        ) -> tokio::sync::mpsc::Receiver<crate::sessions::event::SessionEvent> {
            tokio::sync::mpsc::channel(1).1
        }

        async fn send(&mut self, _input: super::UserInput) -> Result<(), super::ProviderError> {
            Ok(())
        }

        async fn respond_permission(
            &mut self,
            _request_id: &str,
            _decision: crate::sessions::event::Decision,
        ) -> Result<(), super::ProviderError> {
            Ok(())
        }

        async fn respond_question(
            &mut self,
            _request_id: &str,
            _answers: Vec<crate::sessions::event::QuestionAnswer>,
        ) -> Result<(), super::ProviderError> {
            Ok(())
        }

        async fn set_thinking(
            &mut self,
            _level: super::ThinkingLevel,
        ) -> Result<(), super::ProviderError> {
            Ok(())
        }

        async fn cancel(&mut self) -> Result<(), super::ProviderError> {
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), super::ProviderError> {
            Ok(())
        }

        fn resume_token(&self) -> Option<String> {
            None
        }
    }

    #[tokio::test]
    async fn a_session_without_hot_swap_reports_unsupported() {
        let mut session = PlainSession;
        let error = session.set_model("another-model".into()).await.unwrap_err();
        assert!(matches!(error, super::ProviderError::Unsupported(_)));
    }

    #[test]
    fn thinking_levels_use_the_same_names_on_the_wire_as_in_the_config() {
        for level in [
            ThinkingLevel::Off,
            ThinkingLevel::None,
            ThinkingLevel::Minimal,
            ThinkingLevel::Low,
            ThinkingLevel::Medium,
            ThinkingLevel::High,
            ThinkingLevel::XHigh,
            ThinkingLevel::Max,
        ] {
            let encoded = serde_json::to_string(&level).unwrap();
            assert_eq!(
                encoded,
                format!("\"{}\"", level.as_str()),
                "serialized form must match the effort name"
            );
            let decoded: ThinkingLevel = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, level);
        }
        // An earlier cache wrote serde's `x_high` spelling; keep reading it rather than refetching
        // solely because the wire name changed.
        assert_eq!(
            serde_json::from_str::<ThinkingLevel>("\"x_high\"").unwrap(),
            ThinkingLevel::XHigh
        );
    }

    #[test]
    fn every_provider_has_a_unique_id_and_a_default_model_it_ships() {
        let specs = catalog::builtin();
        let mut ids: Vec<&str> = specs.iter().map(|spec| spec.id.as_str()).collect();
        let total = ids.len();
        assert!(total >= 5, "the shipped providers are registered");
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "provider ids must be unique");

        for spec in &specs {
            assert!(
                spec.models.contains(&spec.default_model),
                "{} defaults to a model it does not ship",
                spec.id
            );
            assert!(spec.key_url.starts_with("https://"), "{} key_url", spec.id);
            assert!(!spec.env_var.is_empty(), "{} env_var", spec.id);
        }
    }

    #[test]
    fn a_requested_model_wins_and_an_empty_one_falls_back_to_the_default() {
        let spec = catalog::find("anthropic").expect("anthropic ships");
        assert_eq!(
            spec.model_or_default(Some("claude-opus-4-1")),
            "claude-opus-4-1"
        );
        assert_eq!(spec.model_or_default(Some("   ")), spec.default_model);
        assert_eq!(spec.model_or_default(None), spec.default_model);
    }

    #[tokio::test]
    async fn the_provider_http_client_identifies_as_grove() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&server)
            .await;

        super::http_client().get(server.uri()).send().await.unwrap();

        let requests = server.received_requests().await.unwrap();
        let user_agent = requests[0]
            .headers
            .get("user-agent")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(
            user_agent.starts_with("Grove/"),
            "provider requests identify the client, got {user_agent:?}"
        );
    }
}
