//! Google Generative Language: request encoding and stream decoding.
//!
//! Google differs from the other two in ways that matter here:
//! `contents[].parts[]` instead of a message list, `functionCall`/`functionResponse` instead of tool
//! calls and tool results, and a **whole** function call in one chunk rather than fragmented
//! arguments. A `functionResponse` is addressed by function *name*, which is why the encoder carries
//! the id → name map built from the assistant turn.

use super::tools::ToolSpec;
use super::wire::{
    api_error, authorize, Message, MsgBlock, MsgRole, StopReason, StreamDecoder, StreamEvent,
};
use super::{CompletionRequest, ProviderError, ProviderSpec, ThinkingLevel};
use serde_json::json;

/// One turn's output ceiling. The largest thinking budget below leaves room for a final answer
/// inside the ceiling without raising the ceiling past a model's own output limit.
pub const MAX_OUTPUT_TOKENS: u32 = 8_192;

/// `POST {base}/v1beta/models/{model}:streamGenerateContent?alt=sse`.
///
/// `alt=sse` is what makes the streaming response a usable event stream rather than a JSON array.
pub fn endpoint(spec: &ProviderSpec, model: &str) -> String {
    format!(
        "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
        spec.base_url.trim_end_matches('/'),
        model
    )
}

pub fn body(
    system: &str,
    messages: &[Message],
    tools: &[ToolSpec],
    thinking: ThinkingLevel,
) -> serde_json::Value {
    let mut generation = json!({ "maxOutputTokens": MAX_OUTPUT_TOKENS });
    if let Some(budget) = match thinking {
        ThinkingLevel::Off => None,
        // Gemini uses a zero budget for an explicit off; omitting the config can still reason.
        ThinkingLevel::None => Some(0),
        ThinkingLevel::Minimal => Some(512),
        ThinkingLevel::Low => Some(1_024),
        ThinkingLevel::Medium => Some(2_048),
        ThinkingLevel::High => Some(4_096),
        ThinkingLevel::XHigh => Some(6_144),
        ThinkingLevel::Max => Some(7_168),
    } {
        generation["thinkingConfig"] = json!({ "thinkingBudget": budget });
    }
    json!({
        "systemInstruction": { "parts": [{ "text": system }] },
        "contents": encode_contents(messages),
        "tools": [{
            "functionDeclarations": tools
                .iter()
                .map(|tool| json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }))
                .collect::<Vec<_>>(),
        }],
        "generationConfig": generation,
    })
}

fn encode_contents(messages: &[Message]) -> Vec<serde_json::Value> {
    // Google addresses a tool result by the function's name, so remember every call id we sent.
    let mut names: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut contents: Vec<serde_json::Value> = Vec::new();
    for message in messages {
        let role = match message.role {
            MsgRole::Assistant => "model",
            MsgRole::User | MsgRole::Tool => "user",
        };
        let parts: Vec<serde_json::Value> = message
            .blocks
            .iter()
            .map(|block| match block {
                MsgBlock::Text { text } => json!({ "text": text }),
                MsgBlock::ToolUse { id, name, input } => {
                    names.insert(id.clone(), name.clone());
                    json!({ "functionCall": { "name": name, "args": input } })
                }
                MsgBlock::ToolResult {
                    id,
                    content,
                    is_error,
                } => {
                    let name = names.get(id).cloned().unwrap_or_else(|| id.clone());
                    json!({
                        "functionResponse": {
                            "name": name,
                            "response": {
                                // The API requires an object here; the model reads `content`.
                                "content": content,
                                "error": is_error,
                            }
                        }
                    })
                }
            })
            .collect();
        if parts.is_empty() {
            continue;
        }
        match contents.last_mut() {
            Some(last) if last["role"] == json!(role) => {
                let merged = last["parts"].as_array_mut();
                if let Some(merged) = merged {
                    merged.extend(parts);
                }
            }
            _ => contents.push(json!({ "role": role, "parts": parts })),
        }
    }
    contents
}

/// `POST {base}/v1beta/models/{model}:generateContent` for a one-shot answer.
///
/// The stream variant asks for `alt=sse`; a title call wants one JSON body, so it uses the plain
/// method name.
pub fn generate_endpoint(spec: &ProviderSpec, model: &str) -> String {
    format!(
        "{}/v1beta/models/{}:generateContent",
        spec.base_url.trim_end_matches('/'),
        model
    )
}

