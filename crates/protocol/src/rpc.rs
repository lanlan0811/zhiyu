//! The daemon wire protocol: Hello handshake, JSON-RPC request/response,
//! server events with sequence numbers for reconnect replay, and the full
//! command surface.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::message::{Message, SessionCursor};
use crate::mode::Mode;
use crate::model::ModelConfig;
use crate::thought::ThoughtLevel;

pub const PROTOCOL_VERSION: u32 = 1;

/// Client → daemon handshake. The token authenticates the local client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub protocol_version: u32,
    pub token: String,
    pub client_name: String,
    /// Highest event seq the client has seen; the daemon replays
    /// `last_seq + 1 … current` right after the handshake reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seq: Option<u64>,
}

/// Daemon → client handshake reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloReply {
    pub protocol_version: u32,
    pub server_name: String,
    /// Highest event sequence the daemon has emitted; the client replays
    /// from `last_seq + 1` after a reconnect.
    pub last_seq: u64,
    pub now: DateTime<Utc>,
}

/// The full command surface exposed over JSON-RPC. Each variant maps to a
/// `Request` whose params are the variant's fields (serialized camelCase).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum Command {
    // ---- sessions ---------------------------------------------------------
    /// List sessions of a mode.
    SessionList { mode: Mode },
    /// Create a session.
    SessionCreate { mode: Mode, title: Option<String>, workspace_dir: Option<String> },
    /// Open an existing session.
    SessionOpen { session_id: Uuid },
    /// Delete a session.
    SessionDelete { session_id: Uuid },
    /// Send a user message and start streaming a reply.
    SessionSend {
        session_id: Uuid,
        text: String,
        /// Optional transient thought-level override for this turn.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thought_level: Option<ThoughtLevel>,
    },
    /// Resume a session's stream from a cursor (replay after reconnect).
    SessionResume { cursor: SessionCursor },
    /// Steer the running turn with new instructions.
    SessionSteer { session_id: Uuid, text: String },
    /// Stop the running turn.
    SessionStop { session_id: Uuid },

    // ---- models & keys ----------------------------------------------------
    ModelList,
    ModelSave { config: ModelConfig },
    ModelDelete { model_id: String },
    KeyList { provider: Option<String> },
    KeySave { provider: String, key: String },
    KeyDelete { provider: String, key_id: String },
    KeySetDefault { provider: String, key_id: String },

    // ---- thought level ----------------------------------------------------
    SessionSetThoughtLevel { session_id: Uuid, level: ThoughtLevel },
    SettingsSetDefaultThoughtLevel { mode: Option<Mode>, level: ThoughtLevel },

    // ---- context management -----------------------------------------------
    SessionContextUsage { session_id: Uuid },
    SessionCompact { session_id: Uuid, instructions: Option<String> },
    ModelSwitchGuard { session_id: Uuid, model_id: String },

    // ---- knowledge base (daily mode) --------------------------------------
    KnowledgeSearch { query: String, limit: Option<u32> },
    KnowledgeImport { path: String },
    KnowledgeList,
    KnowledgeDelete { doc_id: Uuid },
    KnowledgeReindex,

    // ---- workspace (coding mode) ------------------------------------------
    WorkspaceOpen { session_id: Uuid, dir: String },
    WorkspaceListDir { session_id: Uuid, path: Option<String> },
    WorkspaceReadFile { session_id: Uuid, path: String },
    WorkspaceWriteFile { session_id: Uuid, path: String, content: String },
    TerminalExec { session_id: Uuid, command: String },
    GitStatus { session_id: Uuid },
    GitCheckpoint { session_id: Uuid, description: Option<String> },
    GitRollback { session_id: Uuid, checkpoint_id: Uuid },
    ReviewDiff { session_id: Uuid },

    // ---- browser -----------------------------------------------------------
    BrowserExecute { session_id: Uuid, request: serde_json::Value },

    // ---- writing (daily mode) ---------------------------------------------
    WritingRun { session_id: Uuid, task: WritingTask },

    // ---- settings ----------------------------------------------------------
    SettingsGet,
    SettingsSet { patch: serde_json::Value },
}

