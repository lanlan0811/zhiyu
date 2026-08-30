//! Model configuration: the built-in model catalogue (overridable) and
//! user-defined custom models share one `ModelConfig` shape.

use serde::{Deserialize, Serialize};

use crate::thought::ReasoningConfig;

/// Which OpenAI-native protocol a model talks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiFormat {
    /// POST /chat/completions
    Chat,
    /// POST /responses
    Responses,
}

impl ApiFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ApiFormat::Chat => "chat/completions",
            ApiFormat::Responses => "responses",
        }
    }

    pub fn endpoint(self) -> &'static str {
        match self {
            ApiFormat::Chat => "/chat/completions",
            ApiFormat::Responses => "/responses",
        }
    }
}

/// A model the harness can drive directly via its OpenAI-compatible API.
///
/// Built-in models are constants in the core crate; the user can override any
/// field here (persisted to `~/.zhiyu/models.json`) and add custom models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    /// Stable id, e.g. `deepseek-v4-pro` or a uuid for custom models.
    pub id: String,
    pub vendor: String,
    pub name: String,
    /// OpenAI-compatible base URL, e.g. `https://api.deepseek.com`.
    pub base_url: String,
    pub api_format: ApiFormat,
    /// Context window in tokens. `[1m]` suffix on input resolves to 1M.
    pub context_window: u64,
    /// Max output tokens, an independent field from the window. The model
    /// switch guard uses `context_window - max_output_tokens`.
    pub max_output_tokens: u64,
    #[serde(default)]
    pub reasoning: ReasoningConfig,
    /// Key id in the keyring for this model's provider (shared per provider).
    #[serde(default)]
    pub provider_key_id: Option<String>,
}

impl ModelConfig {
    /// Tokens available for conversation history once output headroom is
    /// reserved. Used by the context manager's model-switch guard.
    pub fn usable_context(&self) -> u64 {
        self.context_window.saturating_sub(self.max_output_tokens)
    }
}

/// A key entry for a provider: supports multiple keys with rotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKey {
    pub id: String,
    /// The API key. Never written to plaintext storage; kept only in memory
    /// and encrypted at rest via DPAPI / keyring.
    pub key: String,
    #[serde(default)]
    pub is_default: bool,
}

/// A provider's key store: one provider owns a list of keys + the default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKeys {
    pub provider: String,
    pub keys: Vec<ProviderKey>,
    #[serde(default)]
    pub default_key_id: Option<String>,
}

impl ProviderKeys {
    pub fn default_key(&self) -> Option<&ProviderKey> {
        self.keys
            .iter()
            .find(|k| Some(k.id.as_str()) == self.default_key_id.as_deref())
            .or_else(|| self.keys.iter().find(|k| k.is_default))
            .or_else(|| self.keys.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ModelConfig {
        ModelConfig {
            id: "deepseek-v4-pro".into(),
            vendor: "DeepSeek".into(),
            name: "DeepSeek V4 Pro".into(),
            base_url: "https://api.deepseek.com".into(),
            api_format: ApiFormat::Chat,
            context_window: 1_000_000,
            max_output_tokens: 348_000,
            reasoning: ReasoningConfig {
                enabled: true,
                default_level: crate::thought::ThoughtLevel::High,
                levels: vec![],
                provider_options_by_level: Default::default(),
            },
            provider_key_id: Some("deepseek".into()),
        }
    }

    #[test]
    fn usable_context_reserves_output() {
        let m = sample();
        assert_eq!(m.usable_context(), 1_000_000 - 348_000);
    }

    #[test]
    fn config_round_trips_with_camel_case() {
        let m = sample();
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("contextWindow"));
        assert!(json.contains("maxOutputTokens"));
        assert!(json.contains("baseUrl"));
        let back: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn provider_keys_pick_default() {
        let p = ProviderKeys {
            provider: "deepseek".into(),
            keys: vec![
                ProviderKey { id: "k1".into(), key: "a".into(), is_default: false },
                ProviderKey { id: "k2".into(), key: "b".into(), is_default: true },
            ],
            default_key_id: Some("k2".into()),
        };
        assert_eq!(p.default_key().unwrap().id, "k2");
    }

    #[test]
    fn endpoints() {
        assert_eq!(ApiFormat::Chat.endpoint(), "/chat/completions");
        assert_eq!(ApiFormat::Responses.endpoint(), "/responses");
    }
}