/// One non-streaming request with no tools and no history: what a session title needs.
pub async fn complete(
    http: &reqwest::Client,
    spec: &ProviderSpec,
    key: &str,
    request: &CompletionRequest,
) -> Result<String, ProviderError> {
    let body = json!({
        "systemInstruction": { "parts": [{ "text": request.system }] },
        "contents": [{ "role": "user", "parts": [{ "text": request.prompt }] }],
        "generationConfig": {
            "maxOutputTokens": request.max_tokens,
            // Off for this call: a title is a few words, and reasoning tokens would eat the 64-token
            // budget before an answer is written.
            "thinkingConfig": { "thinkingBudget": 0 },
        },
    });
    let response = authorize(
        http.post(generate_endpoint(spec, &request.model)),
        super::Wire::Google,
        key,
    )
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
        .get("candidates")
        .and_then(|candidates| candidates.get(0))
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.as_array())
        .and_then(|parts| {
            parts.iter().find_map(|part| {
                let thought = part
                    .get("thought")
                    .and_then(|thought| thought.as_bool())
                    .unwrap_or(false);
                (!thought)
                    .then(|| part.get("text").and_then(|text| text.as_str()))
                    .flatten()
            })
        })
        .map(str::to_string)
        .ok_or_else(|| {
            ProviderError::Protocol(format!("{} completion returned no text", spec.name))
        })
}

