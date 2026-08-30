//! The streaming API client: fires requests at the model endpoint and yields
//! parsed SSE chunks over a channel.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde_json::Value;
use tokio::sync::mpsc;
use zhiyu_protocol::{ApiFormat, ModelConfig, ThoughtLevel};

use crate::request::{build_chat_body, build_responses_body, ChatMessage, ToolDef};
use crate::sse::{parse_event, SseChunk};

#[derive(Debug, Clone, thiserror::Error)]
pub enum DriverError {
    #[error("http error: {0}")]
    Http(String),
    #[error("non-2xx status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("sse error: {0}")]
    Sse(#[from] crate::sse::SseError),
    #[error("stream ended without [DONE]")]
    StreamEnded,
    #[error("channel closed")]
    ChannelClosed,
}

impl From<reqwest::Error> for DriverError {
    fn from(e: reqwest::Error) -> Self {
        DriverError::Http(e.to_string())
    }
}

/// A single streamed API exchange: chunks flow on the returned receiver, then
/// the channel closes.
pub struct StreamHandle {
    pub rx: mpsc::Receiver<SseChunk>,
    pub task: tokio::task::JoinHandle<Result<(), DriverError>>,
}

/// Streams a completion for the given protocol. `base_url` + `api_key` come
/// from the model config and the keyring.
pub fn stream_completion(
    client: Client,
    model: &ModelConfig,
    api_key: &str,
    messages: &[ChatMessage],
    tools: &[ToolDef],
    thought_level: ThoughtLevel,
) -> StreamHandle {
    let (tx, rx) = mpsc::channel(64);
    let model = model.clone();
    let messages = messages.to_vec();
    let tools = tools.to_vec();
    let api_key = api_key.to_string();
    let task = tokio::spawn(async move {
        match model.api_format {
            ApiFormat::Chat => {
                let body = build_chat_body(&model, &messages, &tools, thought_level);
                stream_into(client, &model, &api_key, body, &tx).await
            }
            ApiFormat::Responses => {
                let body = build_responses_body(&model, &messages, &tools, thought_level);
                stream_into(client, &model, &api_key, body, &tx).await
            }
        }
    });
    StreamHandle { rx, task }
}

async fn stream_into(
    client: Client,
    model: &ModelConfig,
    api_key: &str,
    body: Value,
    tx: &mpsc::Sender<SseChunk>,
) -> Result<(), DriverError> {
    let url = format!("{}{}", model.base_url.trim_end_matches('/'), model.api_format.endpoint());
    let response = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(DriverError::Status { status: status.as_u16(), body });
    }

    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(bytes) = stream.next().await {
        let bytes = bytes?;
        buf.extend_from_slice(&bytes);
        // split complete event frames on blank-line boundaries
        while let Some(pos) = find_frame_end(&buf) {
            let frame = buf.drain(..pos).collect::<Vec<_>>();
            let payload = frame_payload(&frame);
            for chunk in parse_event(&payload, model.api_format)? {
                if tx.send(chunk).await.is_err() {
                    return Err(DriverError::ChannelClosed);
                }
            }
        }
    }
    // flush remaining
    if !buf.is_empty() {
        let payload = String::from_utf8_lossy(&buf).to_string();
        for chunk in parse_event(&payload, model.api_format)? {
            let _ = tx.send(chunk).await;
        }
    }
    Ok(())
}

/// Finds the byte offset of the first complete SSE frame (ending at the
/// blank line `\n\n`, handling `\r\n` too). Returns `None` when no complete
/// frame is buffered yet.
fn find_frame_end(buf: &[u8]) -> Option<usize> {
    let text = String::from_utf8_lossy(buf);
    for (i, _) in text.match_indices('\n') {
        let after = &text[i + 1..];
        if after.starts_with('\n') {
            return Some(i + 2);
        }
        if after.starts_with('\r') && after[1..].starts_with('\n') {
            return Some(i + 3);
        }
    }
    None
}

/// Extracts the `data:` payload lines of a frame, joined.
fn frame_payload(frame: &[u8]) -> String {
    let text = String::from_utf8_lossy(frame);
    let mut out = String::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(data) = line.strip_prefix("data:") {
            out.push_str(data.trim_start_matches(' '));
        }
    }
    out
}

