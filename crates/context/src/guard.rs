//! Model-switch guard: refuse a switch when the used context would overflow
//! the target model's available window.

use zhiyu_protocol::ModelConfig;

/// The result of a guarded model switch.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GuardResult {
    /// Switch allowed.
    Ok { model_id: String },
    /// Used tokens exceed the target's usable window: compaction is required
    /// first.
    RequiresCompaction {
        model_id: String,
        used_tokens: u64,
        usable_window: u64,
        message: String,
    },
    /// A task is currently running; wait for it to finish.
    Busy { message: String },
}

/// Evaluates a model switch for a session.
///
/// `used_tokens` is the session's live usage; `target` is the model being
/// switched to. When `used > target.usable_context()`, the switch is blocked
/// until a compaction brings it down (per the plan: used > contextWindow -
/// maxOutputTokens → force compact first).
pub fn evaluate_switch(
    used_tokens: u64,
    target: &ModelConfig,
    task_running: bool,
) -> GuardResult {
    if task_running {
        return GuardResult::Busy {
            message: "当前有任务正在运行，请等待任务完成后切换模型".into(),
        };
    }
    let usable = target.usable_context();
    if used_tokens > usable {
        GuardResult::RequiresCompaction {
            model_id: target.id.clone(),
            used_tokens,
            usable_window: usable,
            message: format!(
                "当前用量 {} tokens 超过目标模型可用窗口 {} tokens，请先压缩上下文再切换",
                used_tokens, usable
            ),
        }
    } else {
        GuardResult::Ok { model_id: target.id.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhiyu_protocol::{ApiFormat, ReasoningConfig};

    fn model(window: u64, output: u64) -> ModelConfig {
        ModelConfig {
            id: "m".into(),
            vendor: "t".into(),
            name: "m".into(),
            base_url: "https://x".into(),
            api_format: ApiFormat::Chat,
            context_window: window,
            max_output_tokens: output,
            reasoning: ReasoningConfig::default(),
            provider_key_id: None,
        }
    }

    #[test]
    fn allows_within_window() {
        let m = model(1_000_000, 128_000);
        assert_eq!(evaluate_switch(500_000, &m, false), GuardResult::Ok { model_id: "m".into() });
    }

    #[test]
    fn blocks_when_over_usable_window() {
        let m = model(200_000, 8_000);
        // usable = 192_000
        let r = evaluate_switch(193_000, &m, false);
        match r {
            GuardResult::RequiresCompaction { used_tokens, usable_window, .. } => {
                assert_eq!(used_tokens, 193_000);
                assert_eq!(usable_window, 192_000);
            }
            _ => panic!("expected requires compaction"),
        }
    }

    #[test]
    fn blocks_when_busy() {
        let m = model(1_000_000, 128_000);
        assert!(matches!(evaluate_switch(10, &m, true), GuardResult::Busy { .. }));
    }

    #[test]
    fn boundary_exactly_at_window_is_ok() {
        let m = model(200_000, 8_000);
        assert_eq!(evaluate_switch(192_000, &m, false), GuardResult::Ok { model_id: "m".into() });
    }
}
