//! The normalized event model. Every provider reduces to this one enum, which is persisted verbatim
//! and streamed to the UI unchanged (see `architecture.md` §Normalized event model).
//!
//! Adding a kind is additive: `packages/contract` lists the known tags and the webview drops kinds it
//! does not know, so an older UI never breaks on a newer core.

use serde::{Deserialize, Serialize};

/// One event in a session's transcript: when it happened, and what happened.
///
/// `kind` is flattened so the wire shape is `{ "at": "...", "kind": "delta", "text": "..." }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEvent {
    /// RFC3339 with milliseconds, UTC — stable across platforms.
    pub at: String,
    #[serde(flatten)]
    pub kind: EventKind,
}

impl SessionEvent {
    /// Timestamped now. Adapters call this; tests construct events with a fixed `at`.
    pub fn new(kind: EventKind) -> Self {
        Self { at: now(), kind }
    }

    pub fn at(kind: EventKind, at: impl Into<String>) -> Self {
        Self {
            at: at.into(),
            kind,
        }
    }
}

/// The settled event kinds. Rust owns this shape; `packages/contract/src/sessions.ts` mirrors it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    SessionStarted {
        /// harness is the pre-provider name, kept as a read alias so a transcript written by an
        /// older build still loads. It is never written back.
        #[serde(alias = "harness")]
        provider: String,
        model: Option<String>,
        resumed: bool,
    },
    Delta {
        text: String,
    },
    Message {
        role: Role,
        blocks: Vec<Block>,
    },
    ToolCall {
        call_id: String,
        name: String,
        title: String,
    },
    ToolResult {
        call_id: String,
        status: ToolStatus,
        summary: String,
    },
    Diff {
        path: String,
        patch: String,
        added: u32,
        removed: u32,
    },
    PermissionRequest {
        request_id: String,
        title: String,
        detail: String,
    },
    PermissionResolved {
        request_id: String,
        decision: Decision,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    Status {
        state: SessionState,
    },
    Error {
        message: String,
        retryable: bool,
    },
    /// The session's knowledge was (re)assembled from its pinned specs and the plan, and handed to the
    /// provider. Additive: the transcript renders `note` as a visible system line, and `documents` is
    /// exactly what the provider received, so the context a run started from stays auditable.
    ContextRefreshed {
        reason: ContextReason,
        /// The provider that received the documents.
        #[serde(alias = "harness")]
        provider: String,
        documents: Vec<ContextDocument>,
        /// One human-readable line for the transcript.
        note: String,
    },
}

/// What caused a session's context documents to be assembled and handed to the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextReason {
    /// A new session was started.
    Start,
    /// An existing conversation was resumed after a restart.
    Resume,
    /// The user asked for the current pins and plan to be re-sent.
    Refresh,
    /// The same conversation was re-launched on a different provider.
    /// harness_switch is the pre-provider wire value, kept as a read alias for old transcripts.
    #[serde(alias = "harness_switch")]
    ProviderSwitch,
}

impl ContextReason {
    /// The serde tag, for a log line or a test that wants to name the cause.
    pub fn as_str(self) -> &'static str {
        match self {
            ContextReason::Start => "start",
            ContextReason::Resume => "resume",
            ContextReason::Refresh => "refresh",
            ContextReason::ProviderSwitch => "provider_switch",
        }
    }
}

/// One document in a session's context, as recorded in the transcript: what the provider got, not the
/// whole body (the body is already in the session's `StartContext` and would double the log).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDocument {
    /// The spec id, or `plan` for the TODO plan.
    pub id: String,
    pub title: String,
    /// Characters the document contributes, marker included.
    pub chars: usize,
    /// Whether a character budget cut this document short.
    pub truncated: bool,
}

impl EventKind {
    /// The serde tag, for the `events.kind` column. Kept in one place so the column can never drift
    /// from the wire shape.
    pub fn tag(&self) -> &'static str {
        match self {
            EventKind::SessionStarted { .. } => "session_started",
            EventKind::Delta { .. } => "delta",
            EventKind::Message { .. } => "message",
            EventKind::ToolCall { .. } => "tool_call",
            EventKind::ToolResult { .. } => "tool_result",
            EventKind::Diff { .. } => "diff",
            EventKind::PermissionRequest { .. } => "permission_request",
            EventKind::PermissionResolved { .. } => "permission_resolved",
            EventKind::Usage { .. } => "usage",
            EventKind::Status { .. } => "status",
            EventKind::Error { .. } => "error",
            EventKind::ContextRefreshed { .. } => "context_refreshed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        }
    }
}

/// A message is a list of blocks, so thinking and code survive the trip to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text { text: String },
    Thinking { text: String },
    Code { language: String, text: String },
}

impl Block {
    pub fn as_text(&self) -> &str {
        match self {
            Block::Text { text } | Block::Thinking { text } | Block::Code { text, .. } => text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Ok,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    Deny,
}

/// The lifecycle of a session process. Persisted as the `sessions.status` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Starting,
    Ready,
    Busy,
    Idle,
    Exited,
    Error,
}

impl SessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Starting => "starting",
            SessionState::Ready => "ready",
            SessionState::Busy => "busy",
            SessionState::Idle => "idle",
            SessionState::Exited => "exited",
            SessionState::Error => "error",
        }
    }

    /// Parse a persisted status. An unknown value from a newer core degrades to `Error` rather than
    /// failing the whole transcript read.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "starting" => SessionState::Starting,
            "ready" => SessionState::Ready,
            "busy" => SessionState::Busy,
            "idle" => SessionState::Idle,
            "exited" => SessionState::Exited,
            _ => SessionState::Error,
        }
    }
}