/// Decodes `text/event-stream` chunks from `:streamGenerateContent`.
#[derive(Default)]
pub struct Decoder {
    finished: bool,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StreamDecoder for Decoder {
    fn decode(&mut self, payload: &str) -> Result<Vec<StreamEvent>, ProviderError> {
        let value: serde_json::Value = serde_json::from_str(payload)
            .map_err(|error| ProviderError::Protocol(format!("google chunk: {error}")))?;
        let mut out = Vec::new();

        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(|message| message.as_str())
                .unwrap_or("google stream error");
            return Err(ProviderError::Api(message.to_string()));
        }

        if let Some(usage) = value.get("usageMetadata") {
            out.push(StreamEvent::Usage {
                input_tokens: usage
                    .get("promptTokenCount")
                    .and_then(|tokens| tokens.as_u64())
                    .unwrap_or(0),
                output_tokens: usage
                    .get("candidatesTokenCount")
                    .and_then(|tokens| tokens.as_u64())
                    .unwrap_or(0),
            });
        }

        let Some(candidate) = value
            .get("candidates")
            .and_then(|candidates| candidates.get(0))
        else {
            return Ok(out);
        };

        let parts = candidate
            .get("content")
            .and_then(|content| content.get("parts"))
            .and_then(|parts| parts.as_array())
            .cloned()
            .unwrap_or_default();
        let mut saw_call = false;
        for part in parts {
            // `thought: true` marks the model's internal reasoning summary. It is not the answer, and
            // putting it in the assistant bubble reads as prose the model never said.
            let thought = part
                .get("thought")
                .and_then(|thought| thought.as_bool())
                .unwrap_or(false);
            if let Some(text) = part.get("text").and_then(|text| text.as_str()) {
                if !thought {
                    out.push(StreamEvent::Text(text.to_string()));
                }
            }
            if let Some(call) = part.get("functionCall") {
                let name = call
                    .get("name")
                    .and_then(|name| name.as_str())
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                saw_call = true;
                // No id is sent; the name is a stable enough handle for one turn, and the loop pairs
                // the result with the name too.
                out.push(StreamEvent::ToolUse {
                    id: format!("call_{}", out.len()),
                    name,
                    input: call.get("args").cloned().unwrap_or_else(|| json!({})),
                });
            }
        }

        if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
            self.finished = true;
            let stop = if saw_call {
                StopReason::ToolUse
            } else {
                StopReason::parse(reason)
            };
            out.push(StreamEvent::Done { stop });
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
    fn text_parts_become_text_events() {
        let events = decode_all(&[
            r#"{"candidates":[{"content":{"parts":[{"text":"Hel"}],"role":"model"}}]}"#,
            r#"{"candidates":[{"content":{"parts":[{"text":"lo"}],"role":"model"}}]}"#,
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
    fn a_function_call_arrives_whole_with_parsed_args() {
        let events = decode_all(&[
            r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"read_file","args":{"path":"a.txt"}}}],"role":"model"},"finishReason":"STOP"}]}"#,
        ]);
        assert_eq!(
            events[0],
            StreamEvent::ToolUse {
                id: "call_0".into(),
                name: "read_file".into(),
                input: serde_json::json!({ "path": "a.txt" }),
            }
        );
        assert_eq!(
            events[1],
            StreamEvent::Done {
                stop: StopReason::ToolUse
            },
            "a function call means the loop continues, whatever the vendor's finish reason says"
        );
    }

    #[test]
    fn a_plain_stop_ends_the_turn() {
        let events = decode_all(&[
            r#"{"candidates":[{"content":{"parts":[{"text":"done"}],"role":"model"},"finishReason":"STOP"}]}"#,
        ]);
        assert_eq!(
            events.last(),
            Some(&StreamEvent::Done {
                stop: StopReason::EndTurn
            })
        );
    }

    #[test]
    fn usage_metadata_is_reported() {
        let events = decode_all(&[
            r#"{"usageMetadata":{"promptTokenCount":9,"candidatesTokenCount":4,"totalTokenCount":13}}"#,
        ]);
        assert_eq!(
            events,
            vec![StreamEvent::Usage {
                input_tokens: 9,
                output_tokens: 4
            }]
        );
    }

    #[test]
    fn an_error_body_is_an_api_error() {
        let mut decoder = Decoder::new();
        let error = decoder
            .decode(r#"{"error":{"code":400,"message":"API key not valid"}}"#)
            .unwrap_err();
        assert!(error.to_string().contains("API key not valid"));
    }

    #[test]
    fn a_tool_result_is_addressed_by_function_name_not_call_id() {
        let messages = vec![
            Message::user("read"),
            Message::assistant(vec![MsgBlock::ToolUse {
                id: "call_0".into(),
                name: "read_file".into(),
                input: json!({ "path": "a.txt" }),
            }]),
            Message::tool_result("call_0", "contents", false),
        ];
        let contents = encode_contents(&messages);
        let response = &contents[2]["parts"][0]["functionResponse"];
        assert_eq!(response["name"], "read_file");
        assert_eq!(response["response"]["content"], "contents");
        assert_eq!(response["response"]["error"], false);
    }

    #[test]
    fn assistant_turns_encode_with_the_model_role() {
        let contents = encode_contents(&[Message::assistant(vec![MsgBlock::Text {
            text: "hi".into(),
        }])]);
        assert_eq!(contents[0]["role"], "model");
    }

    #[test]
    fn the_endpoint_carries_the_model_and_requests_sse() {
        let spec = crate::providers::catalog::find("google").unwrap();
        assert_eq!(
            endpoint(&spec, "gemini-2.5-pro"),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn thinking_budget_is_only_sent_when_requested() {
        let high = body("system", &[], &[], ThinkingLevel::High);
        assert_eq!(
            high["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            4_096
        );
        let max = body("system", &[], &[], ThinkingLevel::Max);
        assert_eq!(
            max["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            7_168
        );
        let off = body("system", &[], &[], ThinkingLevel::Off);
        assert!(off["generationConfig"].get("thinkingConfig").is_none());
        let none = body("system", &[], &[], ThinkingLevel::None);
        assert_eq!(
            none["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            0
        );
    }

    #[tokio::test]
    async fn a_one_shot_completion_sends_no_tools_or_history() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(
                "/v1beta/models/gemini-2.5-flash:generateContent",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{
                    "content": { "parts": [{ "text": "Fix the login bug" }], "role": "model" }
                }]
            })))
            .mount(&server)
            .await;
        let mut spec = crate::providers::catalog::find("google").unwrap();
        spec.base_url = server.uri();
        let request = crate::providers::CompletionRequest {
            model: "gemini-2.5-flash".into(),
            system: "You name conversations.".into(),
            prompt: "First exchange".into(),
            max_tokens: 64,
        };

        let answer = complete(&reqwest::Client::new(), &spec, "test-key", &request)
            .await
            .expect("the first candidate's text is the answer");
        assert_eq!(answer, "Fix the login bug");

        let sent = server.received_requests().await.unwrap();
        assert_eq!(sent.len(), 1, "one request, not a session");
        let body: serde_json::Value = serde_json::from_slice(&sent[0].body).unwrap();
        assert!(
            body.get("tools").is_none(),
            "the title prompt must not see tools"
        );
        assert_eq!(
            body["contents"].as_array().unwrap().len(),
            1,
            "only the one-shot prompt, no conversation history"
        );
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 64);
        assert_eq!(
            body["generationConfig"]["thinkingConfig"]["thinkingBudget"], 0,
            "thinking is explicitly off for the title call"
        );
        assert_eq!(sent[0].headers.get("x-goog-api-key").unwrap(), "test-key");
    }
}