/// A client with sane timeouts for local streaming.
pub fn default_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_splitting() {
        // "data: {\"a\":1}\n\n" is 15 bytes
        let buf = b"data: {\"a\":1}\n\ndata: {\"b\":2}\n\n";
        assert_eq!(find_frame_end(buf), Some(15));
        let (frame, rest) = buf.split_at(15);
        assert_eq!(frame_payload(frame), r#"{"a":1}"#);
        assert_eq!(find_frame_end(rest), Some(15));
    }

    #[test]
    fn frame_splitting_crlf() {
        // "data: {\"a\":1}\r\n\r\n" is 18 bytes
        let buf = b"data: {\"a\":1}\r\n\r\ndata: {\"b\":2}\r\n\r\n";
        assert_eq!(find_frame_end(buf), Some(18));
        assert_eq!(frame_payload(&buf[..18]), r#"{"a":1}"#);
    }

    #[test]
    fn incomplete_frame_returns_none() {
        let buf = b"data: {\"a\":1}\n";
        assert_eq!(find_frame_end(buf), None);
    }

    #[test]
    fn multi_line_data_is_joined() {
        let frame = b"data: {\"a\":\ndata: 1}\n\n";
        // both lines are data: → joined without the `data:` prefix
        assert_eq!(frame_payload(frame), "{\"a\":1}");
    }

    // ---- mock-API integration: a local HTTP server plays the model
    // endpoint and the driver streams against it (no real network).

    fn mock_server(
        body: String,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            // read headers + body (rough: read until the request is fully sent)
            let mut tmp = [0u8; 4096];
            loop {
                match socket.read(&mut tmp).await {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        // headers end with \r\n\r\n
                        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                            // read a little more for the body
                            let _ = socket.read(&mut tmp).await;
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
        });
        (addr, handle)
    }

    fn model_with_base(base_url: String, api_format: ApiFormat) -> ModelConfig {
        ModelConfig {
            id: "mock".into(),
            vendor: "Mock".into(),
            name: "mock".into(),
            base_url,
            api_format,
            context_window: 200_000,
            max_output_tokens: 8000,
            reasoning: zhiyu_protocol::ReasoningConfig::default(),
            provider_key_id: None,
        }
    }

    #[tokio::test]
    async fn streams_chat_against_mock() {
        let body = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi \"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"there\"}}]}\n\n",
            "data: {\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
            "data: [DONE]\n\n",
        );
        let (addr, srv) = mock_server(body.to_string());
        let model = model_with_base(format!("http://{addr}"), ApiFormat::Chat);
        let client = default_client();
        let handle = stream_completion(
            client,
            &model,
            "sk-test",
            &[ChatMessage { role: "user".into(), content: "hi".into(), tool_call_id: None, tool_calls: vec![] }],
            &[],
            ThoughtLevel::High,
        );
        let mut got = Vec::new();
        let mut rx = handle.rx;
        while let Some(chunk) = rx.recv().await {
            got.push(chunk);
        }
        handle.task.await.unwrap().unwrap();
        srv.await.unwrap();
        assert!(matches!(got[0], SseChunk::TextDelta(ref s) if s == "Hi "));
        assert!(matches!(got[1], SseChunk::TextDelta(ref s) if s == "there"));
        assert!(matches!(got[2], SseChunk::Usage(_)));
        assert!(matches!(got[3], SseChunk::Done));
    }

    #[tokio::test]
    async fn streams_responses_against_mock() {
        let body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"你好\"}\n\n",
            "data: {\"type\":\"response.completed\",\"usage\":{\"input_tokens\":4,\"output_tokens\":2,\"total_tokens\":6}}\n\n",
            "data: [DONE]\n\n",
        );
        let (addr, srv) = mock_server(body.to_string());
        let model = model_with_base(format!("http://{addr}"), ApiFormat::Responses);
        let client = default_client();
        let handle = stream_completion(
            client,
            &model,
            "sk-test",
            &[ChatMessage { role: "user".into(), content: "hi".into(), tool_call_id: None, tool_calls: vec![] }],
            &[],
            ThoughtLevel::Max,
        );
        let mut got = Vec::new();
        let mut rx = handle.rx;
        while let Some(chunk) = rx.recv().await {
            got.push(chunk);
        }
        handle.task.await.unwrap().unwrap();
        srv.await.unwrap();
        assert!(matches!(got[0], SseChunk::TextDelta(ref s) if s == "你好"));
        assert!(matches!(got[1], SseChunk::Usage(u) if u.total_tokens == 6));
        assert!(matches!(got[2], SseChunk::Done));
    }
}
