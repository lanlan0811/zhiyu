//! Context-usage tracking types: per-response usage, the ring-buffer usage
//! summary and the 7-source breakdown.

use serde::{Deserialize, Serialize};

/// Token usage parsed out of an API response (either protocol). Field names
/// are normalized here; the driver accepts both camelCase and snake_case
/// payloads from the wire.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(alias = "input_tokens")]
    pub input_tokens: u64,
    #[serde(alias = "output_tokens")]
    pub output_tokens: u64,
    #[serde(default, alias = "reasoning_tokens")]
    pub reasoning_tokens: u64,
    #[serde(default, alias = "cached_read_tokens")]
    pub cached_read_tokens: u64,
    #[serde(default, alias = "cached_write_tokens")]
    pub cached_write_tokens: u64,
    #[serde(alias = "total_tokens")]
    pub total_tokens: u64,
}

/// The 7 sources whose tokens add up to the session's used context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    SystemPrompt,
    MetaUserContext,
    Skills,
    ToolPrompt,
    SystemToolSchemas,
    McpToolSchemas,
    Messages,
}

impl UsageSource {
    pub const ALL: [UsageSource; 7] = [
        UsageSource::SystemPrompt,
        UsageSource::MetaUserContext,
        UsageSource::Skills,
        UsageSource::ToolPrompt,
        UsageSource::SystemToolSchemas,
        UsageSource::McpToolSchemas,
        UsageSource::Messages,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            UsageSource::SystemPrompt => "system_prompt",
            UsageSource::MetaUserContext => "meta_user_context",
            UsageSource::Skills => "skills",
            UsageSource::ToolPrompt => "tool_prompt",
            UsageSource::SystemToolSchemas => "system_tool_schemas",
            UsageSource::McpToolSchemas => "mcp_tool_schemas",
            UsageSource::Messages => "messages",
        }
    }
}

/// The live context usage of a session, shown as a ring in the UI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    pub used_tokens: u64,
    pub size_tokens: u64,
    pub max_tokens: u64,
    /// Per-source breakdown; `used_tokens` is the sum.
    #[serde(default)]
    pub breakdown: std::collections::BTreeMap<String, u64>,
}

impl ContextUsage {
    pub fn percent(&self) -> f64 {
        if self.max_tokens == 0 {
            0.0
        } else {
            self.used_tokens as f64 / self.max_tokens as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_normalized_fields() {
        let u: Usage = serde_json::from_str(
            r#"{"input_tokens":10,"output_tokens":5,"total_tokens":15}"#,
        )
        .unwrap();
        assert_eq!(u.input_tokens, 10);
        assert_eq!(u.total_tokens, 15);
    }

    #[test]
    fn usage_accepts_camel_case() {
        let u: Usage = serde_json::from_str(
            r#"{"inputTokens":1,"outputTokens":2,"reasoningTokens":3,"totalTokens":6}"#,
        )
        .unwrap();
        assert_eq!(u.input_tokens, 1);
        assert_eq!(u.reasoning_tokens, 3);
        assert_eq!(u.total_tokens, 6);
    }

    #[test]
    fn percent_uses_max() {
        let u = ContextUsage { used_tokens: 85, max_tokens: 100, ..Default::default() };
        assert!((u.percent() - 0.85).abs() < 1e-9);
    }

    #[test]
    fn seven_sources() {
        assert_eq!(UsageSource::ALL.len(), 7);
    }
}
