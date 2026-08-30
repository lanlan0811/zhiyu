//! `zhiyu-context` — context management engine.
//!
//! Resolves the model context window (`[1m]` suffix → 1M, default 200K),
//! tracks usage from every API response (ring buffer + 7-source breakdown),
//! compacts sessions with a summary message and timeline separator, and
//! guards model switching when the used context would exceed the target.

/// Placeholder for the M1 skeleton. Replaced in M6.
pub fn default_window() -> u64 {
    200_000
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_window_is_200k() {
        assert_eq!(super::default_window(), 200_000);
    }
}
