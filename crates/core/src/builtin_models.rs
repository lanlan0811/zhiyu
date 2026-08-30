//! Built-in model catalogue: the five preset models (DeepSeek + GLM) that
//! ship with the app. These are *defaults only* — the user can override any
//! field (persisted to `~/.zhiyu/models.json`) and add custom models.
//!
//! Specs (from the product plan):
//! - `deepseek-v4-pro` / `deepseek-v4-flash`: DeepSeek, 1M window, 348K output,
//!   six thought levels off~max with low/medium→high, xhigh→max aliases.
//! - `glm-5.2` / `glm-5.3` / `glm-5.3-flash`: GLM/智谱, 1M window, 128K output.

use zhiyu_protocol::thought::{PathValue, RequestPatch, ThoughtLevel, ThoughtLevelSpec};
use zhiyu_protocol::{ApiFormat, ModelConfig, ReasoningConfig};

/// All built-in model ids, in catalogue order.
pub const BUILTIN_MODEL_IDS: [&str; 5] = [
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "glm-5.2",
    "glm-5.3",
    "glm-5.3-flash",
];

pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
pub const GLM_BASE_URL: &str = "https://open.bigmodel.cn/api/paas/v4";

/// Returns the built-in catalogue (fresh copies — the caller may mutate them
/// as overrides without touching the constants).
pub fn builtin_models() -> Vec<ModelConfig> {
    vec![
        deepseek("deepseek-v4-pro", "DeepSeek V4 Pro"),
        deepseek("deepseek-v4-flash", "DeepSeek V4 Flash"),
        glm("glm-5.2", "GLM 5.2"),
        glm("glm-5.3", "GLM 5.3"),
        glm("glm-5.3-flash", "GLM 5.3 Flash"),
    ]
}

/// Looks up a built-in model by id.
pub fn builtin_model(id: &str) -> Option<ModelConfig> {
    builtin_models().into_iter().find(|m| m.id == id)
}

fn deepseek(id: &str, name: &str) -> ModelConfig {
    ModelConfig {
        id: id.to_string(),
        vendor: "DeepSeek".into(),
        name: name.to_string(),
        base_url: DEEPSEEK_BASE_URL.into(),
        api_format: ApiFormat::Chat,
        context_window: 1_000_000,
        max_output_tokens: 348_000,
        reasoning: deepseek_reasoning(),
        provider_key_id: Some("deepseek".into()),
    }
}

/// DeepSeek six-level reasoning: off~max, with low/medium→high and
/// xhigh→max as aliases (per the ZCode measured values in the plan).
fn deepseek_reasoning() -> ReasoningConfig {
    let effort = |lvl: ThoughtLevel| {
        let label = lvl.as_str();
        RequestPatch {
            set: vec![PathValue {
                path: vec!["reasoning_effort".into()],
                value: serde_json::json!(label),
            }],
            unset: vec![],
        }
    };
    let levels = vec![
        ThoughtLevelSpec { value: ThoughtLevel::Off, label: "关闭".into(), description: "不启用思考".into() },
        ThoughtLevelSpec { value: ThoughtLevel::Low, label: "低".into(), description: "低档思考（DeepSeek 映射到 high）".into() },
        ThoughtLevelSpec { value: ThoughtLevel::Medium, label: "中".into(), description: "中档思考（DeepSeek 映射到 high）".into() },
        ThoughtLevelSpec { value: ThoughtLevel::High, label: "高".into(), description: "高档思考".into() },
        ThoughtLevelSpec { value: ThoughtLevel::Xhigh, label: "极高".into(), description: "极高思考（DeepSeek 映射到 max）".into() },
        ThoughtLevelSpec { value: ThoughtLevel::Max, label: "最大".into(), description: "最大思考".into() },
    ];
    ReasoningConfig {
        enabled: true,
        default_level: ThoughtLevel::High,
        levels,
        provider_options_by_level: [
            (ThoughtLevel::Off, RequestPatch { set: vec![], unset: vec![vec!["reasoning_effort".into()]] }),
            (ThoughtLevel::Low, effort(ThoughtLevel::High)),
            (ThoughtLevel::Medium, effort(ThoughtLevel::High)),
            (ThoughtLevel::High, effort(ThoughtLevel::High)),
            (ThoughtLevel::Xhigh, effort(ThoughtLevel::Max)),
            (ThoughtLevel::Max, effort(ThoughtLevel::Max)),
        ]
        .into_iter()
        .map(|(l, p)| (l.as_str().to_string(), p))
        .collect(),
    }
}

