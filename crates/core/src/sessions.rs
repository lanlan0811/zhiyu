//! Session management: dual-mode, multi-session, with cursor-based resume.

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::mpsc;
use uuid::Uuid;
use zhiyu_protocol::{Message, Mode, SessionCursor};

use crate::store::{SessionRow, Store};

/// A live (in-memory) session handle.
#[derive(Debug)]
pub struct LiveSession {
    pub row: SessionRow,
    /// Queue of pending user turns (per the harness's message queuing).
    pub queue: Vec<String>,
    /// Whether a turn is currently streaming.
    pub streaming: bool,
    /// The ephemeral thought-level override for the next turn.
    pub thought_level: Option<zhiyu_protocol::ThoughtLevel>,
    /// The model id bound to this session (defaults to the mode default).
    pub model_id: Option<String>,
}

/// Manages sessions of both modes, backed by the SQLite store.
pub struct SessionManager {
    store: Mutex<Store>,
    live: Mutex<HashMap<Uuid, LiveSession>>,
}

impl SessionManager {
    pub fn new(store: Store) -> Self {
        SessionManager { store: Mutex::new(store), live: Mutex::new(HashMap::new()) }
    }

    pub fn create(&self, mode: Mode, title: Option<&str>, workspace_dir: Option<&str>) -> anyhow::Result<SessionRow> {
        let row = self
            .store
            .lock()
            .unwrap()
            .create_session(mode, title.unwrap_or("新会话"), workspace_dir)?;
        self.live.lock().unwrap().insert(
            row.id,
            LiveSession { row: row.clone(), queue: vec![], streaming: false, thought_level: None, model_id: None },
        );
        Ok(row)
    }

    pub fn open(&self, mode: Mode, id: Uuid) -> anyhow::Result<Option<SessionRow>> {
        let mut store = self.store.lock().unwrap();
        let Some(row) = store.get_session(mode, id)? else { return Ok(None) };
        self.live.lock().unwrap().entry(id).or_insert(LiveSession {
            row: row.clone(),
            queue: vec![],
            streaming: false,
            thought_level: None,
            model_id: None,
        });
        Ok(Some(row))
    }

    /// Binds a model id to the session (persisted in memory; a follow-up
    /// stores it in the session row).
    pub fn set_model(&self, session_id: Uuid, model_id: &str) -> anyhow::Result<()> {
        let mut live = self.live.lock().unwrap();
        let s = live.get_mut(&session_id).ok_or_else(|| anyhow::anyhow!("session not open"))?;
        s.model_id = Some(model_id.to_string());
        Ok(())
    }

    /// The model id bound to the session, else the given default.
    pub fn model_id(&self, session_id: Uuid, default: &str) -> String {
        self.live
            .lock()
            .unwrap()
            .get(&session_id)
            .and_then(|s| s.model_id.clone())
            .unwrap_or_else(|| default.to_string())
    }

    pub fn delete(&self, mode: Mode, id: Uuid) -> anyhow::Result<()> {
        self.store.lock().unwrap().delete_session(mode, id)?;
        self.live.lock().unwrap().remove(&id);
        Ok(())
    }

    pub fn list(&self, mode: Mode) -> anyhow::Result<Vec<SessionRow>> {
        self.store.lock().unwrap().list_sessions(mode)
    }

    /// Appends a message to the transcript (persisted).
    pub fn append_message(
        &self,
        mode: Mode,
        session_id: Uuid,
        role: zhiyu_protocol::Role,
        content: &str,
        reasoning: Option<&str>,
        tool_name: Option<&str>,
    ) -> anyhow::Result<Message> {
        self.store.lock().unwrap().append_message(mode, session_id, role, content, reasoning, tool_name)
    }

    pub fn messages(&self, session_id: Uuid) -> anyhow::Result<Vec<Message>> {
        self.store.lock().unwrap().messages(session_id)
    }

    /// Resumes from a cursor: returns messages from the cursor onward. This is
    /// the replay contract for reconnect / fork.
    pub fn resume(&self, cursor: SessionCursor) -> anyhow::Result<Vec<Message>> {
        self.store.lock().unwrap().messages_from(cursor.session_id, cursor.next_cursor)
    }

    pub fn next_cursor(&self, session_id: Uuid) -> anyhow::Result<u64> {
        self.store.lock().unwrap().next_cursor(session_id)
    }

    pub fn truncate(&self, session_id: Uuid, cursor: u64) -> anyhow::Result<()> {
        self.store.lock().unwrap().truncate_messages(session_id, cursor)
    }

