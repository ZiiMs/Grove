//! OpenAI Chat Completions: request encoding and stream decoding.
//!
//! OpenRouter speaks this same surface, so both providers share this module and differ only by base
//! URL and model ids (see [`super::catalog`]). Tool arguments arrive as JSON **fragments** keyed by
//! tool-call index, so the decoder buffers each slot and only emits a call at `finish_reason`.

use super::tools::ToolSpec;
use super::wire::{
    api_error, authorize, Message, MsgBlock, MsgRole, StopReason, StreamDecoder, StreamEvent,
};
use super::{CompletionRequest, ProviderError, ProviderSpec, ThinkingLevel};
use serde_json::json;

/// `POST {base}/chat/completions`. The catalog's base already carries `/v1` where the vendor wants it.
pub fn endpoint(spec: &ProviderSpec) -> String {
    format!("{}/chat/completions", spec.base_url.trim_end_matches('/'))
}

pub fn body(
    model: &str,
    system: &str,
    messages: &[Message],
    tools: &[ToolSpec],
    thinking: ThinkingLevel,
) -> serde_json::Value {
    let mut body = json!({
        "model": model,
        "messages": encode_messages(system, messages),
        "tools": tools
            .iter()
            .map(|tool| json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            }))
            .collect::<Vec<_>>(),
        "stream": true,
        // Without this the final chunk carries no usage and a session's token count is always zero.
        "stream_options": { "include_usage": true },
    });
    if thinking != ThinkingLevel::Off {
        body["reasoning_effort"] = json!(thinking.as_str());
    }
    body
}

fn encode_messages(system: &str, messages: &[Message]) -> Vec<serde_json::Value> {
    let mut encoded = Vec::new();
    if !system.trim().is_empty() {
        encoded.push(json!({ "role": "system", "content": system }));
    }
    for message in messages {
        if message.role == MsgRole::Tool {
            for block in &message.blocks {
                if let MsgBlock::ToolResult {
                    id,
                    content,
                    is_error,
                } = block
                {
                    // This API has no error flag on a tool message; a failed tool says so in its text.
                    let content = if *is_error {
                        format!("ERROR: {content}")
                    } else {
                        content.clone()
                    };
                    encoded.push(json!({
                        "role": "tool",
                        "tool_call_id": id,
                        "content": content,
                    }));
                }
            }
            continue;
        }
        if message.role == MsgRole::Assistant {
            let text = message.text_of();
            let calls: Vec<serde_json::Value> = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    MsgBlock::ToolUse { id, name, input } => Some(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": input.to_string(),
                        }
                    })),
                    _ => None,
                })
                .collect();
            let mut encoded_message = json!({ "role": "assistant", "content": text });
            if !calls.is_empty() {
                encoded_message["tool_calls"] = serde_json::Value::Array(calls);
            }
            encoded.push(encoded_message);
            continue;
        }
        encoded.push(json!({ "role": "user", "content": message.text_of() }));
    }
    encoded
}

/// One non-streaming request with no tools and no history: what a session title needs.
pub async fn complete(
    http: &reqwest::Client,
    spec: &ProviderSpec,
    key: &str,
    request: &CompletionRequest,
) -> Result<String, ProviderError> {
    let body = json!({
        "model": request.model,
        "messages": [
            { "role": "system", "content": request.system },
            { "role": "user", "content": request.prompt },
        ],
        "max_tokens": request.max_tokens,
        "stream": false,
    });
    let response = authorize(http.post(endpoint(spec)), super::Wire::OpenAi, key)
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
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            ProviderError::Protocol(format!("{} completion returned no text", spec.name))
        })
}

/// Decodes `text/event-stream` chunks from `/chat/completions`.
#[derive(Default)]
pub struct Decoder {
    /// Tool-call index → (id, name, argument fragments).
    slots: Vec<(String, String, String)>,
    finished: bool,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    fn flush_tools(&mut self, out: &mut Vec<StreamEvent>) -> Result<(), ProviderError> {
        for (id, name, raw) in std::mem::take(&mut self.slots) {
            if name.is_empty() {
                continue;
            }
            let input = if raw.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&raw).map_err(|error| {
                    ProviderError::Protocol(format!("{name} arguments: {error}"))
                })?
            };
            out.push(StreamEvent::ToolUse { id, name, input });
        }
        Ok(())
    }
}

