//! `zhiyu-core` — the core engine.
//!
//! Owns the daemon-side business logic: dual-mode session management
//! (`daily_sessions` / `coding_sessions`), SQLite/WAL persistence, the
//! coding-mode workspace (directory tree, file read/write, terminal),
//! turn-level git checkpoints with rollback, the skill library and the
//! daily-mode knowledge base.

pub mod builtin_models;
pub mod keyring;
pub mod model_store;
pub mod paths;

pub use builtin_models::{builtin_model, builtin_models, BUILTIN_MODEL_IDS};
pub use keyring::{KeyError, KeyStore};
pub use model_store::{ModelStore, ModelsFile};

/// The data directory name under the user's home.
pub fn data_dir_name() -> &'static str {
    ".zhiyu"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_is_zhiyu() {
        assert_eq!(data_dir_name(), ".zhiyu");
    }
}