    /// Queues a user turn on the session. Returns the position in the queue
    /// (0 = running immediately).
    pub fn enqueue_turn(&self, session_id: Uuid, text: String) -> anyhow::Result<usize> {
        let mut live = self.live.lock().unwrap();
        let s = live.get_mut(&session_id).ok_or_else(|| anyhow::anyhow!("session not open"))?;
        s.queue.push(text);
        Ok(s.queue.len() - 1)
    }

    /// Pops the next queued turn (called by the turn runner).
    pub fn pop_turn(&self, session_id: Uuid) -> anyhow::Result<Option<String>> {
        let mut live = self.live.lock().unwrap();
        let s = live.get_mut(&session_id).ok_or_else(|| anyhow::anyhow!("session not open"))?;
        Ok(s.queue.drain(..1).next())
    }

    /// Transient thought-level override for the next turn.
    pub fn set_thought_level(&self, session_id: Uuid, level: zhiyu_protocol::ThoughtLevel) -> anyhow::Result<()> {
        let mut live = self.live.lock().unwrap();
        let s = live.get_mut(&session_id).ok_or_else(|| anyhow::anyhow!("session not open"))?;
        s.thought_level = Some(level);
        Ok(())
    }

    /// The session's current thought level: session override, else the mode
    /// default from settings (resolved by the caller).
    pub fn take_thought_level(&self, session_id: Uuid, mode_default: zhiyu_protocol::ThoughtLevel) -> zhiyu_protocol::ThoughtLevel {
        let mut live = self.live.lock().unwrap();
        let s = match live.get_mut(&session_id) {
            Some(s) => s,
            None => return mode_default,
        };
        s.thought_level.take().unwrap_or(mode_default)
    }

    pub fn is_streaming(&self, session_id: Uuid) -> bool {
        self.live.lock().unwrap().get(&session_id).map(|s| s.streaming).unwrap_or(false)
    }

    pub fn set_streaming(&self, session_id: Uuid, streaming: bool) {
        if let Some(s) = self.live.lock().unwrap().get_mut(&session_id) {
            s.streaming = streaming;
        }
    }

    /// The workspace dir bound to a coding session.
    pub fn workspace_dir(&self, mode: Mode, session_id: Uuid) -> anyhow::Result<Option<String>> {
        Ok(self.store.lock().unwrap().get_session(mode, session_id)?.and_then(|s| s.workspace_dir))
    }
}

/// Broadcast channel used by the session manager to fan out transcript
/// updates to the daemon's event bus (wired in M7).
#[allow(dead_code)]
pub type UpdateSender = mpsc::UnboundedSender<SessionEvent>;

#[derive(Debug, Clone)]
pub enum SessionEvent {
    Message(Message),
    TurnFinished { session_id: Uuid, cursor: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhiyu_protocol::{Role, ThoughtLevel};

    fn manager() -> (tempfile::TempDir, SessionManager) {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::store::Store::open(&dir.path().join("db.sqlite")).unwrap();
        (dir, SessionManager::new(store))
    }

    #[test]
    fn create_open_append_resume() {
        let (_dir, mgr) = manager();
        let s = mgr.create(Mode::Daily, Some("写作"), None).unwrap();
        mgr.append_message(Mode::Daily, s.id, Role::User, "帮我写", None, None).unwrap();
        mgr.append_message(Mode::Daily, s.id, Role::Assistant, "好的", None, None).unwrap();

        // re-open from storage and resume from cursor 1
        let opened = mgr.open(Mode::Daily, s.id).unwrap().unwrap();
        assert_eq!(opened.id, s.id);
        let resumed = mgr.resume(SessionCursor { session_id: s.id, next_cursor: 1 }).unwrap();
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].content, "好的");
    }

    #[test]
    fn queue_orders_turns() {
        let (_dir, mgr) = manager();
        let s = mgr.create(Mode::Daily, None, None).unwrap();
        assert_eq!(mgr.enqueue_turn(s.id, "一".into()).unwrap(), 0);
        assert_eq!(mgr.enqueue_turn(s.id, "二".into()).unwrap(), 1);
        assert_eq!(mgr.pop_turn(s.id).unwrap().as_deref(), Some("一"));
        assert_eq!(mgr.pop_turn(s.id).unwrap().as_deref(), Some("二"));
        assert_eq!(mgr.pop_turn(s.id).unwrap(), None);
    }

    #[test]
    fn thought_level_override_round_trip() {
        let (_dir, mgr) = manager();
        let s = mgr.create(Mode::Daily, None, None).unwrap();
        mgr.set_thought_level(s.id, ThoughtLevel::Max).unwrap();
        assert_eq!(mgr.take_thought_level(s.id, ThoughtLevel::Medium), ThoughtLevel::Max);
        // consumed — falls back to the mode default
        assert_eq!(mgr.take_thought_level(s.id, ThoughtLevel::Medium), ThoughtLevel::Medium);
    }
}