fn glm(id: &str, name: &str) -> ModelConfig {
    ModelConfig {
        id: id.to_string(),
        vendor: "GLM".into(),
        name: name.to_string(),
        base_url: GLM_BASE_URL.into(),
        api_format: ApiFormat::Chat,
        context_window: 1_000_000,
        max_output_tokens: 128_000,
        reasoning: glm_reasoning(),
        provider_key_id: Some("glm".into()),
    }
}

/// GLM reasoning set: five levels (no xhigh — GLM declares off/low/medium/
/// high/max per the plan).
fn glm_reasoning() -> ReasoningConfig {
    let effort = |lvl: ThoughtLevel| RequestPatch {
        set: vec![PathValue {
            path: vec!["reasoning_effort".into()],
            value: serde_json::json!(lvl.as_str()),
        }],
        unset: vec![],
    };
    let levels = vec![
        ThoughtLevelSpec { value: ThoughtLevel::Off, label: "关闭".into(), description: "不启用思考".into() },
        ThoughtLevelSpec { value: ThoughtLevel::Low, label: "低".into(), description: "低档思考".into() },
        ThoughtLevelSpec { value: ThoughtLevel::Medium, label: "中".into(), description: "中档思考".into() },
        ThoughtLevelSpec { value: ThoughtLevel::High, label: "高".into(), description: "高档思考".into() },
        ThoughtLevelSpec { value: ThoughtLevel::Max, label: "最大".into(), description: "最大思考".into() },
    ];
    ReasoningConfig {
        enabled: true,
        default_level: ThoughtLevel::High,
        levels,
        provider_options_by_level: [
            (ThoughtLevel::Off, RequestPatch { set: vec![], unset: vec![vec!["reasoning_effort".into()]] }),
            (ThoughtLevel::Low, effort(ThoughtLevel::Low)),
            (ThoughtLevel::Medium, effort(ThoughtLevel::Medium)),
            (ThoughtLevel::High, effort(ThoughtLevel::High)),
            (ThoughtLevel::Max, effort(ThoughtLevel::Max)),
        ]
        .into_iter()
        .map(|(l, p)| (l.as_str().to_string(), p))
        .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_has_five_models() {
        let models = builtin_models();
        assert_eq!(models.len(), 5);
        for id in BUILTIN_MODEL_IDS {
            assert!(models.iter().any(|m| m.id == id), "missing {id}");
        }
    }

    #[test]
    fn deepseek_specs() {
        let m = builtin_model("deepseek-v4-pro").unwrap();
        assert_eq!(m.base_url, DEEPSEEK_BASE_URL);
        assert_eq!(m.api_format, ApiFormat::Chat);
        assert_eq!(m.context_window, 1_000_000);
        assert_eq!(m.max_output_tokens, 348_000);
        assert_eq!(m.provider_key_id.as_deref(), Some("deepseek"));
    }

    #[test]
    fn glm_specs() {
        for id in ["glm-5.2", "glm-5.3", "glm-5.3-flash"] {
            let m = builtin_model(id).unwrap();
            assert_eq!(m.base_url, GLM_BASE_URL);
            assert_eq!(m.context_window, 1_000_000);
            assert_eq!(m.max_output_tokens, 128_000);
            assert_eq!(m.provider_key_id.as_deref(), Some("glm"));
        }
    }

    #[test]
    fn deepseek_aliases_low_medium_to_high() {
        let m = builtin_model("deepseek-v4-pro").unwrap();
        let patch = &m.reasoning.provider_options_by_level["low"];
        assert_eq!(patch.set[0].value, serde_json::json!("high"));
        let patch = &m.reasoning.provider_options_by_level["xhigh"];
        assert_eq!(patch.set[0].value, serde_json::json!("max"));
        // off unsets the effort
        assert!(m.reasoning.provider_options_by_level["off"].unset.iter().any(|p| p == &vec!["reasoning_effort".to_string()]));
    }

    #[test]
    fn glm_has_no_xhigh_and_unsets_on_off() {
        let m = builtin_model("glm-5.3").unwrap();
        assert!(!m.reasoning.provider_options_by_level.contains_key("xhigh"));
        assert!(m.reasoning.provider_options_by_level.contains_key("max"));
    }

    #[test]
    fn usable_context_reserves_output() {
        let pro = builtin_model("deepseek-v4-pro").unwrap();
        let flash = builtin_model("glm-5.3-flash").unwrap();
        assert_eq!(pro.usable_context(), 1_000_000 - 348_000);
        assert_eq!(flash.usable_context(), 1_000_000 - 128_000);
    }
}
