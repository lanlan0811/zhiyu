//! Knowledge base (daily mode): ingestion, chunking, full-text + vector
//! hybrid retrieval (RRF), management and agent tools.

pub mod chunking;
pub mod embedding;
pub mod index;
pub mod ingest;

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use uuid::Uuid;

pub use chunking::{Chunk, chunk_document};
pub use embedding::{Embedder, HashedEmbedder, cosine_similarity};
pub use index::{KbDocument, SearchHit, hybrid_search, ingest_document, list_documents, delete_document, open_kb, read_document, save_text_document};
pub use ingest::{extract_text, is_supported, IngestError};

/// The knowledge-base service: owns the SQLite connection and the embedder.
pub struct KnowledgeBase {
    conn: Mutex<Connection>,
    embedder: Box<dyn Embedder>,
}

impl KnowledgeBase {
    /// Opens the KB under `dir` with the default embedder.
    pub fn open(dir: &PathBuf) -> anyhow::Result<Self> {
        let conn = open_kb(dir)?;
        Ok(KnowledgeBase { conn: Mutex::new(conn), embedder: crate::kb::embedding::default_embedder() })
    }

    /// Opens with an explicit embedder (tests).
    pub fn open_with(dir: &PathBuf, embedder: Box<dyn Embedder>) -> anyhow::Result<Self> {
        let conn = open_kb(dir)?;
        Ok(KnowledgeBase { conn: Mutex::new(conn), embedder })
    }

    pub fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<SearchHit>> {
        hybrid_search(&mut self.conn.lock().unwrap(), self.embedder.as_ref(), query, limit)
    }

    pub fn import_file(&self, path: &PathBuf) -> anyhow::Result<KbDocument> {
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "document".into());
        ingest_document(&mut self.conn.lock().unwrap(), self.embedder.as_ref(), path, &title)
    }

    pub fn list(&self) -> anyhow::Result<Vec<KbDocument>> {
        list_documents(&mut self.conn.lock().unwrap())
    }

    pub fn delete(&self, doc_id: Uuid) -> anyhow::Result<()> {
        delete_document(&mut self.conn.lock().unwrap(), doc_id)
    }

    pub fn read(&self, doc_id: Uuid) -> anyhow::Result<Option<(KbDocument, String)>> {
        read_document(&mut self.conn.lock().unwrap(), doc_id)
    }

    /// Agent tool `save_document`: persists text into the index.
    pub fn save_document(&self, title: &str, text: &str) -> anyhow::Result<KbDocument> {
        save_text_document(&mut self.conn.lock().unwrap(), self.embedder.as_ref(), title, text)
    }

    /// Agent tool `read_knowledge_doc` content by id (falls back to title
    /// match for convenience).
    pub fn read_by_id_or_title(&self, id_or_title: &str) -> anyhow::Result<Option<(KbDocument, String)>> {
        let conn = &mut self.conn.lock().unwrap();
        if let Ok(id) = Uuid::parse_str(id_or_title) {
            if let Some(hit) = read_document(conn, id)? {
                return Ok(Some(hit));
            }
        }
        // title match: first document whose title contains the query
        for doc in list_documents(conn)? {
            if doc.title.contains(id_or_title) {
                return read_document(conn, doc.id);
            }
        }
        Ok(None)
    }

    /// Rebuilds the index from the documents folder: wipes all documents then
    /// re-ingests every supported file under `folder` (recursively).
    pub fn reindex(&self, folder: &PathBuf) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("DELETE FROM kb_docs; DELETE FROM kb_chunks; DELETE FROM kb_vectors;")?;
        // clear the FTS table (delete-all command, drained via query)
        if let Ok(mut stmt) = conn.prepare("INSERT INTO kb_fts(kb_fts) VALUES('delete-all')") {
            let _ = stmt.query([]);
        }
        drop(conn);
        let mut count = 0;
        if folder.is_dir() {
            for entry in walkdir::WalkDir::new(folder).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && crate::kb::ingest::is_supported(path) {
                    let title = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "document".into());
                    ingest_document(&mut self.conn.lock().unwrap(), self.embedder.as_ref(), path, &title)?;
                    count += 1;
                }
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb() -> (tempfile::TempDir, KnowledgeBase) {
        let dir = tempfile::tempdir().unwrap();
        let kb = KnowledgeBase::open_with(&dir.path().join("kb"), Box::new(HashedEmbedder::new())).unwrap();
        (dir, kb)
    }

    #[test]
    fn full_cycle_import_search_read_delete() {
        let (_dir, kb) = kb();
        // import a temp file
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("doc.md");
        std::fs::write(&p, "# 标题\n\n知屿知识库支持全文与向量混合检索。").unwrap();
        let doc = kb.import_file(&p).unwrap();
        assert!(doc.chunk_count >= 1);

        let hits = kb.search("知屿 知识库 混合检索", 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].content.contains("知识库"));

        let (loaded, text) = kb.read(doc.id).unwrap().unwrap();
        assert_eq!(loaded.title, "doc");
        assert!(text.contains("混合检索"));

        kb.delete(doc.id).unwrap();
        assert!(kb.list().unwrap().is_empty());
    }

    #[test]
    fn agent_tools_save_and_read() {
        let (_dir, kb) = kb();
        let doc = kb.save_document("浏览器笔记", "网页正文：Rust 生命周期详解。").unwrap();
        let by_id = kb.read_by_id_or_title(&doc.id.to_string()).unwrap().unwrap();
        assert_eq!(by_id.0.title, "浏览器笔记");
        let by_title = kb.read_by_id_or_title("浏览器").unwrap().unwrap();
        assert_eq!(by_title.0.id, doc.id);
    }
}
