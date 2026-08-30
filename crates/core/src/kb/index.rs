//! Knowledge-base index: documents + chunks + FTS5 full-text index +
//! vector table, and the RRF hybrid search.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use super::chunking::chunk_document;
use super::embedding::{Embedder, cosine_similarity};
use super::ingest::extract_text;

/// A document in the knowledge base.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KbDocument {
    pub id: Uuid,
    /// Absolute or relative path the document was imported from.
    pub path: String,
    pub title: String,
    pub chunk_count: usize,
    pub created_at: String,
}

/// A search hit: chunk + source document.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub doc_id: Uuid,
    pub path: String,
    pub title: String,
    pub chunk_index: usize,
    pub heading: Option<String>,
    pub content: String,
    pub score: f32,
}

/// Opens the knowledge base at `dir` (creates `dir/zhiyu_kb.sqlite`).
pub fn open_kb(dir: &Path) -> anyhow::Result<Connection> {
    std::fs::create_dir_all(dir)?;
    let conn = Connection::open(dir.join("zhiyu_kb.sqlite"))?;
    set_pragma(&conn, "journal_mode", "WAL")?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS kb_docs (
            id TEXT PRIMARY KEY,
            path TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            chunk_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS kb_chunks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            doc_id TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            heading TEXT,
            content TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_kb_chunks_doc ON kb_chunks(doc_id);
        CREATE TABLE IF NOT EXISTS kb_vectors (
            chunk_id INTEGER PRIMARY KEY,
            dim INTEGER NOT NULL,
            vec_data BLOB NOT NULL
        );
        "#,
    )?;
    // FTS5 virtual table: CREATE VIRTUAL TABLE returns a result row, which
    // rusqlite's execute/execute_batch reject when extra_check is on
    // (bundled-full), so run it through query() and drain the rows.
    if fts5_available(&conn) {
        let mut stmt = conn.prepare(
            "CREATE VIRTUAL TABLE IF NOT EXISTS kb_fts USING fts5(content, tokenize='unicode61')",
        )?;
        let mut rows = stmt.query([])?;
        while rows.next()?.is_some() {}
    }
    ensure_fts_triggers(&conn)?;
    Ok(conn)
}

/// Whether the bundled SQLite knows the FTS5 module.
fn fts5_available(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT 1 FROM pragma_module_list() WHERE name = 'fts5'",
        [],
        |_| Ok(true),
    )
    .unwrap_or(false)
}

/// Runs a pragma that returns a row (e.g. `journal_mode=WAL`) without
/// tripping rusqlite's extra_check "Execute returned results" guard.
fn set_pragma(conn: &Connection, name: &str, value: &str) -> anyhow::Result<()> {
    let sql = format!("PRAGMA {name}={value}");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while rows.next()?.is_some() {}
    Ok(())
}

/// The FTS5 triggers keep the virtual table in sync with `kb_chunks`.
pub fn ensure_fts_triggers(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TRIGGER IF NOT EXISTS kb_chunks_ai AFTER INSERT ON kb_chunks BEGIN
            INSERT INTO kb_fts(rowid, content) VALUES (new.id, new.content);
        END;
        CREATE TRIGGER IF NOT EXISTS kb_chunks_ad AFTER DELETE ON kb_chunks BEGIN
            INSERT INTO kb_fts(kb_fts, rowid, content) VALUES('delete', old.id, old.content);
        END;
        "#,
    )?;
    Ok(())
}

