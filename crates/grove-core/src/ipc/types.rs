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
