//! Anthropic Messages API: request encoding and stream decoding.
//!
//! Anthropic's content is a list of typed blocks, its tool input arrives as a JSON *string* built up
//! across deltas, and a tool result must travel inside a `user` turn. All three shapes are handled
//! here so the agent loop stays vendor-neutral.

use super::tools::ToolSpec;
use super::wire::{
    api_error, authorize, Message, MsgBlock, MsgRole, StopReason, StreamDecoder, StreamEvent,
};
use super::{CompletionRequest, ProviderError, ProviderSpec, ThinkingLevel};
use serde_json::json;

pub const API_VERSION: &str = "2023-06-01";
/// One turn's output ceiling. Anthropic requires it; a huge value would be a cost surprise, and
/// raising it can exceed an older model's own output limit. The largest thinking budget below
/// leaves room for a final answer inside this ceiling.
pub const MAX_TOKENS: u32 = 8_192;

/// `POST {base}/v1/messages`.
pub fn endpoint(spec: &ProviderSpec) -> String {
    format!("{}/v1/messages", spec.base_url.trim_end_matches('/'))
}

/// The request body for one turn.
pub fn body(
    model: &str,
    system: &str,
    messages: &[Message],
    tools: &[ToolSpec],
    thinking: ThinkingLevel,
) -> serde_json::Value {
    let mut body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": system,
        "messages": encode_messages(messages),
        "tools": tools
            .iter()
            .map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema,
            }))
            .collect::<Vec<_>>(),
        "stream": true,
    });
    if let Some(budget) = match thinking {
        // Both "off" and an explicit `none` disable extended thinking by leaving the field out.
        ThinkingLevel::Off | ThinkingLevel::None => None,
        // Anthropic's minimum thinking budget is 1024 tokens; the UI's "minimal" is clamped to it.
        ThinkingLevel::Minimal | ThinkingLevel::Low => Some(1_024),
        ThinkingLevel::Medium => Some(2_048),
        ThinkingLevel::High => Some(4_096),
        ThinkingLevel::XHigh => Some(6_144),
        ThinkingLevel::Max => Some(7_168),
    } {
        body["thinking"] = json!({ "type": "enabled", "budget_tokens": budget });
    }
    body
}

/// Anthropic wants alternating roles, and every `tool_result` inside a `user` turn. Consecutive
/// messages of the same role are merged, which is exactly what a multi-tool step produces.
fn encode_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    let mut encoded: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    for message in messages {
        let role = match message.role {
            MsgRole::Assistant => "assistant",
            // A tool result is a user turn in this API.
            MsgRole::User | MsgRole::Tool => "user",
        };
        let blocks: Vec<serde_json::Value> = message.blocks.iter().filter_map(block).collect();
        if blocks.is_empty() {
            continue;
        }
        match encoded.last_mut() {
            Some((last_role, last_blocks)) if last_role == role => last_blocks.extend(blocks),
            _ => encoded.push((role.to_string(), blocks)),
        }
    }
    encoded
        .into_iter()
        .map(|(role, content)| json!({ "role": role, "content": content }))
        .collect()
}

fn block(block: &MsgBlock) -> Option<serde_json::Value> {
    match block {
        MsgBlock::Text { text } => Some(json!({ "type": "text", "text": text })),
        MsgBlock::ToolUse { id, name, input } => Some(json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input,
        })),
        MsgBlock::ToolResult {
            id,
            content,
            is_error,
        } => Some(json!({
            "type": "tool_result",
            "tool_use_id": id,
            "content": content,
            "is_error": is_error,
        })),
    }
}

/// One non-streaming request with no tools and no history: what a session title needs.
///
/// The Messages API accepts a whole answer in one JSON body when `stream` is false. It reuses the
/// same endpoint and auth as a turn, so a title call cannot drift from the session client.
pub async fn complete(
    http: &reqwest::Client,
    spec: &ProviderSpec,
    key: &str,
    request: &CompletionRequest,
) -> Result<String, ProviderError> {
    let body = json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "system": request.system,
        "messages": [{
            "role": "user",
            "content": [{ "type": "text", "text": request.prompt }],
        }],
        "stream": false,
    });
    let response = authorize(http.post(endpoint(spec)), super::Wire::Anthropic, key)
        .json(&body)
        .send()
        .await
        .map_err(|error| ProviderError::Api(format!("cannot reach {}: {error}", spec.name)))?;
    let status = response.status();
    let raw = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(api_error(status.as_u16(), &raw));
    }
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|error| ProviderError::Protocol(format!("{} completion: {error}", spec.name)))?;
    value
        .get("content")
        .and_then(|content| content.as_array())
        .and_then(|blocks| {
            blocks.iter().find_map(|block| {
                (block.get("type").and_then(|kind| kind.as_str()) == Some("text"))
                    .then(|| block.get("text").and_then(|text| text.as_str()))
                    .flatten()
            })
        })
        .map(str::to_string)
        .ok_or_else(|| {
            ProviderError::Protocol(format!("{} completion returned no text", spec.name))
        })
}