/// Ingests a document file: extracts text, chunks, indexes FTS + vectors.
/// FTS rows are maintained by the `kb_chunks_ai` trigger.
pub fn ingest_document(
    conn: &mut Connection,
    embedder: &dyn Embedder,
    path: &Path,
    title: &str,
) -> anyhow::Result<KbDocument> {
    let text = extract_text(path)?;
    let doc_id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let chunks = chunk_document(&text);

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO kb_docs (id, path, title, chunk_count, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![doc_id.to_string(), path.display().to_string(), title, chunks.len(), now],
    )?;
    let mut vectors: Vec<(u64, Vec<f32>)> = Vec::new();
    for chunk in &chunks {
        tx.execute(
            "INSERT INTO kb_chunks (doc_id, chunk_index, heading, content) VALUES (?1, ?2, ?3, ?4)",
            params![doc_id.to_string(), chunk.index as i64, chunk.heading, chunk.content],
        )?;
        let row_id = tx.last_insert_rowid();
        let emb = embedder.embed(&chunk.content);
        vectors.push((row_id as u64, emb.values));
    }
    tx.commit()?;

    // vectors stored in a side table after commit (kept simple & serializable)
    store_vectors(conn, &vectors)?;

    Ok(KbDocument {
        id: doc_id,
        path: path.display().to_string(),
        title: title.to_string(),
        chunk_count: chunks.len(),
        created_at: now,
    })
}

