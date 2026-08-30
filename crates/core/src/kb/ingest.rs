//! Document ingestion: reads md/txt/code files and extracts text from
//! PDF/Word documents (lightweight, dependency-free extractors).

use std::fs;
use std::path::{Path, PathBuf};

/// File extensions treated as plain text (markdown / text / code).
const TEXT_EXTENSIONS: &[&str] = &[
    "md", "markdown", "txt", "rst", "adoc", "log",
    "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "kt", "c", "h", "cpp", "hpp",
    "rb", "php", "sh", "ps1", "sql", "json", "yaml", "yml", "toml", "ini", "cfg", "xml",
    "html", "css", "scss", "vue", "svelte", "zig", "swift",
];

/// Returns true when the path is a file we can ingest.
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| TEXT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()) || e.eq_ignore_ascii_case("pdf") || e.eq_ignore_ascii_case("docx"))
        .unwrap_or(false)
}

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported file type: {0}")]
    Unsupported(PathBuf),
    #[error("pdf extraction failed: {0}")]
    Pdf(String),
}

/// Reads the text content of a document, dispatching by extension.
pub fn extract_text(path: &Path) -> Result<String, IngestError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => extract_pdf(path),
        "docx" => extract_docx(path),
        _ if TEXT_EXTENSIONS.contains(&ext.as_str()) => Ok(fs::read_to_string(path)?),
        _ => Err(IngestError::Unsupported(path.to_path_buf())),
    }
}

/// A minimal PDF text extractor: scans content streams (between `stream` and
/// `endstream`) for `(…) Tj` show operators. Good enough for text-based PDFs
/// (the plan's honest boundary: complex layouts are a follow-up).
fn extract_pdf(path: &Path) -> Result<String, IngestError> {
    let bytes = fs::read(path)?;
    let eof = find_bytes(&bytes, b"%%EOF").ok_or_else(|| IngestError::Pdf("missing EOF marker".into()))?;
    let data = &bytes[..eof];
    let mut out = String::new();
    let mut rest = data;
    while let Some(stream_start) = find_bytes(rest, b"stream") {
        // skip to after "stream\r\n" / "stream\n"
        let content_start = stream_start + 6;
        let Some(end_rel) = find_bytes(&rest[content_start..], b"endstream") else { break };
        let stream_content = &rest[content_start..content_start + end_rel];
        out.push_str(&extract_show_strings(stream_content));
        rest = &rest[content_start + end_rel + 9..];
    }
    if out.trim().is_empty() {
        return Err(IngestError::Pdf("no extractable text (scanned image?)".into()));
    }
    Ok(out)
}

/// Extracts text from `(…) Tj` / `[…] TJ` show operators in a content stream.
fn extract_show_strings(stream: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < stream.len() {
        if stream[i] == b'(' {
            let mut j = i + 1;
            let mut escaped = false;
            while j < stream.len() {
                let c = stream[j];
                if c == b'\\' && !escaped {
                    escaped = true;
                } else if c == b')' && !escaped {
                    break;
                } else {
                    escaped = false;
                }
                j += 1;
            }
            if j < stream.len() {
                let s = String::from_utf8_lossy(&stream[i + 1..j]).to_string();
                out.push_str(&s.replace("\\(", "(").replace("\\)", ")").replace("\\\\", "\\"));
                out.push(' ');
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// A minimal DOCX extractor: the file is a zip; `word/document.xml` holds the
/// text inside `<w:t>` elements. Paragraph breaks map to `<w:p` openers.
fn extract_docx(path: &Path) -> Result<String, IngestError> {
    let bytes = fs::read(path)?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| IngestError::Pdf(format!("not a valid docx: {e}")))?;
    let mut document = String::new();
    for name in ["word/document.xml", "word/document2.xml"] {
        if let Ok(mut file) = archive.by_name(name) {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut buf)
                .map_err(|e| IngestError::Pdf(format!("docx read failed: {e}")))?;
            document = String::from_utf8_lossy(&buf).to_string();
            break;
        }
    }
    if document.is_empty() {
        return Err(IngestError::Pdf("docx has no document.xml".into()));
    }

    // split into paragraphs on <w:p / <w:p> openers
    let mut out = String::new();
    let mut rest = document.as_str();
    while !rest.is_empty() {
        let open = rest
            .find("<w:p ")
            .or_else(|| rest.find("<w:p>"))
            .or_else(|| rest.find("<w:p/>"))
            .or_else(|| rest.find("<w:tr "));
        let (paragraph, tail) = match open {
            Some(idx) => (&rest[..idx], &rest[idx + 1..]),
            None => (rest, ""),
        };
        let text = extract_wt(paragraph);
        if !text.trim().is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text.trim());
        }
        rest = tail;
    }
    if out.trim().is_empty() {
        // fallback: scan the whole document for <w:t> content
        out = extract_wt(&document);
    }
    Ok(out)
}

/// Extracts the concatenated text of all `<w:t>…</w:t>` runs in `xml`.
fn extract_wt(xml: &str) -> String {
    let mut out = String::new();
    let mut tail = xml;
    while let Some(t) = tail.find("<w:t") {
        let after_open = &tail[t..];
        let Some(gt) = after_open.find('>') else { break };
        let content_start = gt + 1;
        let Some(close) = after_open[content_start..].find("</w:t>") else { break };
        out.push_str(&after_open[content_start..content_start + close]);
        tail = &after_open[content_start + close + 6..];
    }
    out
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_extensions() {
        assert!(is_supported(Path::new("a.md")));
        assert!(is_supported(Path::new("src/main.rs")));
        assert!(is_supported(Path::new("doc.PDF")));
        assert!(is_supported(Path::new("report.docx")));
        assert!(!is_supported(Path::new("archive.zip")));
        assert!(!is_supported(Path::new("noext")));
    }

    #[test]
    fn reads_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("note.md");
        fs::write(&p, "# 标题\n内容").unwrap();
        assert_eq!(extract_text(&p).unwrap(), "# 标题\n内容");
    }

    #[test]
    fn rejects_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.zip");
        fs::write(&p, b"PK\x03\x04").unwrap();
        assert!(matches!(extract_text(&p), Err(IngestError::Unsupported(_))));
    }

    #[test]
    fn extracts_simple_pdf_text() {
        // a hand-crafted minimal PDF with a Tj show operator
        let pdf = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n2 0 obj\n<< >>\nstream\n(Hello PDF world) Tj\nendstream\nendobj\n%%EOF\n";
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.pdf");
        fs::write(&p, pdf).unwrap();
        let text = extract_text(&p).unwrap();
        assert!(text.contains("Hello PDF world"));
    }

    #[test]
    fn empty_pdf_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.pdf");
        fs::write(&p, b"not a pdf").unwrap();
        assert!(matches!(extract_text(&p), Err(IngestError::Pdf(_))));
    }
}
