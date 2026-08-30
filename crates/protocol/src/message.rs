//! Session messages, cursors and checkpoints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The role of a message in a session transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// One message in a session transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: Role,
    /// Text content (markdown). Tool messages carry JSON here.
    pub content: String,
    /// Reasoning/thinking text streamed alongside assistant text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Tool name when `role == Tool`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Monotonic cursor within the session (0, 1, 2, …) used for resume.
    pub cursor: u64,
    pub created_at: DateTime<Utc>,
}

/// A session cursor: the last message sequence the client has seen, used to
/// resume a session after a reconnect or fork.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCursor {
    pub session_id: Uuid,
    /// Next expected message cursor; replay resumes from here.
    pub next_cursor: u64,
}

/// Turn-level git checkpoint metadata (coding mode).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub id: Uuid,
    pub session_id: Uuid,
    /// Git ref / commit hash the checkpoint points at.
    pub ref_name: String,
    /// Short human description, e.g. the last user turn.
    pub description: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_round_trips() {
        let m = Message {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            role: Role::Assistant,
            content: "你好".into(),
            reasoning: Some("思考中".into()),
            tool_name: None,
            cursor: 7,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert!(json.contains("sessionId"));
        assert!(json.contains("createdAt"));
    }

    #[test]
    fn cursor_replay_semantics() {
        let c = SessionCursor { session_id: Uuid::new_v4(), next_cursor: 12 };
        assert_eq!(c.next_cursor, 12);
    }
}
