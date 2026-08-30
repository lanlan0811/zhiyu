//! `zhiyu-driver` — OpenAI dual-protocol driver.
//!
//! Implements direct API calls to the model endpoint over the two OpenAI
//! native protocols (`chat/completions` and `responses`), SSE streaming,
//! tool calling, usage reporting and the thought-level patch system.

pub mod client;
pub mod request;
pub mod sse;
pub mod thought_level;

pub use client::{default_client, stream_completion, DriverError, StreamHandle};
pub use request::{build_chat_body, build_responses_body, ChatMessage, ToolCall, ToolDef};
pub use sse::{parse_event, parse_sse, SseChunk, SseError};
pub use thought_level::{apply_patch, chat_patch, responses_patch};

/// The two OpenAI-native protocols this driver speaks.
pub fn protocol_support() -> &'static [&'static str] {
    &["chat/completions", "responses"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_both_openai_protocols() {
        let p = protocol_support();
        assert!(p.contains(&"chat/completions"));
        assert!(p.contains(&"responses"));
    }
}
