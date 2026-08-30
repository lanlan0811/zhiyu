//! Request body construction for the two OpenAI protocols.

use serde_json::{json, Value};
use zhiyu_protocol::{ApiFormat, ModelConfig, RequestPatch, ThoughtLevel};

use crate::thought_level::apply_patch;

/// A tool the driver can expose to the model.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON schema for the arguments.
    pub parameters: Value,
}

/// Builds the `chat/completions` request body.
pub fn build_chat_body(
    model: &ModelConfig,
    messages: &[ChatMessage],
    tools: &[ToolDef],
    thought_level: ThoughtLevel,
) -> Value {
    let mut body = json!({
        "model": name_or_id(model),
        "stream": true,
        "messages": messages.iter().map(chat_message_json).collect::<Vec<_>>(),
    });
    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(tool_json).collect::<Vec<_>>());
    }
    // apply the model's declared patch for this level; fall back to the
    // default effort mapping when the model declares no patch.
    let patch = model
        .reasoning
        .provider_options_by_level
        .get(thought_level.as_str())
        .cloned()
        .unwrap_or_else(|| crate::thought_level::chat_patch(thought_level));
    apply_patch(&mut body, &patch);
    body
}

/// Builds the `responses` request body.
pub fn build_responses_body(
    model: &ModelConfig,
    input: &[ChatMessage],
    tools: &[ToolDef],
    thought_level: ThoughtLevel,
) -> Value {
    let mut body = json!({
        "model": name_or_id(model),
        "stream": true,
        "input": input.iter().map(chat_message_json).collect::<Vec<_>>(),
    });
    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(tool_json).collect::<Vec<_>>());
    }
    let patch = model
        .reasoning
        .provider_options_by_level
        .get(thought_level.as_str())
        .cloned()
        .unwrap_or_else(|| crate::thought_level::responses_patch(thought_level));
    apply_patch(&mut body, &patch);
    body
}

/// A message in the driver's neutral representation.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: String, // "user" | "assistant" | "system" | "tool"
    pub content: String,
    /// Tool call id (for tool-role messages).
    pub tool_call_id: Option<String>,
    /// Tool calls in assistant messages.
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

fn chat_message_json(msg: &ChatMessage) -> Value {
    let mut v = json!({ "role": msg.role, "content": msg.content });
    if let Some(id) = &msg.tool_call_id {
        v["tool_call_id"] = json!(id);
    }
    if !msg.tool_calls.is_empty() {
        v["tool_calls"] = json!(msg
            .tool_calls
            .iter()
            .map(|tc| json!({
                "id": tc.id,
                "type": "function",
                "function": { "name": tc.name, "arguments": tc.arguments }
            }))
            .collect::<Vec<_>>());
    }
    v
}

fn name_or_id(model: &ModelConfig) -> &str {
    if model.name.is_empty() {
        &model.id
    } else {
        &model.name
    }
}

fn tool_json(tool: &ToolDef) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhiyu_protocol::ReasoningConfig;

    fn model(api_format: ApiFormat) -> ModelConfig {
        ModelConfig {
            id: "test-model".into(),
            vendor: "Test".into(),
            name: "test-model".into(),
            base_url: "https://example.com".into(),
            api_format,
            context_window: 200_000,
            max_output_tokens: 8000,
            reasoning: ReasoningConfig::default(),
            provider_key_id: None,
        }
    }

    fn messages() -> Vec<ChatMessage> {
        vec![ChatMessage {
            role: "user".into(),
            content: "hi".into(),
            tool_call_id: None,
            tool_calls: vec![],
        }]
    }

    #[test]
    fn chat_body_has_effort() {
        let m = model(ApiFormat::Chat);
        let body = build_chat_body(&m, &messages(), &[], ThoughtLevel::High);
        assert_eq!(body["reasoning_effort"], json!("high"));
        assert_eq!(body["model"], json!("test-model"));
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["messages"][0]["role"], json!("user"));
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn responses_body_nests_effort() {
        let m = model(ApiFormat::Responses);
        let body = build_responses_body(&m, &messages(), &[], ThoughtLevel::Max);
        assert_eq!(body["reasoning"]["effort"], json!("max"));
        assert_eq!(body["input"][0]["content"], json!("hi"));
    }

    #[test]
    fn chat_body_includes_tools() {
        let m = model(ApiFormat::Chat);
        let tools = vec![ToolDef {
            name: "search_knowledge".into(),
            description: "search".into(),
            parameters: json!({"type": "object", "properties": {}}),
        }];
        let body = build_chat_body(&m, &messages(), &tools, ThoughtLevel::Off);
        assert_eq!(body["tools"][0]["function"]["name"], json!("search_knowledge"));
        // off level must not carry reasoning_effort
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn model_declared_patch_wins() {
        let mut m = model(ApiFormat::Chat);
        m.reasoning.provider_options_by_level.insert(
            "high".into(),
            RequestPatch {
                set: vec![zhiyu_protocol::PathValue {
                    path: vec!["reasoning_effort".into()],
                    value: json!("custom"),
                }],
                unset: vec![],
            },
        );
        let body = build_chat_body(&m, &messages(), &[], ThoughtLevel::High);
        assert_eq!(body["reasoning_effort"], json!("custom"));
    }
}
