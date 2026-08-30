//! `zhiyu-context` — context management engine.
//!
//! Resolves the model context window (`[1m]` suffix → 1M, default 200K),
//! tracks usage from every API response (ring buffer + 7-source breakdown),
//! compacts sessions with a summary message and timeline separator, and
//! guards model switching when the used context would exceed the target.

pub mod compact;
pub mod guard;
pub mod usage;
pub mod window;

pub use compact::{CompactionPlan, CompactionSeparator, compacted_transcript, plan_compaction, should_auto_compact};
pub use guard::{GuardResult, evaluate_switch};
pub use usage::UsageTracker;
pub use window::{DEFAULT_WINDOW, ONE_MILLION, resolve_context_window};

/// Default window when nothing is configured.
pub fn default_window() -> u64 {
    DEFAULT_WINDOW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_is_200k() {
        assert_eq!(default_window(), 200_000);
    }
}
