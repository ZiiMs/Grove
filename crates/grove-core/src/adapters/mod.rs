//! The adapter contract: the only place a harness's protocol is known.
//!
//! An adapter depends on this module, [`crate::sessions::event`], the IPC DTOs and generic crates —
//! never on another adapter, `tauri`, or UI code. Adding a harness is one module plus one line in
//! [`registry::builtin_registry`]; nothing else in Grove changes.

pub mod claude;
pub mod mock;
pub mod registry;

pub use claude::ClaudeAdapter;

use crate::paths::AppPaths;
use crate::secrets::SecretStore;
use crate::sessions::event::HarnessEvent;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Stable harness name. It is what the UI stores, what the settings file keys on, and what
/// `sessions.harness` holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct HarnessId(pub &'static str);

impl HarnessId {
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for HarnessId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Capability honesty: a flag is `true` only when the adapter really emits that event kind. The UI
/// degrades from this, never from the harness name.
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

/// A document handed to the harness at start: a pinned spec or the current plan, already rendered to
/// markdown. Ticket 14 fills it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDoc {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// Everything an adapter needs to launch. `config` is adapter-specific and comes from settings; it is
/// the documented hook a user or test uses to point at another binary (`config.command`).
#[derive(Debug, Clone)]
pub struct StartContext {
    pub cwd: PathBuf,
    pub model: Option<String>,
    /// An initial user prompt, or the replayed conversation tail for adapters without resume.
    pub prompt: Option<String>,
    pub resume_token: Option<String>,
    pub config: serde_json::Value,
    pub context_files: Vec<ContextDoc>,
}

/// The services an adapter may use. Sessions never require the network: `http` exists for catalog and
/// MCP work, and a session path must not call it.
#[derive(Clone)]
pub struct AdapterDeps {
    pub paths: AppPaths,
    pub http: reqwest::Client,
    pub secrets: Arc<dyn SecretStore>,
}

/// What the doctor and the settings panel report about an installed harness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    /// Human-readable reason when something is missing or degraded.
    pub detail: Option<String>,
}

impl Detection {
    pub fn installed(version: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            installed: true,
            version: Some(version.into()),
            path: Some(path.into()),
            detail: None,
        }
    }

    pub fn missing(detail: impl Into<String>) -> Self {
        Self {
            installed: false,
            version: None,
            path: None,
            detail: Some(detail.into()),
        }
    }
}

/// The one thing a user can say to a harness.
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
pub enum AdapterError {
    /// The binary or SDK is not installed or not configured. Detection never blocks starting, so this
    /// is raised at start time with an actionable message.
    #[error("adapter unavailable: {0}")]
    Unavailable(String),
    /// The harness does not support this operation (for example permissions on a read-only adapter).
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// The harness spoke something the adapter could not parse. Never fatal to the session.
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("adapter io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

/// One harness. `start` is the only way a process (or an in-process actor) is created.
#[async_trait::async_trait]
pub trait HarnessAdapter: Send + Sync {
    fn id(&self) -> HarnessId;
    fn capabilities(&self) -> Capabilities;
    fn detect(&self, deps: &AdapterDeps) -> Detection;
    async fn start(
        &self,
        ctx: StartContext,
        deps: AdapterDeps,
    ) -> Result<Box<dyn HarnessSession>, AdapterError>;
}

/// A handle to the harness's conversation id, shared by the adapter that learns it and the session
/// pump that persists it.
///
/// A process-backed adapter learns the id from the harness's *first line* — after the manager's
/// post-command mirror has already run — so it updates this slot as soon as it knows, and the pump
/// mirrors it into `sessions.resume_token` without waiting for another command.
pub type ResumeToken = Arc<Mutex<Option<String>>>;

/// One running conversation with a harness.
///
/// Commands are serialized by the session manager, so a session never has two writers on the same
/// stdio pipe. Events flow the other way on the receiver returned by [`HarnessSession::take_events`],
/// which is called exactly once.
#[async_trait::async_trait]
pub trait HarnessSession: Send {
    fn take_events(&mut self) -> tokio::sync::mpsc::Receiver<HarnessEvent>;
    async fn send(&mut self, input: UserInput) -> Result<(), AdapterError>;
    async fn respond_permission(
        &mut self,
        request_id: &str,
        decision: crate::sessions::event::Decision,
    ) -> Result<(), AdapterError>;
    async fn cancel(&mut self) -> Result<(), AdapterError>;
    async fn shutdown(&mut self) -> Result<(), AdapterError>;
    /// The harness's own conversation id, mirrored into `sessions.resume_token`. `None` until the
    /// harness has told the adapter what it is.
    fn resume_token(&self) -> Option<String>;

    /// The slot [`HarnessSession::resume_token`] reads, when the adapter can update it mid-turn.
    ///
    /// Adapters that know their token at start (the mock) leave this `None`: the manager mirrors
    /// after every command. A process-backed adapter returns the same slot its reader task writes,
    /// and the pump mirrors it as soon as it changes.
    fn resume_token_slot(&self) -> Option<ResumeToken> {
        None
    }
}
