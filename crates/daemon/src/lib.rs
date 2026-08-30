//! `zhiyu-daemon` — the local control-plane daemon.
//!
//! Serves a WebSocket + JSON-RPC endpoint on loopback with token
//! authentication and a Hello/Request/Event envelope; events carry a
//! monotonically increasing sequence number so a reconnecting client can
//! replay anything it missed.

pub mod auth;
pub mod event_bus;
pub mod paths;
pub mod server;

pub use event_bus::EventBus;
pub use paths::{data_dir, database_path, knowledge_dir, models_path, settings_path, state_path, token_path};
pub use server::{serve, RequestHandler};

/// The default loopback port for the daemon. Odd number to sit well above the
/// ephemeral range; overridable by the shell.
pub fn default_port() -> u16 {
    17691
}

/// The data directory under the user's home, e.g. `~/.zhiyu`.
pub fn data_dir_name() -> &'static str {
    ".zhiyu"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_port_is_odd_and_local() {
        let p = default_port();
        assert!((10_000..=20_000).contains(&p));
        assert_eq!(p % 2, 1);
    }
}
