//! `zhiyu-protocol` — the wire contracts shared by every layer of 知屿 Zhīyǔ.
//!
//! This crate contains only data types (no I/O, no business logic): modes,
//! messages, session cursors, checkpoints, thought levels, context usage,
//! model configuration and the JSON-RPC request/response/event envelopes.

/// Placeholder for the M1 skeleton. Replaced with the full contract types in M2.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn protocol_version_is_semver() {
        assert!(super::version().starts_with("0.1."));
    }
}
