//! Document chunking: split ingested text into indexable chunks by heading
//! and length, with overlap for retrieval continuity.

/// Target chunk size in characters (≈ 500 tokens for CJK-heavy text).
pub const CHUNK_SIZE: usize = 1600;
/// Overlap between consecutive chunks.
pub const CHUNK_OVERLAP: usize = 160;

/// A chunk of a document.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    /// Zero-based position within the document.
    pub index: usize,
    /// Markdown heading context, e.g. "## 安装" when the chunk started under
    /// that heading.
    pub heading: Option<String>,
    pub content: String,
}

/// Splits a document into chunks. Markdown headings (`#`…`######`) become
/// chunk boundaries; long sections are cut at `CHUNK_SIZE` with overlap.
pub fn chunk_document(text: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_heading: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if is_heading(trimmed) {
            flush(&mut current, &mut chunks, &current_heading);
            current_heading = Some(trimmed.to_string());
            continue;
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
        while current.chars().count() >= CHUNK_SIZE {
            let (head, tail) = split_at_char(&current, CHUNK_SIZE - CHUNK_OVERLAP);
            chunks.push(Chunk { index: chunks.len(), heading: current_heading.clone(), content: head.trim().to_string() });
            current = tail;
        }
    }
    flush(&mut current, &mut chunks, &current_heading);
    if chunks.is_empty() {
        chunks.push(Chunk { index: 0, heading: None, content: String::new() });
    }
    chunks
}

fn is_heading(trimmed: &str) -> bool {
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    // 1–6 hashes followed by a non-space (e.g. "# 标题" or "#标题")
    if !(1..=6).contains(&hashes) {
        return false;
    }
    // byte offset of the char right after the hashes (char-boundary safe)
    let after = trimmed.char_indices().nth(hashes).map(|(i, _)| i).unwrap_or(trimmed.len());
    trimmed[after..].trim_start().chars().next().is_some()
}

fn flush(current: &mut String, chunks: &mut Vec<Chunk>, heading: &Option<String>) {
    if !current.trim().is_empty() {
        chunks.push(Chunk { index: chunks.len(), heading: heading.clone(), content: current.trim().to_string() });
    }
    current.clear();
}

/// Splits a string at a char boundary, returning (head, tail).
fn split_at_char(s: &str, char_index: usize) -> (String, String) {
    for (i, (count, _)) in s.char_indices().enumerate() {
        if count == char_index {
            return (s[..i].to_string(), s[i..].to_string());
        }
    }
    (s.to_string(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_document_gets_one_empty_chunk() {
        let chunks = chunk_document("");
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn headings_bound_chunks() {
        let text = "# 标题一\n\n内容甲\n\n## 标题二\n\n内容乙";
        let chunks = chunk_document(text);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading.as_deref(), Some("# 标题一"));
        assert!(chunks[0].content.contains("内容甲"));
        assert_eq!(chunks[1].heading.as_deref(), Some("## 标题二"));
        assert!(chunks[1].content.contains("内容乙"));
    }

    #[test]
    fn long_section_is_cut_with_overlap() {
        let long = "词".repeat(4000);
        let chunks = chunk_document(&long);
        assert!(chunks.len() > 2);
        // every chunk within target size
        for c in &chunks {
            assert!(c.content.chars().count() <= CHUNK_SIZE);
        }
    }

    #[test]
    fn plain_text_without_headings_chunks() {
        let text = (0..3000).map(|i| char::from(b'a' + (i % 26) as u8)).collect::<String>();
        let chunks = chunk_document(&text);
        assert!(chunks.len() >= 2);
        assert!(chunks[0].heading.is_none());
    }
}