/// AI writing task types (daily mode).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WritingTask {
    pub kind: WritingKind,
    pub topic: String,
    /// Length / target-word guidance, e.g. "800 字".
    pub length: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WritingKind {
    Longform,
    Rewrite,
    Polish,
    Summarize,
    Translate,
    Outline,
}

/// JSON-RPC request envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub id: u64,
    pub command: Command,
}

/// JSON-RPC response envelope. `result` or `error` is set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    pub code: i32,
    pub message: String,
}

/// Server → client event, sequenced for replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A new message (or an update to the in-flight message) landed.
    Message { seq: u64, message: Message },
    /// Streaming delta of the assistant text.
    TextDelta { seq: u64, session_id: Uuid, delta: String },
    /// Streaming delta of reasoning text.
    ReasoningDelta { seq: u64, session_id: Uuid, delta: String },
    /// A tool invocation started (name + args).
    ToolStarted { seq: u64, session_id: Uuid, call_id: String, name: String, args: serde_json::Value },
    /// A tool invocation finished.
    ToolFinished { seq: u64, session_id: Uuid, call_id: String, ok: bool, output: String },
    /// Usage update after an API response (context manager input).
    UsageUpdate { seq: u64, session_id: Uuid, usage: crate::context::Usage },
    /// The turn ended.
    TurnFinished { seq: u64, session_id: Uuid, cursor: u64 },
    /// Session state changed (created / deleted / opened).
    SessionChanged { seq: u64, session_id: Uuid, mode: Mode },
    /// Generic status line (e.g. "connected", "compacted").
    Status { seq: u64, session_id: Option<Uuid>, text: String },
    /// A context compaction timeline marker was inserted.
    ContextCompacted {
        seq: u64,
        session_id: Uuid,
        trigger: String,
        pre_compact_tokens: u64,
        post_compact_tokens: u64,
    },
}

impl Event {
    pub fn seq(&self) -> u64 {
        match self {
            Event::Message { seq, .. }
            | Event::TextDelta { seq, .. }
            | Event::ReasoningDelta { seq, .. }
            | Event::ToolStarted { seq, .. }
            | Event::ToolFinished { seq, .. }
            | Event::UsageUpdate { seq, .. }
            | Event::TurnFinished { seq, .. }
            | Event::SessionChanged { seq, .. }
            | Event::Status { seq, .. }
            | Event::ContextCompacted { seq, .. } => *seq,
        }
    }
}

/// The full inbound frame: either the handshake or a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Inbound {
    Hello(Hello),
    Request(Request),
}

/// The full outbound frame: either the handshake reply or a response/event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Outbound {
    HelloReply(HelloReply),
    Response(Response),
    Event(Event),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips() {
        let req = Request {
            id: 1,
            command: Command::SessionCreate {
                mode: Mode::Daily,
                title: Some("写作".into()),
                workspace_dir: None,
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("sessionCreate"));
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn event_seq_accessor() {
        let e = Event::TextDelta { seq: 42, session_id: Uuid::new_v4(), delta: "a".into() };
        assert_eq!(e.seq(), 42);
    }

    #[test]
    fn hello_reply_carries_last_seq() {
        let r = HelloReply {
            protocol_version: PROTOCOL_VERSION,
            server_name: "zhiyu-daemon".into(),
            last_seq: 9,
            now: Utc::now(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: HelloReply = serde_json::from_str(&json).unwrap();
        assert_eq!(back.last_seq, 9);
    }

    #[test]
    fn inbound_untagged_discrimination() {
        let hello: Inbound = serde_json::from_str(
            r#"{"protocolVersion":1,"token":"t","clientName":"web","lastSeq":5}"#,
        )
        .unwrap();
        match hello {
            Inbound::Hello(h) => assert_eq!(h.last_seq, Some(5)),
            _ => panic!("expected hello"),
        }

        let req: Inbound =
            serde_json::from_str(r#"{"id":3,"command":{"method":"modelList"}}"#).unwrap();
        assert!(matches!(req, Inbound::Request(_)));
    }
}
