//! Model thought-level system: six levels plus the per-level patch table.

use serde::{Deserialize, Serialize};

/// The six thought levels. `off` disables reasoning entirely; `max` requests
/// the strongest reasoning the model supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThoughtLevel {
    Off,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ThoughtLevel {
    pub const ALL: [ThoughtLevel; 6] = [
        ThoughtLevel::Off,
        ThoughtLevel::Low,
        ThoughtLevel::Medium,
        ThoughtLevel::High,
        ThoughtLevel::Xhigh,
        ThoughtLevel::Max,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ThoughtLevel::Off => "off",
            ThoughtLevel::Low => "low",
            ThoughtLevel::Medium => "medium",
            ThoughtLevel::High => "high",
            ThoughtLevel::Xhigh => "xhigh",
            ThoughtLevel::Max => "max",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "off" => Some(ThoughtLevel::Off),
            "low" => Some(ThoughtLevel::Low),
            "medium" => Some(ThoughtLevel::Medium),
            "high" => Some(ThoughtLevel::High),
            "xhigh" => Some(ThoughtLevel::Xhigh),
            "max" => Some(ThoughtLevel::Max),
            _ => None,
        }
    }

    /// Next level in the cyclic order (off → low → … → max → off).
    pub fn next(self) -> Self {
        match self {
            ThoughtLevel::Off => ThoughtLevel::Low,
            ThoughtLevel::Low => ThoughtLevel::Medium,
            ThoughtLevel::Medium => ThoughtLevel::High,
            ThoughtLevel::High => ThoughtLevel::Xhigh,
            ThoughtLevel::Xhigh => ThoughtLevel::Max,
            ThoughtLevel::Max => ThoughtLevel::Off,
        }
    }
}

/// A path-based patch: set/unset JSON paths inside the API request body.
/// Mirrors the ZCode provider-options patch system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPatch {
    /// Paths to set, e.g. `["reasoning", "effort"] = "high"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub set: Vec<PathValue>,
    /// Paths to remove from the request body entirely.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unset: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathValue {
    pub path: Vec<String>,
    pub value: serde_json::Value,
}

/// A declared thought level for a model: value + label + description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtLevelSpec {
    pub value: ThoughtLevel,
    pub label: String,
    pub description: String,
}

/// The reasoning capability declaration of a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningConfig {
    pub enabled: bool,
    #[serde(default)]
    pub default_level: ThoughtLevel,
    /// Levels this model supports, in display order.
    #[serde(default)]
    pub levels: Vec<ThoughtLevelSpec>,
    /// Per-level request patches, keyed by level name (off/low/medium/…).
    #[serde(default)]
    pub provider_options_by_level: std::collections::BTreeMap<String, RequestPatch>,
}

impl ReasoningConfig {
    /// Normalizes `level` into a supported level: falls back to the default
    /// when the model does not declare the requested level.
    pub fn sanitize(&self, level: ThoughtLevel) -> ThoughtLevel {
        if !self.enabled {
            return ThoughtLevel::Off;
        }
        if self.levels.iter().any(|l| l.value == level) {
            level
        } else {
            self.default_level
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_cycle() {
        let mut l = ThoughtLevel::Off;
        for expected in [ThoughtLevel::Low, ThoughtLevel::Medium, ThoughtLevel::High, ThoughtLevel::Xhigh, ThoughtLevel::Max, ThoughtLevel::Off] {
            l = l.next();
            assert_eq!(l, expected);
        }
    }

    #[test]
    fn sanitize_falls_back_to_default() {
        let cfg = ReasoningConfig {
            enabled: true,
            default_level: ThoughtLevel::Medium,
            levels: vec![
                ThoughtLevelSpec { value: ThoughtLevel::Off, label: "off".into(), description: String::new() },
                ThoughtLevelSpec { value: ThoughtLevel::Medium, label: "medium".into(), description: String::new() },
            ],
            provider_options_by_level: Default::default(),
        };
        assert_eq!(cfg.sanitize(ThoughtLevel::High), ThoughtLevel::Medium);
        assert_eq!(cfg.sanitize(ThoughtLevel::Off), ThoughtLevel::Off);
    }

    #[test]
    fn disabled_means_off() {
        let cfg = ReasoningConfig {
            enabled: false,
            default_level: ThoughtLevel::Max,
            levels: vec![],
            provider_options_by_level: Default::default(),
        };
        assert_eq!(cfg.sanitize(ThoughtLevel::Max), ThoughtLevel::Off);
    }
}
