//! SQLite (WAL) persistence: sessions, messages, checkpoints.

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;
use zhiyu_protocol::{Message, Mode, Role, SessionCursor};

/// Opens (or creates) the database at `path` with WAL enabled and the schema
/// migrated.
pub fn open_db(path: &Path) -> anyhow::Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Applies the schema. Sessions are stored per mode
/// (`daily_sessions` / `coding_sessions`), keeping the two modes fully
/// independent.
fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS daily_sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            workspace_dir TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS coding_sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            workspace_dir TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            mode TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            reasoning TEXT,
            tool_name TEXT,
            cursor INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, cursor);
        CREATE TABLE IF NOT EXISTS checkpoints (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            ref_name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_checkpoints_session ON checkpoints(session_id);
        "#,
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: Uuid,
    pub title: String,
    pub workspace_dir: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// CRUD for sessions and messages.
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        Ok(Store { conn: open_db(path)? })
    }

    pub fn create_session(
        &mut self,
        mode: Mode,
        title: &str,
        workspace_dir: Option<&str>,
    ) -> anyhow::Result<SessionRow> {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            &format!(
                "INSERT INTO {} (id, title, workspace_dir, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                mode.session_table()
            ),
            rusqlite::params![id.to_string(), title, workspace_dir, now],
        )?;
        Ok(SessionRow { id, title: title.to_string(), workspace_dir: workspace_dir.map(String::from), created_at: Utc::now(), updated_at: Utc::now() })
    }

    pub fn list_sessions(&mut self, mode: Mode) -> anyhow::Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, title, workspace_dir, created_at, updated_at FROM {} ORDER BY updated_at DESC",
            mode.session_table()
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok(SessionRow {
                id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                title: r.get(1)?,
                workspace_dir: r.get(2)?,
                created_at: r.get::<_, String>(3)?.parse().unwrap_or_else(|_| Utc::now()),
                updated_at: r.get::<_, String>(4)?.parse().unwrap_or_else(|_| Utc::now()),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_session(&mut self, mode: Mode, id: Uuid) -> anyhow::Result<Option<SessionRow>> {
        let row = self
            .conn
            .query_row(
                &format!(
                    "SELECT id, title, workspace_dir, created_at, updated_at FROM {} WHERE id = ?1",
                    mode.session_table()
                ),
                [id.to_string()],
                |r| {
                    Ok(SessionRow {
                        id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                        title: r.get(1)?,
                        workspace_dir: r.get(2)?,
                        created_at: r.get::<_, String>(3)?.parse().unwrap_or_else(|_| Utc::now()),
                        updated_at: r.get::<_, String>(4)?.parse().unwrap_or_else(|_| Utc::now()),
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn delete_session(&mut self, mode: Mode, id: Uuid) -> anyhow::Result<()> {
        self.conn.execute(
            &format!("DELETE FROM {} WHERE id = ?1", mode.session_table()),
            [id.to_string()],
        )?;
        self.conn.execute("DELETE FROM messages WHERE session_id = ?1", [id.to_string()])?;
        self.conn.execute("DELETE FROM checkpoints WHERE session_id = ?1", [id.to_string()])?;
        Ok(())
    }

    pub fn touch_session(&mut self, mode: Mode, id: Uuid) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            &format!("UPDATE {} SET updated_at = ?2 WHERE id = ?1", mode.session_table()),
            rusqlite::params![id.to_string(), now],
        )?;
        Ok(())
    }

    /// Appends a message, assigning the next cursor within the session.
    pub fn append_message(&mut self, mode: Mode, session_id: Uuid, role: Role, content: &str, reasoning: Option<&str>, tool_name: Option<&str>) -> anyhow::Result<Message> {
        let next_cursor = self.next_cursor(session_id)?;
        let msg = Message {
            id: Uuid::new_v4(),
            session_id,
            role,
            content: content.to_string(),
            reasoning: reasoning.map(String::from),
            tool_name: tool_name.map(String::from),
            cursor: next_cursor,
            created_at: Utc::now(),
        };
        self.conn.execute(
            "INSERT INTO messages (id, session_id, mode, role, content, reasoning, tool_name, cursor, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                msg.id.to_string(),
                session_id.to_string(),
                mode.as_str(),
                role_str(role),
                msg.content,
                msg.reasoning,
                msg.tool_name,
                msg.cursor,
                msg.created_at.to_rfc3339(),
            ],
        )?;
        self.touch_session(mode, session_id)?;
        Ok(msg)
    }

    /// The next cursor to assign — max(cursor)+1, 0 when empty.
    pub fn next_cursor(&mut self, session_id: Uuid) -> anyhow::Result<u64> {
        let max: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(cursor) FROM messages WHERE session_id = ?1",
                [session_id.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(max.map_or(0, |m| m as u64 + 1))
    }

    /// Messages from a cursor onward (resume/replay).
    pub fn messages_from(&mut self, session_id: Uuid, cursor: u64) -> anyhow::Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, reasoning, tool_name, cursor, created_at FROM messages WHERE session_id = ?1 AND cursor >= ?2 ORDER BY cursor",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id.to_string(), cursor as i64], row_to_message)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// All messages of a session.
    pub fn messages(&mut self, session_id: Uuid) -> anyhow::Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, reasoning, tool_name, cursor, created_at FROM messages WHERE session_id = ?1 ORDER BY cursor",
        )?;
        let rows = stmt.query_map([session_id.to_string()], row_to_message)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Truncates messages at a cursor (rollback / fork point).
    pub fn truncate_messages(&mut self, session_id: Uuid, cursor: u64) -> anyhow::Result<()> {
        self.conn.execute(
            "DELETE FROM messages WHERE session_id = ?1 AND cursor >= ?2",
            rusqlite::params![session_id.to_string(), cursor as i64],
        )?;
        Ok(())
    }

    pub fn save_checkpoint(&mut self, session_id: Uuid, ref_name: &str, description: &str) -> anyhow::Result<zhiyu_protocol::Checkpoint> {
        let cp = zhiyu_protocol::Checkpoint {
            id: Uuid::new_v4(),
            session_id,
            ref_name: ref_name.to_string(),
            description: description.to_string(),
            created_at: Utc::now(),
        };
        self.conn.execute(
            "INSERT INTO checkpoints (id, session_id, ref_name, description, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![cp.id.to_string(), session_id.to_string(), cp.ref_name, cp.description, cp.created_at.to_rfc3339()],
        )?;
        Ok(cp)
    }

    pub fn list_checkpoints(&mut self, session_id: Uuid) -> anyhow::Result<Vec<zhiyu_protocol::Checkpoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, ref_name, description, created_at FROM checkpoints WHERE session_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([session_id.to_string()], |r| {
            Ok(zhiyu_protocol::Checkpoint {
                id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                session_id: Uuid::parse_str(&r.get::<_, String>(1)?).unwrap_or_default(),
                ref_name: r.get(2)?,
                description: r.get(3)?,
                created_at: r.get::<_, String>(4)?.parse().unwrap_or_else(|_| Utc::now()),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_checkpoint(&mut self, id: Uuid) -> anyhow::Result<Option<zhiyu_protocol::Checkpoint>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, session_id, ref_name, description, created_at FROM checkpoints WHERE id = ?1",
                [id.to_string()],
                |r| {
                    Ok(zhiyu_protocol::Checkpoint {
                        id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                        session_id: Uuid::parse_str(&r.get::<_, String>(1)?).unwrap_or_default(),
                        ref_name: r.get(2)?,
                        description: r.get(3)?,
                        created_at: r.get::<_, String>(4)?.parse().unwrap_or_else(|_| Utc::now()),
                    })
                },
            )
            .optional()?;
        Ok(row)
    }
}

fn row_to_message(r: &rusqlite::Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
        session_id: Uuid::parse_str(&r.get::<_, String>(1)?).unwrap_or_default(),
        role: parse_role(&r.get::<_, String>(2)?).unwrap_or(Role::User),
        content: r.get(3)?,
        reasoning: r.get(4)?,
        tool_name: r.get(5)?,
        cursor: r.get::<_, i64>(6)? as u64,
        created_at: r.get::<_, String>(7)?.parse().unwrap_or_else(|_| Utc::now()),
    })
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

fn parse_role(s: &str) -> Option<Role> {
    match s {
        "user" => Some(Role::User),
        "assistant" => Some(Role::Assistant),
        "system" => Some(Role::System),
        "tool" => Some(Role::Tool),
        _ => None,
    }
}

impl SessionRow {
    pub fn cursor(&self) -> SessionCursor {
        // cursor is tracked per message; the row-level cursor is computed by
        // the store on demand.
        SessionCursor { session_id: self.id, next_cursor: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("test.db")).unwrap();
        (dir, store)
    }

    #[test]
    fn create_list_get_delete_session() {
        let (_dir, mut store) = store();
        let s = store.create_session(Mode::Daily, "写作", None).unwrap();
        assert!(!s.title.is_empty());
        let sessions = store.list_sessions(Mode::Daily).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, s.id);

        let got = store.get_session(Mode::Daily, s.id).unwrap().unwrap();
        assert_eq!(got.title, "写作");

        store.delete_session(Mode::Daily, s.id).unwrap();
        assert!(store.get_session(Mode::Daily, s.id).unwrap().is_none());
        assert!(store.list_sessions(Mode::Daily).unwrap().is_empty());
    }

    #[test]
    fn modes_are_independent() {
        let (_dir, mut store) = store();
        store.create_session(Mode::Daily, "d1", None).unwrap();
        store.create_session(Mode::Coding, "c1", Some(r"D:\proj")).unwrap();
        assert_eq!(store.list_sessions(Mode::Daily).unwrap().len(), 1);
        assert_eq!(store.list_sessions(Mode::Coding).unwrap().len(), 1);
        let coding = store.list_sessions(Mode::Coding).unwrap();
        assert_eq!(coding[0].workspace_dir.as_deref(), Some(r"D:\proj"));
    }

    #[test]
    fn messages_cursor_and_resume() {
        let (_dir, mut store) = store();
        let s = store.create_session(Mode::Daily, "t", None).unwrap();
        let m1 = store.append_message(Mode::Daily, s.id, Role::User, "你好", None, None).unwrap();
        let m2 = store.append_message(Mode::Daily, s.id, Role::Assistant, "你好！", Some("思考".into()), None).unwrap();
        assert_eq!(m1.cursor, 0);
        assert_eq!(m2.cursor, 1);
        assert_eq!(store.next_cursor(s.id).unwrap(), 2);

        // resume from cursor 1 → only the assistant message
        let resumed = store.messages_from(s.id, 1).unwrap();
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].cursor, 1);
        assert_eq!(resumed[0].reasoning.as_deref(), Some("思考"));
    }

    #[test]
    fn truncate_rolls_back() {
        let (_dir, mut store) = store();
        let s = store.create_session(Mode::Daily, "t", None).unwrap();
        store.append_message(Mode::Daily, s.id, Role::User, "a", None, None).unwrap();
        store.append_message(Mode::Daily, s.id, Role::Assistant, "b", None, None).unwrap();
        store.truncate_messages(s.id, 1).unwrap();
        let msgs = store.messages(s.id).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "a");
    }

    #[test]
    fn checkpoints_round_trip() {
        let (_dir, mut store) = store();
        let s = store.create_session(Mode::Coding, "t", None).unwrap();
        let cp = store.save_checkpoint(s.id, "refs/checkpoint/abc", "turn 1").unwrap();
        let list = store.list_checkpoints(s.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, cp.id);
        assert_eq!(list[0].ref_name, "refs/checkpoint/abc");
        let got = store.get_checkpoint(cp.id).unwrap().unwrap();
        assert_eq!(got.description, "turn 1");
    }

    #[test]
    fn wal_is_enabled() {
        let (_dir, store) = store();
        let mode: String = store.conn.pragma_query_value(None, "journal_mode", |r| r.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }
}
