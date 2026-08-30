//! Data-directory resolution for the core crate. Everything lives under
//! `~/.zhiyu/`.

use std::path::PathBuf;

/// The user-level data directory `~/.zhiyu`.
pub fn data_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".zhiyu"))
        .unwrap_or_else(|| PathBuf::from(".zhiyu"))
}

/// `~/.zhiyu/models.json` — user overrides + custom models.
pub fn models_path() -> PathBuf {
    data_dir().join("models.json")
}

/// `~/.zhiyu/zhiyu.db` — SQLite (WAL) store.
pub fn database_path() -> PathBuf {
    data_dir().join("zhiyu.db")
}

/// `~/.zhiyu/settings.json`
pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

/// `~/.zhiyu/knowledge/`
pub fn knowledge_dir() -> PathBuf {
    data_dir().join("knowledge")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_paths_under_data_dir() {
        let base = data_dir();
        for p in [models_path(), database_path(), settings_path(), knowledge_dir()] {
            assert!(p.starts_with(&base));
        }
    }
}