impl StreamDecoder for Decoder {
    fn decode(&mut self, payload: &str) -> Result<Vec<StreamEvent>, ProviderError> {
        if payload.trim() == "[DONE]" {
            let mut out = Vec::new();
            self.flush_tools(&mut out)?;
            if !self.finished {
                self.finished = true;
                out.push(StreamEvent::Done {
                    stop: StopReason::EndTurn,
                });
            }
            return Ok(out);
        }
        let value: serde_json::Value = serde_json::from_str(payload)
            .map_err(|error| ProviderError::Protocol(format!("chunk: {error}")))?;
        // OpenRouter reports mid-stream failures inline instead of breaking the connection.
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(|message| message.as_str())
                .unwrap_or("provider stream error");
            return Err(ProviderError::Api(message.to_string()));
        }

        let mut out = Vec::new();
        if let Some(usage) = value.get("usage").filter(|usage| !usage.is_null()) {
            let input_tokens = usage
                .get("prompt_tokens")
                .and_then(|tokens| tokens.as_u64())
                .unwrap_or(0);
            let output_tokens = usage
                .get("completion_tokens")
                .and_then(|tokens| tokens.as_u64())
                .unwrap_or(0);
            out.push(StreamEvent::Usage {
                input_tokens,
                output_tokens,
            });
        }

        let Some(choice) = value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .cloned()
        else {
            // A usage-only chunk has no choices; everything else with no choice is noise.
            return Ok(out);
        };

        let delta = choice.get("delta").cloned().unwrap_or_default();
        if let Some(text) = delta.get("content").and_then(|text| text.as_str()) {
            out.push(StreamEvent::Text(text.to_string()));
        }
        if let Some(calls) = delta.get("tool_calls").and_then(|calls| calls.as_array()) {
            for call in calls {
                let index = call
                    .get("index")
                    .and_then(|index| index.as_u64())
                    .unwrap_or(0) as usize;
                while self.slots.len() <= index {
                    self.slots
                        .push((String::new(), String::new(), String::new()));
                }
                let slot = &mut self.slots[index];
                if let Some(id) = call.get("id").and_then(|id| id.as_str()) {
                    // Some gateways repeat the id on every fragment; keep the first.
                    if slot.0.is_empty() {
                        slot.0 = id.to_string();
                    }
                }
                if let Some(function) = call.get("function") {
                    if let Some(name) = function.get("name").and_then(|name| name.as_str()) {
                        if slot.1.is_empty() {
                            slot.1 = name.to_string();
                        }
                    }
                    if let Some(arguments) = function.get("arguments").and_then(|a| a.as_str()) {
                        slot.2.push_str(arguments);
                    }
                }
            }
        }

