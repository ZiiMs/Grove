use crate::adapters::Capabilities;
use crate::sessions::event::{HarnessEvent, SessionState};
use serde::{Deserialize, Serialize};

/// Header payload for the shell. Pinned by `contract/fixtures/app-info.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    /// Forward-slash path, so the fixture is identical on Windows and CI.
    pub app_data_dir: String,
    pub platform: String,
}

/// Envelope for every `grove://` event. `payload` stays generic at the wire level; the webview
/// narrows it with a zod schema and ignores kinds it does not know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    pub fn global(payload: T) -> Self {
        Self {
            session_id: None,
            payload,
        }
    }

    pub fn session(session_id: impl Into<String>, payload: T) -> Self {
        Self {
            session_id: Some(session_id.into()),
            payload,
        }
    }
}

/// A session row: the durable identity of a conversation. A session is a row, not a process, which
/// is what makes restart survival possible (see `architecture.md` §Session runtime).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub harness: String,
    /// Forward-slash working directory the harness runs in.
    pub cwd: String,
    pub model: Option<String>,
    pub title: String,
    pub status: SessionState,
    pub created_at: String,
    pub updated_at: String,
}

/// One row of the session list: the session plus the transcript size, so the list needs no join in
/// the webview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    #[serde(flatten)]
    pub session: Session,
    pub event_count: u64,
}

/// A session and its full transcript in sequence order. `session_resume` restarts it from the row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    #[serde(flatten)]
    pub session: Session,
    pub events: Vec<HarnessEvent>,
}

/// A registered harness and its honest capabilities, for the session picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessInfo {
    pub id: String,
    pub capabilities: Capabilities,
}

/// Arguments to `session_create`. `cwd` falls back to the process directory, then the app data dir.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub harness: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Adapter-specific config from settings. Tests pass `chunkDelayMs` to the mock harness.
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}
