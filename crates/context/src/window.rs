//! Context-window resolution: `[1m]` suffix → 1M, invalid/missing → 200K.

/// Parses a context-window value from a model config field.
///
/// Accepts a plain number of tokens, or a string with a size suffix
/// (`[1m]` → 1,000,000, `200k` → 200,000). Anything unparseable falls back
/// to the default 200K.
pub fn resolve_context_window(raw: &str) -> u64 {
    let raw = raw.trim();
    if raw.is_empty() {
        return DEFAULT_WINDOW;
    }
    // "[1m]" style suffix
    if let Some(inner) = raw
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .or_else(|| raw.strip_prefix('(').and_then(|s| s.strip_suffix(')')))
    {
        return parse_size(inner).unwrap_or(DEFAULT_WINDOW);
    }
    // bare number or "200k"/"1m" suffix
    if let Ok(n) = raw.parse::<u64>() {
        return n;
    }
    parse_size(raw).unwrap_or(DEFAULT_WINDOW)
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_ascii_lowercase();
    let (num, mult) = if let Some(rest) = s.strip_suffix('m') {
        (rest, 1_000_000u64)
    } else if let Some(rest) = s.strip_suffix('k') {
        (rest, 1_000u64)
    } else {
        (s.as_str(), 1u64)
    };
    num.trim().parse::<u64>().ok().map(|n| n.saturating_mul(mult))
}

/// Default window when nothing is configured.
pub const DEFAULT_WINDOW: u64 = 200_000;
/// 1M tokens.
pub const ONE_MILLION: u64 = 1_000_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracket_suffix_is_one_million() {
        assert_eq!(resolve_context_window("[1m]"), ONE_MILLION);
        assert_eq!(resolve_context_window("[1M]"), ONE_MILLION);
        assert_eq!(resolve_context_window("(1m)"), ONE_MILLION);
    }

    #[test]
    fn bare_and_k_suffix() {
        assert_eq!(resolve_context_window("200000"), 200_000);
        assert_eq!(resolve_context_window("200k"), 200_000);
        assert_eq!(resolve_context_window("1m"), ONE_MILLION);
    }

    #[test]
    fn invalid_falls_back_to_default() {
        assert_eq!(resolve_context_window(""), DEFAULT_WINDOW);
        assert_eq!(resolve_context_window("garbage"), DEFAULT_WINDOW);
        assert_eq!(resolve_context_window("!!!"), DEFAULT_WINDOW);
    }

    #[test]
    fn from_u64_strings() {
        assert_eq!(resolve_context_window("999999"), 999_999);
    }
}
