//! The two harness modes.

use serde::{Deserialize, Serialize};

/// The harness runs in one of two modes. Mode is a property of a session, not
/// of the app: the UI switches modes at the top, and each mode has its own
/// session list, history and data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Daily mode: knowledge base Q&A, AI writing, general chat, embedded
    /// browser research. No developer tools.
    Daily,
    /// Coding mode: project workspace, file read/write, terminal, git
    /// checkpoints, diff review, log/error analysis, web debugging.
    Coding,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Daily => "daily",
            Mode::Coding => "coding",
        }
    }

    /// Table name suffix for the per-mode session storage.
    pub fn session_table(self) -> &'static str {
        match self {
            Mode::Daily => "daily_sessions",
            Mode::Coding => "coding_sessions",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_round_trip() {
        for mode in [Mode::Daily, Mode::Coding] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: Mode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn session_tables_are_separate() {
        assert_ne!(Mode::Daily.session_table(), Mode::Coding.session_table());
    }
}
