//! `zhiyu-browser` — embedded browser control.
//!
//! Manages WebView2 tab lifecycles and exposes a Playwright-style control
//! engine (locator resolution, DOM snapshots, clicks, fills, screenshots,
//! evaluation) through the `browser_execute` tool, with user/agent tab
//! isolation and knowledge-base handoff in daily mode.

pub mod engine;
pub mod service;
pub mod tabs;

pub use engine::{BrowserCommand, OpResult, SnapshotNode};
pub use service::{BrowserLogEntry, BrowserService};
pub use tabs::{Tab, TabManager, TabOrigin};

/// Whether the browser supports agent control (always true; the WebView2
/// bridge is wired on the Tauri side in M7).
pub fn supports_agent_control() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_control_supported() {
        assert!(supports_agent_control());
    }
}
