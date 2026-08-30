//! Token generation and validation for the local daemon.

use rand::RngCore;

/// Generates a fresh 32-byte hex token. Persisted to the data dir on first
/// launch so the Tauri shell and the daemon share it across restarts.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time-ish validation. Tokens are short (64 chars) and both sides
/// are local, so a simple equality check is acceptable; the comparison is
/// still written to avoid early-exit timing.
pub fn validate_token(expected: &str, provided: &str) -> bool {
    if expected.len() != provided.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.bytes().zip(provided.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_64_hex_chars() {
        let t = generate_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn validates_equality() {
        let t = generate_token();
        assert!(validate_token(&t, &t));
        assert!(!validate_token(&t, &"x".repeat(64)));
        assert!(!validate_token(&t, &t[..63]));
    }

    #[test]
    fn tokens_are_unique() {
        assert_ne!(generate_token(), generate_token());
    }
}
