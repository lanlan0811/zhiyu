//! SSE (Server-Sent Events) streaming parsing for both OpenAI protocols.
//!
//! `chat/completions` streams `data:` lines carrying `choices[].delta`.
//! `responses` streams `response.output_text.delta` (and friends) events.
//! Both end with a `data: [DONE]` sentinel.

/// A parsed stream chunk: text deltas, reasoning deltas, tool calls and the
/// final usage block.
#[derive(Debug, Clone, PartialEq)]
pub enum SseChunk {
    TextDelta(String),
    ReasoningDelta(String),
    /// Tool call delta (chat protocol): index, call id, name, args so far.
    ToolCallDelta {
        index: usize,
        call_id: Option<String>,
        name: Option<String>,
        arguments: String,
    },
    /// Tool call final args for the responses protocol.
    ToolCallArgs { call_id: String, arguments: String },
    Usage(zhiyu_protocol::Usage),
    Done,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SseError {
    #[error("malformed sse line: {0}")]
    Malformed(String),
    #[error("json error: {0}")]
    Json(String),
}

impl From<serde_json::Error> for SseError {
    fn from(e: serde_json::Error) -> Self {
        SseError::Json(e.to_string())
    }
}

/// Splits an SSE byte stream into `data:` payloads, skipping comments and
/// blank lines. `[DONE]` is yielded as-is.
fn sse_data_lines(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut current = String::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        if line.starts_with(':') {
            continue; // comment
        }
        if let Some(data) = line.strip_prefix("data:") {
            current.push_str(data.trim_start_matches(' '));
            current.push('\n');
        }
    }
    if !current.is_empty() {
        out.push(std::mem::take(&mut current));
    }
    out
}

/// Parses a single SSE `data:` payload (one JSON object or `[DONE]`) into
/// chunks for the given protocol. Used by the streaming client per event
/// frame. A single frame may produce zero or more chunks.
pub fn parse_event(payload: &str, api_format: ApiFormat) -> Result<Vec<SseChunk>, SseError> {
    let payload = payload.trim();
    if payload.is_empty() {
        return Ok(Vec::new());
    }
    if payload == "[DONE]" {
        return Ok(vec![SseChunk::Done]);
    }
    let json: serde_json::Value = serde_json::from_str(payload)?;
    let mut chunks = Vec::new();
    match api_format {
        ApiFormat::Chat => parse_chat_chunk(&json, &mut chunks)?,
        ApiFormat::Responses => parse_responses_chunk(&json, &mut chunks)?,
    }
    Ok(chunks)
}

/// Parses a full SSE body (already collected) into chunks for the given
/// protocol. In production the driver feeds `Bytes` frames incrementally;
/// this accumulates a single response body for tests and small responses.
pub fn parse_sse(body: &[u8], api_format: ApiFormat) -> Result<Vec<SseChunk>, SseError> {
    let mut chunks = Vec::new();
    for payload in sse_data_lines(body) {
        chunks.extend(parse_event(&payload, api_format)?);
    }
    Ok(chunks)
}

fn parse_chat_chunk(
    json: &serde_json::Value,
    chunks: &mut Vec<SseChunk>,
) -> Result<(), SseError> {
    // usage (final chunk)
    if let Some(usage) = json.get("usage") {
        chunks.push(SseChunk::Usage(usage_from_json(usage)));
    }
    let Some(choices) = json.get("choices").and_then(|c| c.as_array()) else {
        return Ok(());
    };
    for choice in choices {
        let Some(_index) = choice.get("index").and_then(|i| i.as_u64()) else {
            continue;
        };
        let Some(delta) = choice.get("delta") else { continue };
        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
            chunks.push(SseChunk::TextDelta(content.to_string()));
        }
        if let Some(reasoning) = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(|c| c.as_str())
        {
            chunks.push(SseChunk::ReasoningDelta(reasoning.to_string()));
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tool_calls {
                let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let call_id = tc.get("id").and_then(|v| v.as_str()).map(String::from);
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let arguments = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                chunks.push(SseChunk::ToolCallDelta { index: idx, call_id, name, arguments });
            }
        }
    }
    Ok(())
}

