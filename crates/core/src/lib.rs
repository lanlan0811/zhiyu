//! `zhiyu-core` — the core engine.
//!
//! Owns the daemon-side business logic: dual-mode session management
//! (`daily_sessions` / `coding_sessions`), SQLite/WAL persistence, the
//! coding-mode workspace (directory tree, file read/write, terminal),
//! turn-level git checkpoints with rollback, the skill library and the
//! daily-mode knowledge base.

/// Placeholder for the M1 skeleton. Replaced in M4/M5.
pub fn data_dir_name() -> &'static str {
    ".zhiyu"
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_dir_is_zhiyu() {
        assert_eq!(super::data_dir_name(), ".zhiyu");
    }
}
