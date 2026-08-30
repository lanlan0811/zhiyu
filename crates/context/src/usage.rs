//! Usage tracking: accumulates usage from every API response and keeps a
//! bounded ring buffer of per-response snapshots plus the 7-source breakdown.

use std::collections::BTreeMap;

use zhiyu_protocol::{ContextUsage, Usage, UsageSource};

/// How many per-response snapshots the ring buffer keeps.
pub const RING_CAP: usize = 256;

/// Accumulates usage over a session.
#[derive(Debug, Clone, Default)]
pub struct UsageTracker {
    /// Ring buffer of recent per-response usage snapshots.
    ring: VecDeque<Usage>,
    /// Aggregated totals.
    total: Usage,
    /// Per-source breakdown (token count by source name).
    breakdown: BTreeMap<String, u64>,
}

use std::collections::VecDeque;

impl UsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one API response's usage.
    pub fn record(&mut self, usage: Usage) {
        self.ring.push_back(usage);
        while self.ring.len() > RING_CAP {
            self.ring.pop_front();
        }
        self.total.input_tokens += usage.input_tokens;
        self.total.output_tokens += usage.output_tokens;
        self.total.reasoning_tokens += usage.reasoning_tokens;
        self.total.cached_read_tokens += usage.cached_read_tokens;
        self.total.cached_write_tokens += usage.cached_write_tokens;
        self.total.total_tokens += usage.total_tokens;
    }

    /// Assigns a token count to one of the 7 sources (system prompt, skills,
    /// tool schemas, messages, …).
    pub fn set_source(&mut self, source: UsageSource, tokens: u64) {
        self.breakdown.insert(source.as_str().to_string(), tokens);
    }

    pub fn source(&self, source: UsageSource) -> u64 {
        self.breakdown.get(source.as_str()).map(|v| *v).unwrap_or(0)
    }

    /// The aggregate used tokens: the sum of the 7 sources, falling back to
    /// the recorded totals when sources are not populated.
    pub fn used_tokens(&self) -> u64 {
        let sum: u64 = self.breakdown.values().sum();
        if sum > 0 {
            sum
        } else {
            self.total.input_tokens + self.total.output_tokens
        }
    }

    /// The live context usage snapshot for the UI ring.
    pub fn snapshot(&self, max_tokens: u64) -> ContextUsage {
        let used = self.used_tokens();
        ContextUsage {
            used_tokens: used,
            size_tokens: self.total.total_tokens,
            max_tokens,
            breakdown: self.breakdown.clone(),
        }
    }

    pub fn total(&self) -> &Usage {
        &self.total
    }

    /// Recent per-response usages (ring).
    pub fn recent(&self) -> &VecDeque<Usage> {
        &self.ring
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_totals() {
        let mut t = UsageTracker::new();
        t.record(Usage { input_tokens: 100, output_tokens: 50, reasoning_tokens: 10, total_tokens: 150, ..Default::default() });
        t.record(Usage { input_tokens: 200, output_tokens: 100, reasoning_tokens: 20, total_tokens: 300, ..Default::default() });
        assert_eq!(t.total().input_tokens, 300);
        assert_eq!(t.total().output_tokens, 150);
        assert_eq!(t.used_tokens(), 450);
    }

    #[test]
    fn breakdown_sources() {
        let mut t = UsageTracker::new();
        t.set_source(UsageSource::SystemPrompt, 500);
        t.set_source(UsageSource::Messages, 3000);
        assert_eq!(t.source(UsageSource::SystemPrompt), 500);
        assert_eq!(t.used_tokens(), 3500);
        // all seven source names exist
        assert_eq!(UsageSource::ALL.len(), 7);
        for s in UsageSource::ALL {
            assert!(!s.as_str().is_empty());
        }
    }

    #[test]
    fn snapshot_for_ring() {
        let mut t = UsageTracker::new();
        t.record(Usage { input_tokens: 10, output_tokens: 5, total_tokens: 15, ..Default::default() });
        let snap = t.snapshot(200_000);
        assert_eq!(snap.used_tokens, 15);
        assert_eq!(snap.max_tokens, 200_000);
        assert!((snap.percent() - 0.000075).abs() < 1e-9);
    }

    #[test]
    fn ring_is_bounded() {
        let mut t = UsageTracker::new();
        for i in 0..(RING_CAP + 50) {
            t.record(Usage { input_tokens: i as u64, output_tokens: 1, total_tokens: i as u64 + 1, ..Default::default() });
        }
        assert_eq!(t.recent().len(), RING_CAP);
    }
}