/// Decodes `text/event-stream` chunks from `/v1/messages`.
/// The stream names each content block by index; a tool call is only emitted at `content_block_stop`,
/// once its accumulated JSON parses.
#[derive(Default)]
pub struct Decoder {
    /// Index → (tool id, tool name, accumulated argument JSON).
    tools: Vec<(String, String, String)>,
    /// The index currently receiving `input_json_delta`, when it is a tool.
    open: Option<usize>,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StreamDecoder for Decoder {
    fn decode(&mut self, payload: &str) -> Result<Vec<StreamEvent>, ProviderError> {
        let value: serde_json::Value = serde_json::from_str(payload)
            .map_err(|error| ProviderError::Protocol(format!("anthropic chunk: {error}")))?;
        let kind = value
            .get("type")
            .and_then(|kind| kind.as_str())
            .unwrap_or("");
        let mut out = Vec::new();
        match kind {
            "content_block_start" => {
                let index = index_of(&value)?;
                let block = value.get("content_block").cloned().unwrap_or_default();
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let id = block
                        .get("id")
                        .and_then(|id| id.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|name| name.as_str())
                        .unwrap_or_default()
                        .to_string();
                    grow(&mut self.tools, index);
                    self.tools[index] = (id, name, String::new());
                    self.open = Some(index);
                }
            }
            "content_block_delta" => {
                let index = index_of(&value)?;
                let delta = value.get("delta").cloned().unwrap_or_default();
                match delta.get("type").and_then(|t| t.as_str()) {
                    Some("text_delta") => {
                        if let Some(text) = delta.get("text").and_then(|text| text.as_str()) {
                            out.push(StreamEvent::Text(text.to_string()));
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(partial) = delta.get("partial_json").and_then(|p| p.as_str()) {
                            grow(&mut self.tools, index);
                            self.tools[index].2.push_str(partial);
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = index_of(&value)?;
                if self.open == Some(index) {
                    self.open = None;
                }
                if let Some((id, name, raw)) = self.tools.get(index).cloned() {
                    if !name.is_empty() {
                        let input = if raw.trim().is_empty() {
                            json!({})
                        } else {
                            serde_json::from_str(&raw).map_err(|error| {
                                ProviderError::Protocol(format!(
                                    "anthropic tool arguments for {name}: {error}"
                                ))
                            })?
                        };
                        out.push(StreamEvent::ToolUse { id, name, input });
                    }
                }
            }
            "message_start" => {
                if let Some(tokens) = value
                    .get("message")
                    .and_then(|message| message.get("usage"))
                    .and_then(|usage| usage.get("input_tokens"))
                    .and_then(|tokens| tokens.as_u64())
                {
                    out.push(StreamEvent::Usage {
                        input_tokens: tokens,
                        output_tokens: 0,
                    });
                }
            }
            "message_delta" => {
                if let Some(tokens) = value
                    .get("usage")
                    .and_then(|usage| usage.get("output_tokens"))
                    .and_then(|tokens| tokens.as_u64())
                {
                    out.push(StreamEvent::Usage {
                        input_tokens: 0,
                        output_tokens: tokens,
                    });
                }
                if let Some(stop) = value
                    .get("delta")
                    .and_then(|delta| delta.get("stop_reason"))
                    .and_then(|stop| stop.as_str())
                {
                    out.push(StreamEvent::Done {
                        stop: StopReason::parse(stop),
                    });
                }
            }
            "error" => {
                let message = value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(|message| message.as_str())
                    .unwrap_or("anthropic stream error");
                return Err(ProviderError::Api(message.to_string()));
            }
            // `message_stop`, `ping`, and anything a newer API adds: nothing to emit.
            _ => {}
        }
        Ok(out)
    }
}

fn index_of(value: &serde_json::Value) -> Result<usize, ProviderError> {
    value
        .get("index")
        .and_then(|index| index.as_u64())
        .map(|index| index as usize)
        .ok_or_else(|| ProviderError::Protocol("anthropic chunk without an index".into()))
}

fn grow(slots: &mut Vec<(String, String, String)>, index: usize) {
    while slots.len() <= index {
        slots.push((String::new(), String::new(), String::new()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(payloads: &[&str]) -> Vec<StreamEvent> {
        let mut decoder = Decoder::new();
        payloads
            .iter()
            .flat_map(|payload| decoder.decode(payload).unwrap())
            .collect()
    }

    #[test]
    fn text_deltas_become_text_events() {
        let events = decode_all(&[
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#,
        ]);
        assert_eq!(
            events,
            vec![
                StreamEvent::Text("Hel".into()),
                StreamEvent::Text("lo".into())
            ]
        );
    }

    #[test]
    fn a_tool_call_is_emitted_once_its_arguments_parse() {
        let events = decode_all(&[
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"read_file","input":{}}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"a.txt\"}"}}"#,
            r#"{"type":"content_block_stop","index":1}"#,
        ]);
        assert_eq!(
            events,
            vec![StreamEvent::ToolUse {
                id: "toolu_1".into(),
                name: "read_file".into(),
                input: serde_json::json!({ "path": "a.txt" }),
            }]
        );
    }

    #[test]
    fn usage_and_stop_reason_are_reported() {
        let events = decode_all(&[
            r#"{"type":"message_start","message":{"usage":{"input_tokens":12}}}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":7}}"#,
        ]);
        assert_eq!(
            events,
            vec![
                StreamEvent::Usage {
                    input_tokens: 12,
                    output_tokens: 0
                },
                StreamEvent::Usage {
                    input_tokens: 0,
                    output_tokens: 7
                },
                StreamEvent::Done {
                    stop: StopReason::ToolUse
                },
            ]
        );
    }

    #[test]
    fn an_error_frame_becomes_an_api_error() {
        let mut decoder = Decoder::new();
        let error = decoder
            .decode(
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            )
            .unwrap_err();
        assert!(error.to_string().contains("Overloaded"));
    }

    #[test]
    fn unknown_frames_are_ignored_rather_than_fatal() {
        let mut decoder = Decoder::new();
        assert!(decoder.decode(r#"{"type":"ping"}"#).unwrap().is_empty());
        assert!(decoder
            .decode(r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hmm"}}"#)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn malformed_json_is_a_protocol_error_not_a_panic() {
        let mut decoder = Decoder::new();
        assert!(matches!(
            decoder.decode("{not json"),
            Err(ProviderError::Protocol(_))
        ));
    }

    #[test]
    fn tool_results_travel_inside_a_user_turn() {
        let messages = vec![
            Message::user("read it"),
            Message::assistant(vec![MsgBlock::ToolUse {
                id: "t1".into(),
                name: "read_file".into(),
                input: json!({ "path": "a.txt" }),
            }]),
            Message::tool_result("t1", "contents", false),
        ];
        let encoded = encode_messages(&messages);
        assert_eq!(encoded.len(), 3);
        assert_eq!(encoded[1]["content"][0]["type"], "tool_use");
        assert_eq!(encoded[2]["role"], "user");
        assert_eq!(encoded[2]["content"][0]["type"], "tool_result");
    }

    #[test]
    fn consecutive_same_role_messages_are_merged() {
        let messages = vec![
            Message::user("one"),
            Message::user("two"),
            Message::tool_result("t1", "a", false),
            Message::tool_result("t2", "b", false),
        ];
        let encoded = encode_messages(&messages);
        assert_eq!(encoded.len(), 1, "one merged user turn");
        assert_eq!(encoded[0]["content"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn the_endpoint_joins_the_base_url_without_a_double_slash() {
        let spec = crate::providers::catalog::find("anthropic").unwrap();
        assert_eq!(endpoint(&spec), "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn thinking_budget_is_only_sent_when_requested() {
        let high = body("claude", "system", &[], &[], ThinkingLevel::High);
        assert_eq!(high["thinking"]["budget_tokens"], 4_096);
        let max = body("claude", "system", &[], &[], ThinkingLevel::Max);
        assert_eq!(max["thinking"]["budget_tokens"], 7_168);
        let minimal = body("claude", "system", &[], &[], ThinkingLevel::Minimal);
        assert_eq!(minimal["thinking"]["budget_tokens"], 1_024);
        let off = body("claude", "system", &[], &[], ThinkingLevel::Off);
        assert!(off.get("thinking").is_none());
        let none = body("claude", "system", &[], &[], ThinkingLevel::None);
        assert!(none.get("thinking").is_none());
    }

    #[tokio::test]
    async fn a_one_shot_completion_sends_no_tools_or_history() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "content": [
                    { "type": "thinking", "thinking": "hmm" },
                    { "type": "text", "text": "Fix the login bug" }
                ]
            })))
            .mount(&server)
            .await;
        let mut spec = crate::providers::catalog::find("anthropic").unwrap();
        spec.base_url = server.uri();
        let request = crate::providers::CompletionRequest {
            model: "claude-haiku-4-5".into(),
            system: "You name conversations.".into(),
            prompt: "First exchange".into(),
            max_tokens: 64,
        };

        let answer = complete(&reqwest::Client::new(), &spec, "test-key", &request)
            .await
            .expect("the first text block is the answer");
        assert_eq!(answer, "Fix the login bug");

        let sent = server.received_requests().await.unwrap();
        assert_eq!(sent.len(), 1, "one request, not a session");
        let body: serde_json::Value = serde_json::from_slice(&sent[0].body).unwrap();
        assert_eq!(body["stream"], false);
        assert_eq!(body["max_tokens"], 64);
        assert!(
            body.get("tools").is_none(),
            "the title prompt must not see tools"
        );
        assert_eq!(
            body["messages"].as_array().unwrap().len(),
            1,
            "only the one-shot prompt, no conversation history"
        );
        assert!(body.get("thinking").is_none(), "thinking is off");
        assert_eq!(sent[0].headers.get("x-api-key").unwrap(), "test-key");
    }
}
