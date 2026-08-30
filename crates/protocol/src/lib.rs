//! 知屿 Zhīyǔ protocol crate — wire contracts shared by every layer.

pub mod context;
pub mod message;
pub mod mode;
pub mod model;
pub mod rpc;
pub mod settings;
pub mod thought;

pub use context::{ContextUsage, Usage, UsageSource};
pub use message::{Checkpoint, Message, Role, SessionCursor};
pub use mode::Mode;
pub use model::{ApiFormat, ModelConfig, ProviderKey, ProviderKeys};
pub use rpc::{
    Command, ErrorInfo, Event, Hello, HelloReply, Inbound, Outbound, PROTOCOL_VERSION, Request,
    Response, WritingKind, WritingTask,
};
pub use settings::Settings;
pub use thought::{PathValue, ReasoningConfig, RequestPatch, ThoughtLevel, ThoughtLevelSpec};

/// The wire protocol version this crate implements.
pub fn protocol_version() -> u32 {
    PROTOCOL_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_contract_compiles_and_round_trips() {
        // A representative end-to-end envelope: create session → reply → event.
        let req = Request {
            id: 7,
            command: Command::SessionCreate { mode: Mode::Coding, title: None, workspace_dir: Some(r"D:\proj".into()) },
        };
        let wire = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&wire).unwrap();
        match back.command {
            Command::SessionCreate { mode, workspace_dir, .. } => {
                assert_eq!(mode, Mode::Coding);
                assert_eq!(workspace_dir.as_deref(), Some(r"D:\proj"));
            }
            _ => panic!("wrong command"),
        }

        let ev = Event::Status { seq: 1, session_id: None, text: "hi".into() };
        let wire = serde_json::to_string(&ev).unwrap();
        let back: Event = serde_json::from_str(&wire).unwrap();
        assert_eq!(back.seq(), 1);
    }
}