/// Vector table lives in a separate table (dim + serialized floats).
fn store_vectors(conn: &mut Connection, vectors: &[(u64, Vec<f32>)]) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kb_vectors (chunk_id INTEGER PRIMARY KEY, dim INTEGER NOT NULL, vec_data BLOB NOT NULL);",
    )?;
    let tx = conn.transaction()?;
    for (chunk_id, values) in vectors {
        let blob: Vec<u8> = values.iter().flat_map(|f| f.to_le_bytes()).collect();
        tx.execute(
            "INSERT OR REPLACE INTO kb_vectors (chunk_id, dim, vec_data) VALUES (?1, ?2, ?3)",
            params![*chunk_id as i64, values.len() as i64, blob],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Lists all documents.
pub fn list_documents(conn: &mut Connection) -> anyhow::Result<Vec<KbDocument>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, title, chunk_count, created_at FROM kb_docs ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(KbDocument {
            id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
            path: r.get(1)?,
            title: r.get(2)?,
            chunk_count: r.get(3)?,
            created_at: r.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Deletes a document and its chunks (FTS rows are removed via the
/// `kb_chunks_ad` trigger).
pub fn delete_document(conn: &mut Connection, doc_id: Uuid) -> anyhow::Result<()> {
    conn.execute("DELETE FROM kb_docs WHERE id = ?1", params![doc_id.to_string()])?;
    conn.execute("DELETE FROM kb_chunks WHERE doc_id = ?1", params![doc_id.to_string()])?;
    Ok(())
}

const RRF_K: f32 = 60.0;
const RRF_TOP: usize = 20;

/// Hybrid search: FTS5 BM25 + vector cosine, fused with Reciprocal Rank
/// Fusion.
pub fn hybrid_search(
    conn: &mut Connection,
    embedder: &dyn Embedder,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<SearchHit>> {
    // ---- full-text candidates (BM25)
    let fts_query = fts_query(query);
    let mut fts_hits: Vec<(u64, String)> = Vec::new();
    if !fts_query.is_empty() {
        let sql = format!(
            "SELECT c.id, c.content FROM kb_fts f JOIN kb_chunks c ON c.id = f.rowid WHERE kb_fts MATCH ?1 ORDER BY rank LIMIT ?2"
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let mut rows = stmt.query(params![fts_query, RRF_TOP as i64])?;
            while let Some(row) = rows.next()? {
                let id: i64 = row.get(0)?;
                let content: String = row.get(1)?;
                fts_hits.push((id as u64, content));
            }
        }
    }
    // LIKE fallback for CJK: the unicode61 tokenizer does not split Chinese,
    // so MATCH misses CJK queries. A substring scan catches them.
    if fts_hits.is_empty() {
        let mut like_hits: Vec<(u64, String)> = Vec::new();
        for term in query.split_whitespace().filter(|t| !t.is_empty()) {
            let like = format!("%{term}%");
            if let Ok(mut stmt) = conn.prepare(
                "SELECT id, content FROM kb_chunks WHERE content LIKE ?1 LIMIT ?2",
            ) {
                if let Ok(mut rows) = stmt.query(params![like, RRF_TOP as i64]) {
                    while let Ok(Some(row)) = rows.next() {
                        let id: i64 = match row.get(0) { Ok(v) => v, Err(_) => continue };
                        let content: String = match row.get(1) { Ok(v) => v, Err(_) => continue };
                        if !like_hits.iter().any(|(i, _)| *i == id as u64) {
                            like_hits.push((id as u64, content));
                        }
                    }
                }
            }
        }
        fts_hits = like_hits;
    }

    // ---- vector candidates (cosine)
    let q_vec = embedder.embed(query);
    let mut vec_hits: Vec<(u64, f32)> = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT chunk_id, dim, vec_data FROM kb_vectors")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let chunk_id: i64 = row.get(0)?;
            let dim: i64 = row.get(1)?;
            let blob: Vec<u8> = row.get(2)?;
            let values: Vec<f32> = blob
                .chunks_exact(4)
                .take(dim as usize)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let sim = cosine_similarity(&q_vec, &super::embedding::Embedding { dim: dim as usize, values });
            if sim > 0.01 {
                vec_hits.push((chunk_id as u64, sim));
            }
        }
    }
    vec_hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    vec_hits.truncate(RRF_TOP);

    // ---- RRF fusion
    let mut rank_scores: std::collections::HashMap<u64, f32> = std::collections::HashMap::new();
    for (rank, (id, _)) in fts_hits.iter().enumerate() {
        *rank_scores.entry(*id).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
    }
    for (rank, (id, _)) in vec_hits.iter().enumerate() {
        *rank_scores.entry(*id).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
    }

    // ---- hydrate hits
    let mut ranked: Vec<(u64, f32)> = rank_scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(limit.max(1));

    let mut hits = Vec::new();
    for (chunk_id, score) in ranked {
        if let Some(hit) = hydrate(conn, chunk_id as i64, score)? {
            hits.push(hit);
        }
    }
    Ok(hits)
}

fn hydrate(conn: &mut Connection, chunk_id: i64, score: f32) -> anyhow::Result<Option<SearchHit>> {
    let row = conn
        .query_row(
            "SELECT c.doc_id, c.chunk_index, c.heading, c.content, d.path, d.title
             FROM kb_chunks c JOIN kb_docs d ON d.id = c.doc_id WHERE c.id = ?1",
            params![chunk_id],
            |r| {
                Ok(SearchHit {
                    doc_id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                    chunk_index: r.get::<_, i64>(1)? as usize,
                    heading: r.get(2)?,
                    content: r.get(3)?,
                    path: r.get(4)?,
                    title: r.get(5)?,
                    score,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// Turns a user query into an FTS5 MATCH expression (quoted terms).
fn fts_query(query: &str) -> String {
    let terms: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    terms.join(" OR ")
}

/// Reads the full content of a document (agent tool `read_knowledge_doc`).
pub fn read_document(conn: &mut Connection, doc_id: Uuid) -> anyhow::Result<Option<(KbDocument, String)>> {
    let doc = conn
        .query_row(
            "SELECT id, path, title, chunk_count, created_at FROM kb_docs WHERE id = ?1",
            params![doc_id.to_string()],
            |r| {
                Ok(KbDocument {
                    id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                    path: r.get(1)?,
                    title: r.get(2)?,
                    chunk_count: r.get(3)?,
                    created_at: r.get(4)?,
                })
            },
        )
        .optional()?;
    let Some(doc) = doc else { return Ok(None) };
    let mut stmt = conn.prepare(
        "SELECT content FROM kb_chunks WHERE doc_id = ?1 ORDER BY chunk_index",
    )?;
    let rows = stmt.query_map(params![doc_id.to_string()], |r| r.get::<_, String>(0))?;
    let mut text = String::new();
    for row in rows {
        if let Ok(chunk) = row {
            text.push_str(&chunk);
            text.push('\n');
        }
    }
    Ok(Some((doc, text)))
}

/// Saves a text document into the knowledge base from a string
/// (agent tool `save_document` / browser→KB handoff).
pub fn save_text_document(
    conn: &mut Connection,
    embedder: &dyn Embedder,
    title: &str,
    text: &str,
) -> anyhow::Result<KbDocument> {
    let doc_id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let chunks = chunk_document(text);

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO kb_docs (id, path, title, chunk_count, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![doc_id.to_string(), format!("memory://{title}"), title, chunks.len(), now],
    )?;
    let mut vectors: Vec<(u64, Vec<f32>)> = Vec::new();
    for chunk in &chunks {
        tx.execute(
            "INSERT INTO kb_chunks (doc_id, chunk_index, heading, content) VALUES (?1, ?2, ?3, ?4)",
            params![doc_id.to_string(), chunk.index as i64, chunk.heading, chunk.content],
        )?;
        let row_id = tx.last_insert_rowid();
        let emb = embedder.embed(&chunk.content);
        vectors.push((row_id as u64, emb.values));
    }
    tx.commit()?;

    store_vectors(conn, &vectors)?;
    Ok(KbDocument { id: doc_id, path: format!("memory://{title}"), title: title.to_string(), chunk_count: chunks.len(), created_at: now })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kb::embedding::HashedEmbedder;
    use std::fs;

    fn kb() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_kb(dir.path()).unwrap();
        ensure_fts_triggers(&conn).unwrap();
        (dir, conn)
    }

    #[test]
    fn ingest_and_list() {
        let (_dir, mut conn) = kb();
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("note.md");
        fs::write(&p, "# 知屿\n\n知屿是一个纯本地的双模式 Harness 应用。").unwrap();
        let doc = ingest_document(&mut conn, &HashedEmbedder::new(), &p, "note").unwrap();
        assert_eq!(doc.title, "note");
        let docs = list_documents(&mut conn).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id, doc.id);
    }

    #[test]
    fn hybrid_search_hits_the_right_doc() {
        let (_dir, mut conn) = kb();
        let d = tempfile::tempdir().unwrap();

        let rust = d.path().join("rust.md");
        fs::write(&rust, "# Rust\nRust 的所有权系统保证了内存安全，借用检查器在编译期阻止悬垂引用。").unwrap();
        ingest_document(&mut conn, &HashedEmbedder::new(), &rust, "rust").unwrap();

        let weather = d.path().join("weather.md");
        fs::write(&weather, "# 天气\n明天的天气预报显示多云转晴，气温回升。").unwrap();
        ingest_document(&mut conn, &HashedEmbedder::new(), &weather, "weather").unwrap();

        let hits = hybrid_search(&mut conn, &HashedEmbedder::new(), "Rust 所有权 借用", 3).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].title, "rust", "rust doc should rank first: {:?}", hits.iter().map(|h| &h.title).collect::<Vec<_>>());
        assert!(hits[0].content.contains("所有权"));
    }

    #[test]
    fn save_text_document_and_read() {
        let (_dir, mut conn) = kb();
        let doc = save_text_document(&mut conn, &HashedEmbedder::new(), "浏览器保存", "网页正文：知屿支持内置浏览器研究并保存到知识库。").unwrap();
        let (loaded, text) = read_document(&mut conn, doc.id).unwrap().unwrap();
        assert_eq!(loaded.title, "浏览器保存");
        assert!(text.contains("内置浏览器"));
    }

    #[test]
    fn delete_removes_document() {
        let (_dir, mut conn) = kb();
        let doc = save_text_document(&mut conn, &HashedEmbedder::new(), "临时", "要被删除的内容").unwrap();
        delete_document(&mut conn, doc.id).unwrap();
        assert!(list_documents(&mut conn).unwrap().is_empty());
        assert!(read_document(&mut conn, doc.id).unwrap().is_none());
    }
}
