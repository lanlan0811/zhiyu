//! Data-directory resolution: everything lives under `~/.zhiyu/`.

use std::path::PathBuf;

use crate::data_dir_name;

/// The user-level data directory `~/.zhiyu`. Created on first use by the
/// caller. Falls back to the current directory when no home can be resolved
/// (defensive; both Windows and macOS resolve a home in practice).
pub fn data_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(data_dir_name()))
        .unwrap_or_else(|| PathBuf::from(data_dir_name()))
}

/// `~/.zhiyu/settings.json` — user/app settings (model defaults, thought
/// levels, compaction thresholds, UI prefs).
pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

/// `~/.zhiyu/app.json` — app state (last open session per mode, window prefs).
pub fn app_state_path() -> PathBuf {
    data_dir().join("app.json")
}

/// `~/.zhiyu/state.json` — daemon state (auth token, daemon port).
pub fn state_path() -> PathBuf {
    data_dir().join("state.json")
}

/// `~/.zhiyu/models.json` — user overrides of the built-in model catalogue and
/// custom models.
pub fn models_path() -> PathBuf {
    data_dir().join("models.json")
}

/// `~/.zhiyu/zhiyu.db` — the SQLite (WAL) store.
pub fn database_path() -> PathBuf {
    data_dir().join("zhiyu.db")
}

/// `~/.zhiyu/token` — the daemon auth token.
pub fn token_path() -> PathBuf {
    data_dir().join("token")
}

/// `~/.zhiyu/knowledge/` — the knowledge-base index directory.
pub fn knowledge_dir() -> PathBuf {
    data_dir().join("knowledge")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_paths_under_data_dir() {
        let base = data_dir();
        for p in [
            settings_path(),
            app_state_path(),
            state_path(),
            models_path(),
            database_path(),
            token_path(),
            knowledge_dir(),
        ] {
            assert!(p.starts_with(&base), "{p:?} not under {base:?}");
        }
    }
}