impl std::str::FromStr for SessionState {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Ok(SessionState::parse(raw))
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// RFC3339 with milliseconds in UTC: `2025-01-01T00:00:00.000Z`.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Render the tail of a transcript as plain text, for adapters that cannot resume a conversation
/// and need the history replayed as context.
pub fn render_tail(events: &[SessionEvent], limit: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    for event in events {
        match &event.kind {
            EventKind::Message { role, blocks } => {
                let text = blocks
                    .iter()
                    .map(Block::as_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                lines.push(format!("{}: {text}", role.as_str()));
            }
            EventKind::ToolCall { name, title, .. } => lines.push(format!("tool {name}: {title}")),
            EventKind::Error { message, .. } => lines.push(format!("error: {message}")),
            _ => {}
        }
    }
    let start = lines.len().saturating_sub(limit);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_json() {
        let kinds = vec![
            EventKind::SessionStarted {
                provider: "mock".into(),
                model: None,
                resumed: false,
            },
            EventKind::Delta { text: "hi".into() },
            EventKind::Message {
                role: Role::Assistant,
                blocks: vec![
                    Block::Text { text: "a".into() },
                    Block::Thinking { text: "b".into() },
                    Block::Code {
                        language: "rust".into(),
                        text: "c".into(),
                    },
                ],
            },
            EventKind::ToolCall {
                call_id: "c1".into(),
                name: "echo".into(),
                title: "echo hi".into(),
            },
            EventKind::ToolResult {
                call_id: "c1".into(),
                status: ToolStatus::Ok,
                summary: "done".into(),
            },
            EventKind::Diff {
                path: "a.rs".into(),
                patch: "@@".into(),
                added: 1,
                removed: 0,
            },
            EventKind::PermissionRequest {
                request_id: "r1".into(),
                title: "run".into(),
                detail: "rm".into(),
            },
            EventKind::PermissionResolved {
                request_id: "r1".into(),
                decision: Decision::Deny,
            },
            EventKind::Usage {
                input_tokens: 3,
                output_tokens: 4,
            },
            EventKind::Status {
                state: SessionState::Idle,
            },
            EventKind::Error {
                message: "boom".into(),
                retryable: true,
            },
            EventKind::ContextRefreshed {
                reason: ContextReason::Refresh,
                provider: "mock".into(),
                documents: vec![ContextDocument {
                    id: "grove-architecture".into(),
                    title: "Grove — architecture".into(),
                    chars: 42,
                    truncated: false,
                }],
                note: "Context refreshed for mock: 1 document (42 chars)".into(),
            },
        ];

        for kind in kinds {
            let event = SessionEvent::at(kind.clone(), "2025-01-01T00:00:00.000Z");
            let json = serde_json::to_string(&event).unwrap();
            assert!(
                json.contains(&format!("\"kind\":\"{}\"", kind.tag())),
                "tag missing for {}: {json}",
                kind.tag()
            );
            let back: SessionEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(back, event);
        }
    }

    #[test]
    fn a_transcript_written_before_providers_still_loads() {
        // The event payload is persisted JSON, so renaming the field is a schema change: an install
        // with history must keep its transcript readable rather than failing the whole session read.
        let legacy: SessionEvent = serde_json::from_str(
            r#"{"at":"2026-10-24T00:00:00.000Z","kind":"session_started","harness":"claude","model":"claude-sonnet-4-5","resumed":false}"#,
        )
        .expect("an old payload deserialises");
        assert_eq!(
            legacy.kind,
            EventKind::SessionStarted {
                provider: "claude".into(),
                model: Some("claude-sonnet-4-5".into()),
                resumed: false,
            }
        );

        // Writing it back uses the new name, so the alias is read-only.
        let json = serde_json::to_string(&legacy).unwrap();
        assert!(json.contains("\"provider\":\"claude\""), "{json}");
        assert!(!json.contains("\"harness\""), "{json}");

        // The reason enum was renamed too, and it is persisted in the same payload.
        let switched: ContextReason = serde_json::from_str("\"harness_switch\"").unwrap();
        assert_eq!(switched, ContextReason::ProviderSwitch);
        assert_eq!(
            serde_json::to_string(&switched).unwrap(),
            "\"provider_switch\""
        );
    }
    #[test]
    fn event_flattens_kind_beside_at() {
        let event = SessionEvent::at(
            EventKind::Status {
                state: SessionState::Idle,
            },
            "2025-01-01T00:00:00.000Z",
        );
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({
                "at": "2025-01-01T00:00:00.000Z",
                "kind": "status",
                "state": "idle",
            })
        );
    }

    #[test]
    fn session_state_round_trips_and_degrades_unknown_values() {
        for state in [
            SessionState::Starting,
            SessionState::Ready,
            SessionState::Busy,
            SessionState::Idle,
            SessionState::Exited,
            SessionState::Error,
        ] {
            assert_eq!(SessionState::parse(state.as_str()), state);
        }
        assert_eq!(SessionState::parse("from-the-future"), SessionState::Error);
    }

    #[test]
    fn every_context_reason_names_its_wire_tag() {
        for reason in [
            ContextReason::Start,
            ContextReason::Resume,
            ContextReason::Refresh,
            ContextReason::ProviderSwitch,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(json, format!("\"{}\"", reason.as_str()));
        }
    }

    #[test]
    fn render_tail_keeps_the_last_lines() {
        let events: Vec<SessionEvent> = (0..5)
            .map(|n| {
                SessionEvent::at(
                    EventKind::Message {
                        role: Role::User,
                        blocks: vec![Block::Text {
                            text: format!("m{n}"),
                        }],
                    },
                    "2025-01-01T00:00:00.000Z",
                )
            })
            .collect();
        let tail = render_tail(&events, 2);
        assert_eq!(tail, "user: m3\nuser: m4");
    }
}
