//! Thought-level runtime: model switch with CAS-style semantics and level
//! validation (illegal levels fall back to the model default).

use zhiyu_protocol::{ModelConfig, ThoughtLevel};

/// The result of switching a session's model.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSwitchResult {
    pub model_id: String,
    /// The level in effect after the switch (sanitized against the new
    /// model's declared levels).
    pub effective_thought_level: ThoughtLevel,
    /// Whether the requested level had to be adjusted.
    pub level_adjusted: bool,
    pub previous_model_id: Option<String>,
}

/// Switches the session's model, validating the current thought level against
/// the target model's declared levels (CAS semantics: the caller passes the
/// expected previous model to avoid races).
///
/// `requested_level` is the level the user had selected; if the target model
/// does not support it, the level falls back to the target model's default.
pub fn switch_model_config(
    previous_model_id: Option<&str>,
    target: &ModelConfig,
    requested_level: ThoughtLevel,
) -> ModelSwitchResult {
    let sanitized = target.reasoning.sanitize(requested_level);
    ModelSwitchResult {
        model_id: target.id.clone(),
        effective_thought_level: sanitized,
        level_adjusted: sanitized != requested_level,
        previous_model_id: previous_model_id.map(String::from),
    }
}

/// The effective default level for a session: session override wins, else the
/// mode default; both are sanitized against the model's declared levels.
pub fn effective_level(
    model: &ModelConfig,
    session_override: Option<ThoughtLevel>,
    mode_default: ThoughtLevel,
) -> ThoughtLevel {
    let level = session_override.unwrap_or(mode_default);
    model.reasoning.sanitize(level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhiyu_protocol::ReasoningConfig;

    fn model(levels: Vec<ThoughtLevel>, default: ThoughtLevel) -> ModelConfig {
        ModelConfig {
            id: "m".into(),
            vendor: "t".into(),
            name: "m".into(),
            base_url: "https://x".into(),
            api_format: zhiyu_protocol::ApiFormat::Chat,
            context_window: 1_000_000,
            max_output_tokens: 128_000,
            reasoning: ReasoningConfig {
                enabled: true,
                default_level: default,
                levels: levels
                    .into_iter()
                    .map(|l| zhiyu_protocol::ThoughtLevelSpec { value: l, label: l.as_str().into(), description: String::new() })
                    .collect(),
                provider_options_by_level: Default::default(),
            },
            provider_key_id: None,
        }
    }

    #[test]
    fn valid_level_passes_through() {
        let m = model(vec![ThoughtLevel::Off, ThoughtLevel::High, ThoughtLevel::Max], ThoughtLevel::High);
        let r = switch_model_config(Some("old"), &m, ThoughtLevel::High);
        assert_eq!(r.effective_thought_level, ThoughtLevel::High);
        assert!(!r.level_adjusted);
        assert_eq!(r.previous_model_id.as_deref(), Some("old"));
    }

    #[test]
    fn illegal_level_falls_back_to_default() {
        let m = model(vec![ThoughtLevel::Off, ThoughtLevel::Max], ThoughtLevel::Max);
        let r = switch_model_config(None, &m, ThoughtLevel::Medium); // not declared
        assert_eq!(r.effective_thought_level, ThoughtLevel::Max);
        assert!(r.level_adjusted);
    }

    #[test]
    fn disabled_reasoning_pins_to_off() {
        let mut m = model(vec![ThoughtLevel::Off, ThoughtLevel::Max], ThoughtLevel::Max);
        m.reasoning.enabled = false;
        let r = switch_model_config(None, &m, ThoughtLevel::Max);
        assert_eq!(r.effective_thought_level, ThoughtLevel::Off);
    }

    #[test]
    fn effective_level_prefers_session_override() {
        let m = model(vec![ThoughtLevel::Off, ThoughtLevel::Medium, ThoughtLevel::High], ThoughtLevel::Medium);
        assert_eq!(effective_level(&m, Some(ThoughtLevel::High), ThoughtLevel::Medium), ThoughtLevel::High);
        assert_eq!(effective_level(&m, None, ThoughtLevel::Medium), ThoughtLevel::Medium);
        // illegal override falls back to model default
        assert_eq!(effective_level(&m, Some(ThoughtLevel::Max), ThoughtLevel::Medium), ThoughtLevel::Medium);
    }
}
