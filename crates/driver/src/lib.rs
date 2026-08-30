//! `zhiyu-driver` — OpenAI dual-protocol driver.
//!
//! Implements direct API calls to the model endpoint over the two OpenAI
//! native protocols (`chat/completions` and `responses`), SSE streaming,
//! tool calling, usage reporting and the thought-level patch system.

/// Placeholder for the M1 skeleton. Replaced in M3.
pub fn protocol_support() -> &'static [&'static str] {
    &["chat/completions", "responses"]
}

#[cfg(test)]
mod tests {
    #[test]
    fn supports_both_openai_protocols() {
        let p = super::protocol_support();
        assert!(p.contains(&"chat/completions"));
        assert!(p.contains(&"responses"));
    }
}