        if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
            self.flush_tools(&mut out)?;
            self.finished = true;
            out.push(StreamEvent::Done {
                stop: StopReason::parse(reason),
            });
        }
        Ok(out)
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
    fn content_deltas_become_text() {
        let events = decode_all(&[
            r#"{"choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#,
            r#"{"choices":[{"index":0,"delta":{"content":" there"},"finish_reason":null}]}"#,
        ]);
        assert_eq!(
            events,
            vec![
                StreamEvent::Text("Hi".into()),
                StreamEvent::Text(" there".into())
            ]
        );
    }

    #[test]
    fn fragmented_tool_arguments_are_reassembled_into_one_call() {
        let events = decode_all(&[
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":""}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":\"a"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":".txt\"}"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
        ]);
        assert_eq!(
            events,
            vec![
                StreamEvent::ToolUse {
                    id: "call_1".into(),
                    name: "read_file".into(),
                    input: serde_json::json!({ "path": "a.txt" }),
                },
                StreamEvent::Done {
                    stop: StopReason::ToolUse
                },
            ]
        );
    }

    #[test]
    fn usage_is_reported_and_the_done_sentinel_ends_the_turn_once() {
        let events = decode_all(&[
            r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
            r#"{"usage":{"prompt_tokens":11,"completion_tokens":3}}"#,
            "[DONE]",
        ]);
        assert!(events.contains(&StreamEvent::Usage {
            input_tokens: 11,
            output_tokens: 3
        }));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, StreamEvent::Done { .. }))
                .count(),
            1,
            "only one Done even though finish_reason and [DONE] both arrived"
        );
    }

    #[test]
    fn an_inline_gateway_error_is_an_api_error() {
        let mut decoder = Decoder::new();
        let error = decoder
            .decode(r#"{"error":{"message":"insufficient credits","code":402}}"#)
            .unwrap_err();
        assert!(error.to_string().contains("insufficient credits"));
    }

    #[test]
    fn assistant_tool_calls_encode_as_a_tool_calls_array() {
        let messages = vec![
            Message::user("go"),
            Message::assistant(vec![
                MsgBlock::Text {
                    text: "reading".into(),
                },
                MsgBlock::ToolUse {
                    id: "call_1".into(),
                    name: "read_file".into(),
                    input: json!({ "path": "a.txt" }),
                },
            ]),
            Message::tool_result("call_1", "body", false),
        ];
        let encoded = encode_messages("sys", &messages);
        assert_eq!(encoded[0]["role"], "system");
        assert_eq!(encoded[2]["tool_calls"][0]["id"], "call_1");
        assert_eq!(encoded[2]["tool_calls"][0]["function"]["name"], "read_file");
        assert_eq!(encoded[3]["role"], "tool");
        assert_eq!(encoded[3]["tool_call_id"], "call_1");
    }

    #[test]
    fn a_failed_tool_result_is_marked_in_its_text() {
        let encoded = encode_messages("sys", &[Message::tool_result("call_1", "boom", true)]);
        assert_eq!(encoded[1]["content"], "ERROR: boom");
    }

    #[test]
    fn an_empty_system_prompt_is_not_sent() {
        let encoded = encode_messages("  ", &[Message::user("hi")]);
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0]["role"], "user");
    }

    #[test]
    fn the_endpoint_uses_the_catalog_base_url_including_v1() {
        let openai = crate::providers::catalog::find("openai").unwrap();
        assert_eq!(
            endpoint(&openai),
            "https://api.openai.com/v1/chat/completions"
        );
        let openrouter = crate::providers::catalog::find("openrouter").unwrap();
        assert_eq!(
            endpoint(&openrouter),
            "https://openrouter.ai/api/v1/chat/completions"
        );
    }

    #[test]
    fn reasoning_effort_is_sent_only_for_a_thinking_level() {
        let medium = body("gpt-5", "system", &[], &[], ThinkingLevel::Medium);
        assert_eq!(medium["reasoning_effort"], "medium");
        let max = body("gpt-5", "system", &[], &[], ThinkingLevel::Max);
        assert_eq!(max["reasoning_effort"], "max");
        let none = body("gpt-5", "system", &[], &[], ThinkingLevel::None);
        assert_eq!(none["reasoning_effort"], "none");
        let off = body("gpt-5", "system", &[], &[], ThinkingLevel::Off);
        assert!(off.get("reasoning_effort").is_none());
    }

    #[tokio::test]
    async fn a_one_shot_completion_sends_no_tools_or_history() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/chat/completions"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "index": 0, "message": { "role": "assistant", "content": "Fix the login bug" } }]
            })))
            .mount(&server)
            .await;
        let mut spec = crate::providers::catalog::find("openai").unwrap();
        spec.base_url = server.uri();
        let request = crate::providers::CompletionRequest {
            model: "gpt-5-mini".into(),
            system: "You name conversations.".into(),
            prompt: "First exchange".into(),
            max_tokens: 64,
        };

        let answer = complete(&reqwest::Client::new(), &spec, "test-key", &request)
            .await
            .expect("the first choice is the answer");
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
            2,
            "the system instruction and the one-shot prompt, no history"
        );
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(body.get("reasoning_effort").is_none(), "thinking is off");
        assert_eq!(
            sent[0].headers.get("authorization").unwrap(),
            "Bearer test-key"
        );
    }
}
