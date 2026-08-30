//! User/app settings persisted to the data directory (`~/.zhiyu/settings.json`).

use serde::{Deserialize, Serialize};

use crate::mode::Mode;
use crate::thought::ThoughtLevel;

/// Application-level settings. Per-mode fields may override the global
/// defaults; sessions may further override at runtime (ephemeral).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Default model id for each mode (fallback: the mode's default from the
    /// built-in catalogue).
    #[serde(default)]
    pub default_model: std::collections::BTreeMap<Mode, String>,
    /// Global default thought level.
    #[serde(default = "default_global_thought_level")]
    pub default_thought_level: ThoughtLevel,
    /// Per-mode default thought level (daily is mid-low, coding is mid-high).
    #[serde(default = "default_mode_thought_levels")]
    pub mode_thought_level: std::collections::BTreeMap<Mode, ThoughtLevel>,
    /// Auto-compact trigger: compact when used tokens cross this fraction of
    /// the context window (default 0.85).
    #[serde(default = "default_auto_compact_ratio")]
    pub auto_compact_ratio: f64,
    /// Compact target: stop compacting once used tokens fall below this
    /// fraction of the window (default 0.60).
    #[serde(default = "default_compact_target_ratio")]
    pub compact_target_ratio: f64,
    /// UI preferences.
    #[serde(default)]
    pub ui: UiPreferences,
}

fn default_global_thought_level() -> ThoughtLevel {
    ThoughtLevel::High
}

fn default_mode_thought_levels() -> std::collections::BTreeMap<Mode, ThoughtLevel> {
    let mut m = std::collections::BTreeMap::new();
    m.insert(Mode::Daily, ThoughtLevel::Medium);
    m.insert(Mode::Coding, ThoughtLevel::High);
    m
}

fn default_auto_compact_ratio() -> f64 {
    0.85
}

fn default_compact_target_ratio() -> f64 {
    0.60
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiPreferences {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_locale")]
    pub locale: String,
}

fn default_theme() -> String {
    "system".into()
}

fn default_locale() -> String {
    "zh-CN".into()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            default_model: std::collections::BTreeMap::new(),
            default_thought_level: default_global_thought_level(),
            mode_thought_level: default_mode_thought_levels(),
            auto_compact_ratio: default_auto_compact_ratio(),
            compact_target_ratio: default_compact_target_ratio(),
            ui: UiPreferences {
                theme: default_theme(),
                locale: default_locale(),
            },
        }
    }
}

impl Settings {
    /// Effective default thought level for a mode: per-mode override wins,
    /// else the global default.
    pub fn thought_level_for(&self, mode: Mode) -> ThoughtLevel {
        self.mode_thought_level
            .get(&mode)
            .copied()
            .unwrap_or(self.default_thought_level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.auto_compact_ratio, 0.85);
        assert_eq!(s.compact_target_ratio, 0.60);
        assert_eq!(s.thought_level_for(Mode::Daily), ThoughtLevel::Medium);
        assert_eq!(s.thought_level_for(Mode::Coding), ThoughtLevel::High);
    }

    #[test]
    fn round_trips_camel_case() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("autoCompactRatio"));
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
