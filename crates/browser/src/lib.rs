//! `zhiyu-browser` — embedded browser control.
//!
//! Manages WebView2 tab lifecycles and exposes a Playwright-style control
//! engine (locator resolution, DOM snapshots, clicks, fills, screenshots,
//! evaluation) through the `browser_execute` tool, with user/agent tab
//! isolation and knowledge-base handoff in daily mode.

/// Placeholder for the M1 skeleton. Replaced in M6.
pub fn supports_agent_control() -> bool {
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn agent_control_supported() {
        assert!(super::supports_agent_control());
    }
}