fn parse_responses_chunk(
    json: &serde_json::Value,
    chunks: &mut Vec<SseChunk>,
) -> Result<(), SseError> {
    let Some(etype) = json.get("type").and_then(|t| t.as_str()) else {
        return Ok(());
    };
    match etype {
        "response.output_text.delta" => {
            if let Some(delta) = json.get("delta").and_then(|d| d.as_str()) {
                chunks.push(SseChunk::TextDelta(delta.to_string()));
            }
        }
        "response.reasoning_summary_text.delta" => {
            if let Some(delta) = json.get("delta").and_then(|d| d.as_str()) {
                chunks.push(SseChunk::ReasoningDelta(delta.to_string()));
            }
        }
        "response.function_call_arguments.delta" => {
            if let (Some(call_id), Some(args)) = (
                json.get("item_id").and_then(|i| i.as_str()),
                json.get("delta").and_then(|d| d.as_str()),
            ) {
                chunks.push(SseChunk::ToolCallArgs { call_id: call_id.to_string(), arguments: args.to_string() });
            }
        }
        "response.completed" => {
            if let Some(usage) = json.get("usage") {
                chunks.push(SseChunk::Usage(usage_from_json(usage)));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Normalizes both camelCase and snake_case usage payloads.
pub fn usage_from_json(value: &serde_json::Value) -> zhiyu_protocol::Usage {
    fn num(v: &serde_json::Value, names: &[&str]) -> u64 {
        for n in names {
            if let Some(x) = v.get(*n).and_then(|x| x.as_u64()) {
                return x;
            }
        }
        0
    }
    let input = num(value, &["input_tokens", "inputTokens", "prompt_tokens", "promptTokens"]);
    let output = num(value, &["output_tokens", "outputTokens", "completion_tokens", "completionTokens"]);
    let reasoning = num(value, &["reasoning_tokens", "reasoningTokens"]);
    let cached_read = num(value, &["cached_read_tokens", "cachedReadTokens", "cached_input_tokens", "cachedInputTokens"]);
    let cached_write = num(value, &["cached_write_tokens", "cachedWriteTokens"]);
    let total = num(value, &["total_tokens", "totalTokens"])
        .max(input + output)
        .max(0);
    zhiyu_protocol::Usage {
        input_tokens: input,
        output_tokens: output,
        reasoning_tokens: reasoning,
        cached_read_tokens: cached_read,
        cached_write_tokens: cached_write,
        total_tokens: total,
    }
}

use zhiyu_protocol::model::ApiFormat;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_stream_deltas_and_usage() {
        let body = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"你\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"好\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"想想\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"f\",\"arguments\":\"{\\\"a\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"1}\"}}]}}]}\n\n",
            "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
            "data: [DONE]\n\n",
        );
        let chunks = parse_sse(body.as_bytes(), ApiFormat::Chat).unwrap();
        assert!(matches!(chunks[0], SseChunk::TextDelta(ref s) if s == "你"));
        assert!(matches!(chunks[1], SseChunk::TextDelta(ref s) if s == "好"));
        assert!(matches!(chunks[2], SseChunk::ReasoningDelta(ref s) if s == "想想"));
        match &chunks[3] {
            SseChunk::ToolCallDelta { index, call_id, name, arguments } => {
                assert_eq!(*index, 0);
                assert_eq!(call_id.as_deref(), Some("c1"));
                assert_eq!(name.as_deref(), Some("f"));
                assert_eq!(arguments, "{\"a\":");
            }
            _ => panic!("expected tool call delta"),
        }
        match &chunks[4] {
            SseChunk::ToolCallDelta { arguments, .. } => assert_eq!(arguments, "1}"),
            _ => panic!("expected tool call delta 2"),
        }
        match &chunks[5] {
            SseChunk::Usage(u) => {
                assert_eq!(u.input_tokens, 10);
                assert_eq!(u.output_tokens, 5);
                assert_eq!(u.total_tokens, 15);
            }
            _ => panic!("expected usage"),
        }
        assert!(matches!(chunks[6], SseChunk::Done));
    }

    #[test]
    fn responses_stream_deltas() {
        let body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\" there\"}\n\n",
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"r1\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_1\",\"delta\":\"{\\\"x\\\":\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_1\",\"delta\":\"1}\"}\n\n",
            "data: {\"type\":\"response.completed\",\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"reasoning_tokens\":1,\"total_tokens\":5}}\n\n",
            "data: [DONE]\n\n",
        );
        let chunks = parse_sse(body.as_bytes(), ApiFormat::Responses).unwrap();
        assert!(matches!(chunks[0], SseChunk::TextDelta(ref s) if s == "Hi"));
        assert!(matches!(chunks[1], SseChunk::TextDelta(ref s) if s == " there"));
        assert!(matches!(chunks[2], SseChunk::ReasoningDelta(ref s) if s == "r1"));
        match &chunks[3] {
            SseChunk::ToolCallArgs { call_id, arguments } => {
                assert_eq!(call_id, "fc_1");
                assert_eq!(arguments, "{\"x\":");
            }
            _ => panic!("expected tool call args"),
        }
        match &chunks[4] {
            SseChunk::ToolCallArgs { arguments, .. } => assert_eq!(arguments, "1}"),
            _ => panic!("expected tool call args 2"),
        }
        match &chunks[5] {
            SseChunk::Usage(u) => {
                assert_eq!(u.input_tokens, 3);
                assert_eq!(u.output_tokens, 2);
                assert_eq!(u.reasoning_tokens, 1);
            }
            _ => panic!("expected usage"),
        }
        assert!(matches!(chunks[6], SseChunk::Done));
    }

    #[test]
    fn usage_normalizes_camel_and_snake() {
        let snake = serde_json::json!({"input_tokens": 1, "output_tokens": 2, "total_tokens": 3});
        let u = usage_from_json(&snake);
        assert_eq!(u.input_tokens, 1);
        assert_eq!(u.output_tokens, 2);

        let camel = serde_json::json!({"inputTokens": 4, "outputTokens": 5, "totalTokens": 9});
        let u = usage_from_json(&camel);
        assert_eq!(u.input_tokens, 4);
        assert_eq!(u.total_tokens, 9);
    }

    #[test]
    fn done_and_comments_are_handled() {
        let body = ": keep-alive\n\ndata: [DONE]\n\n";
        let chunks = parse_sse(body.as_bytes(), ApiFormat::Chat).unwrap();
        assert_eq!(chunks, vec![SseChunk::Done]);
    }
}
