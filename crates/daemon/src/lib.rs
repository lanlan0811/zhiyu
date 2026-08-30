//! `zhiyu-daemon` — the local control-plane daemon.
//!
//! Serves a WebSocket + JSON-RPC endpoint on loopback with token
//! authentication and a Hello/Request/Event envelope; events carry a
//! monotonically increasing sequence number so a reconnecting client can
//! replay anything it missed.

/// Placeholder for the M1 skeleton. Replaced in M2.
pub fn default_port() -> u16 {
    17691
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_port_is_odd_and_local() {
        let p = super::default_port();
        assert!((10_000..=20_000).contains(&p));
    }
}
