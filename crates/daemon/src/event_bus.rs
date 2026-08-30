//! The event bus: assigns monotonic sequence numbers to every event, keeps a
//! bounded ring buffer for reconnect replay, and broadcasts to subscribers.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tokio::sync::mpsc;
use zhiyu_protocol::Event;

/// How many events the daemon keeps for replay after a reconnect.
const REPLAY_BUFFER_CAP: usize = 4096;

/// Broadcasts events to all connected clients and supports replay from a
/// known sequence number.
#[derive(Debug, Default)]
pub struct EventBus {
    next_seq: AtomicU64,
    buffer: Mutex<VecDeque<(u64, Event)>>,
    subscribers: Mutex<Vec<mpsc::UnboundedSender<Event>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Emits an event, assigns its seq and returns it.
    pub fn emit(&self, mut event: Event) -> u64 {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        set_seq(&mut event, seq);
        {
            let mut buf = self.buffer.lock().unwrap();
            buf.push_back((seq, event.clone()));
            while buf.len() > REPLAY_BUFFER_CAP {
                buf.pop_front();
            }
        }
        let subs = self.subscribers.lock().unwrap().clone();
        for sub in subs {
            let _ = sub.send(event.clone());
        }
        seq
    }

    /// Latest seq assigned so far.
    pub fn last_seq(&self) -> u64 {
        self.next_seq.load(Ordering::SeqCst).saturating_sub(1)
    }

    /// Events with seq `> last_seq`, oldest first, for replay.
    pub fn replay_from(&self, last_seq: u64) -> Vec<Event> {
        self.buffer
            .lock()
            .unwrap()
            .iter()
            .filter(|(seq, _)| *seq > last_seq)
            .map(|(_, e)| e.clone())
            .collect()
    }

    /// Subscribes this connection to every future event (after replay).
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<Event> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }
}

fn set_seq(event: &mut Event, seq: u64) {
    match event {
        Event::Message { seq: s, .. }
        | Event::TextDelta { seq: s, .. }
        | Event::ReasoningDelta { seq: s, .. }
        | Event::ToolStarted { seq: s, .. }
        | Event::ToolFinished { seq: s, .. }
        | Event::UsageUpdate { seq: s, .. }
        | Event::TurnFinished { seq: s, .. }
        | Event::SessionChanged { seq: s, .. }
        | Event::Status { seq: s, .. }
        | Event::ContextCompacted { seq: s, .. } => *s = seq,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(text: &str) -> Event {
        Event::Status { seq: 0, session_id: None, text: text.to_string() }
    }

    #[tokio::test]
    async fn assigns_monotonic_seqs() {
        let bus = EventBus::new();
        let a = bus.emit(status("a"));
        let b = bus.emit(status("b"));
        assert!(a < b);
        assert_eq!(bus.last_seq(), b);
    }

    #[tokio::test]
    async fn replays_from_seq() {
        let bus = EventBus::new();
        bus.emit(status("a"));
        bus.emit(status("b"));
        let replayed = bus.replay_from(0);
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].seq(), 1);
        assert!(bus.replay_from(1).is_empty());
    }

    #[tokio::test]
    async fn broadcasts_to_subscribers() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        bus.emit(status("hello"));
        let got = rx.recv().await.unwrap();
        assert_eq!(got.seq(), 0);
    }

    #[tokio::test]
    async fn replay_and_subscribe_deliver_no_gaps() {
        let bus = EventBus::new();
        bus.emit(status("old1"));
        bus.emit(status("old2"));
        let replayed = bus.replay_from(0);
        let mut rx = bus.subscribe();
        bus.emit(status("new1"));
        let live = rx.recv().await.unwrap();
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].seq(), 1);
        assert_eq!(live.seq(), 2);
    }
}
