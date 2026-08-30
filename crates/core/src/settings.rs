//! Settings persistence: `~/.zhiyu/settings.json` with defaults.

use std::path::PathBuf;

use zhiyu_protocol::{Mode, Settings, ThoughtLevel};

use crate::paths::settings_path;

/// Loads settings, falling back to defaults when missing or corrupt.
pub fn load_settings(path: Option<&PathBuf>) -> Settings {
    let path = path.cloned().unwrap_or_else(settings_path);
    if !path.exists() {
        return Settings::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Saves settings.
pub fn save_settings(path: Option<&PathBuf>, settings: &Settings) -> anyhow::Result<()> {
    let path = path.cloned().unwrap_or_else(settings_path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

/// Applies a JSON patch (merge) onto settings and persists.
pub fn patch_settings(path: Option<&PathBuf>, patch: serde_json::Value) -> anyhow::Result<Settings> {
    let mut settings = load_settings(path);
    // merge patch object onto the settings value
    if let serde_json::Value::Object(map) = patch {
        let mut value = serde_json::to_value(&settings)?;
        if let serde_json::Value::Object(target) = &mut value {
            for (k, v) in map {
                target.insert(k, v);
            }
        }
        settings = serde_json::from_value(value)?;
    }
    save_settings(path, &settings)?;
    Ok(settings)
}

/// The effective default model id for a mode: per-mode setting wins, else a
/// sensible built-in default.
pub fn default_model_for(settings: &Settings, mode: Mode) -> String {
    settings
        .default_model
        .get(&mode)
        .cloned()
        .unwrap_or_else(|| match mode {
            Mode::Daily => "deepseek-v4-flash".into(),
            Mode::Coding => "deepseek-v4-pro".into(),
        })
}

/// The effective default thought level for a mode.
pub fn default_thought_level_for(settings: &Settings, mode: Mode) -> ThoughtLevel {
    settings.thought_level_for(mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let s = load_settings(Some(&path));
        assert_eq!(s.auto_compact_ratio, 0.85);
        assert_eq!(default_thought_level_for(&s, Mode::Coding), ThoughtLevel::High);
        assert_eq!(default_model_for(&s, Mode::Daily), "deepseek-v4-flash");
    }

    #[test]
    fn save_and_patch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        save_settings(Some(&path), &Settings::default()).unwrap();

        let patched = patch_settings(
            Some(&path),
            serde_json::json!({ "autoCompactRatio": 0.9, "defaultModel": { "coding": "glm-5.3" } }),
        )
        .unwrap();
        assert_eq!(patched.auto_compact_ratio, 0.9);
        assert_eq!(default_model_for(&patched, Mode::Coding), "glm-5.3");
        assert_eq!(default_model_for(&patched, Mode::Daily), "deepseek-v4-flash");

        // reload from disk
        let reloaded = load_settings(Some(&path));
        assert_eq!(reloaded.auto_compact_ratio, 0.9);
    }
}
