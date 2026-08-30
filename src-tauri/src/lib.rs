//! Library entry for the Tauri shell (used by `tauri::generate_context!`).

use std::sync::Arc;

use zhiyu_daemon::handler::{AppState, CoreHandler};

/// Boots the core state and returns the request handler + event bus so the
/// shell (or tests) can drive the daemon programmatically.
pub fn boot() -> (Arc<AppState>, Arc<CoreHandler>) {
    let state = Arc::new(AppState::open(None).expect("open app state"));
    let handler = Arc::new(CoreHandler { state: state.clone() });
    (state, handler)
}
