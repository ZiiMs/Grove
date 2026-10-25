//! The neutral conversation and the shared transport pieces.
//!
//! Each provider module converts [`Message`] into its own JSON and decodes its own stream into
//! [`StreamEvent`]. Nothing above this module knows a vendor's field names, which is what lets the
//! agent loop be written once.

use super::{ProviderError, Wire};
use crate::sessions::event::Role;
use serde::{Deserialize, Serialize};

/// One message in the conversation the agent keeps in memory.
///
/// A vendor's message shape is lossy in different ways (Anthropic has content blocks, OpenAI has a
/// flat `content` plus separate `tool_calls`), so the neutral form is deliberately block-based: a
/// conversion can always express itself, and a vendor that cannot express a block drops it rather
/// than corrupting the transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: MsgRole,
    pub blocks: Vec<MsgBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MsgRole {
    User,
    Assistant,
    Tool,
}

impl MsgRole {
    pub fn as_str(self) -> &'static str {
        match self {
            MsgRole::User => "user",
            MsgRole::Assistant => "assistant",
            MsgRole::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MsgBlock {
    Text {
        text: String,
    },
    /// The model asked for a tool. `input` is already-parsed JSON: the loop executes from this, so a
    /// malformed fragment must be rejected by the decoder rather than reaching a tool.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        id: String,
        content: String,
        is_error: bool,
    },
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: MsgRole::User,
            blocks: vec![MsgBlock::Text { text: text.into() }],
        }
    }

    pub fn assistant(blocks: Vec<MsgBlock>) -> Self {
        Self {
            role: MsgRole::Assistant,
            blocks,
        }
    }

    pub fn tool_result(id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self {
            role: MsgRole::Tool,
            blocks: vec![MsgBlock::ToolResult {
                id: id.into(),
                content: content.into(),
                is_error,
            }],
        }
    }

    pub fn text_of(&self) -> String {
        let parts: Vec<&str> = self
            .blocks
            .iter()
            .filter_map(|block| match block {
                MsgBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        parts.join("\n")
    }
}

/// One decoded piece of a provider's response stream.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Text(String),
    /// A tool call the model wants to run. Emitted once per call, with its arguments already parsed.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    /// The vendor said the turn is over. `tool_use` means the loop should run tools and continue.
    Done {
        stop: StopReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}

impl StopReason {
    /// Map a vendor's stop string. Unknown values become `EndTurn`: a vendor that adds a reason must
    /// end the turn, not hang the loop.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "tool_use" | "tool_calls" | "function_call" => StopReason::ToolUse,
            "max_tokens" | "length" | "MAX_TOKENS" => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        }
    }
}

/// A vendor's stream decoder. One per running turn: the decoder accumulates whatever partial state
/// that vendor's framing needs (Anthropic's per-index tool arguments, OpenAI's tool-call slots).
pub trait StreamDecoder: Send {
    /// Decode one SSE payload. Unknown frames must return an empty vector, never an error: a vendor
    /// adding an event must degrade to a missing detail, not a dead session.
    fn decode(&mut self, payload: &str) -> Result<Vec<StreamEvent>, ProviderError>;
}

/// One SSE `data:` payload, already de-framed.
pub struct SseFramer {
    buffer: String,
}

impl Default for SseFramer {
    fn default() -> Self {
        Self::new()
    }
}

impl SseFramer {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Feed one network chunk and get back every complete event payload it completed.
    ///
    /// Events are separated by a blank line; an event may carry several `data:` lines, which the SSE
    /// spec concatenates with newlines. Comment lines (`: ping`) are dropped — vendors use them as
    /// keep-alives and they are not a protocol error.
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(chunk);
        let mut payloads = Vec::new();
        while let Some(end) = find_event_end(&self.buffer) {
            // `end` is just past the blank line, so the raw event keeps its terminator and the rest
            // of the buffer starts at the next event's first byte.
            let raw: String = self.buffer.drain(..end).collect();
            if let Some(payload) = event_payload(&raw) {
                payloads.push(payload);
            }
            // Vendors occasionally emit a second separator; it carries no payload, so skip it.
            let skip = self.buffer.len() - self.buffer.trim_start_matches(['\r', '\n']).len();
            self.buffer.drain(..skip);
        }
        payloads
    }
}

/// Index just past the blank line that ends the next event, if there is one.
fn find_event_end(buffer: &str) -> Option<usize> {
    let bytes = buffer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            // `\n\n`, or `\r\n\r\n` (the leftover `\r` is harmless in the drained event).
            if bytes.get(index + 1) == Some(&b'\n') {
                return Some(index + 2);
            }
            if bytes.get(index + 1) == Some(&b'\r') && bytes.get(index + 2) == Some(&b'\n') {
                return Some(index + 3);
            }
        }
        index += 1;
    }
    None
}

fn event_payload(raw: &str) -> Option<String> {
    let mut data: Vec<&str> = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
        // `event:` and `id:` carry nothing this client needs; the payload names its own type.
    }
    if data.is_empty() {
        return None;
    }
    Some(data.join("\n"))
}

/// Apply a credential to an outgoing request in the vendor's own idiom.
///
/// Kept here rather than in three modules so a fix to the header shape cannot miss a provider — the
/// same reasoning that once kept five adapters' helpers in one place.
pub fn authorize(
    mut request: reqwest::RequestBuilder,
    wire: Wire,
    key: &str,
) -> reqwest::RequestBuilder {
    match wire {
        Wire::Anthropic => {
            request = request
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01");
        }
        Wire::OpenAi => {
            request = request.header("authorization", format!("Bearer {key}"));
        }
        Wire::Google => {
            request = request.header("x-goog-api-key", key);
        }
    }
    request
}

/// A vendor's error body, reduced to one line a user can act on.
pub fn api_error(status: u16, body: &str) -> ProviderError {
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|value| {
            // ANTHROPIC: {"error":{"message":..}} · OPENAI: {"error":{"message":..}} · GOOGLE: {"error":{"message":..}}
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(|message| message.as_str())
        })
        .or_else(|| {
            parsed
                .as_ref()
                .and_then(|value| value.get("message"))
                .and_then(|message| message.as_str())
        })
        .unwrap_or(body)
        .trim()
        .to_string();
    let message = if message.is_empty() {
        format!("provider returned HTTP {status}")
    } else {
        format!("HTTP {status}: {message}")
    };
    match status {
        401 | 403 => ProviderError::Unavailable(format!(
            "{message} — check the API key in Settings → Providers"
        )),
        429 => ProviderError::Api(format!("{message} — rate limited, retry shortly")),
        _ => ProviderError::Api(message),
    }
}

/// Map the neutral roles onto the role a transcript row shows. The agent's own messages are
/// assistant turns; a tool result is a system-visible row.
pub fn role_of(message: &Message) -> Role {
    match message.role {
        MsgRole::User => Role::User,
        MsgRole::Assistant => Role::Assistant,
        MsgRole::Tool => Role::System,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn far() -> SseFramer {
        SseFramer::new()
    }

    #[test]
    fn one_chunk_with_two_events_yields_both() {
        let mut framer = far();
        let payloads = framer.push("data: {\"a\":1}\n\ndata: {\"b\":2}\n\n");
        assert_eq!(payloads, vec!["{\"a\":1}", "{\"b\":2}"]);
    }

    #[test]
    fn an_event_split_across_chunks_is_reassembled() {
        let mut framer = far();
        assert!(framer.push("data: {\"delta\":").is_empty());
        assert!(framer.push("\"hello\"").is_empty());
        assert_eq!(framer.push("}\n\n"), vec!["{\"delta\":\"hello\"}"]);
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let mut framer = far();
        assert_eq!(framer.push("data: {\"a\":1}\r\n\r\n"), vec!["{\"a\":1}"]);
    }

    #[test]
    fn keepalive_comments_produce_nothing() {
        let mut framer = far();
        assert!(framer.push(": ping\n\n").is_empty());
        assert_eq!(framer.push("data: x\n\n"), vec!["x"]);
    }

    #[test]
    fn multi_line_data_is_joined_with_newlines() {
        let mut framer = far();
        assert_eq!(framer.push("data: a\ndata: b\n\n"), vec!["a\nb"]);
    }

    #[test]
    fn the_openai_done_sentinel_arrives_as_its_own_payload() {
        let mut framer = far();
        assert_eq!(
            framer.push("data: {\"x\":1}\n\ndata: [DONE]\n\n"),
            vec!["{\"x\":1}", "[DONE]"]
        );
    }

    #[test]
    fn an_error_body_becomes_an_actionable_message() {
        let unavailable = api_error(401, r#"{"error":{"message":"invalid x-api-key"}}"#);
        assert!(matches!(unavailable, ProviderError::Unavailable(_)));
        assert!(unavailable.to_string().contains("Settings → Providers"));

        let limited = api_error(429, r#"{"error":{"message":"slow down"}}"#);
        assert!(limited.to_string().contains("rate limited"));

        let plain = api_error(500, "boom");
        assert_eq!(plain.to_string(), "HTTP 500: boom");
    }

    #[test]
    fn stop_reasons_map_and_unknown_values_end_the_turn() {
        assert_eq!(StopReason::parse("tool_use"), StopReason::ToolUse);
        assert_eq!(StopReason::parse("tool_calls"), StopReason::ToolUse);
        assert_eq!(StopReason::parse("length"), StopReason::MaxTokens);
        assert_eq!(StopReason::parse("something-new"), StopReason::EndTurn);
    }
}
