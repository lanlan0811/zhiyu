//! Coding-mode workspace operations: directory tree, file read/write, diff
//! review.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("path escapes the workspace: {0}")]
    Escape(PathBuf),
    #[error("not a directory: {0}")]
    NotADir(PathBuf),
    #[error("path not found: {0}")]
    NotFound(PathBuf),
    #[error("path is a directory: {0}")]
    IsDir(PathBuf),
}

/// A directory entry in the tree view.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

/// Lists a workspace directory (single level, no hidden files).
pub fn list_dir(root: &Path, rel: Option<&str>) -> Result<Vec<DirEntry>, WorkspaceError> {
    let dir = resolve(root, rel)?;
    if !dir.is_dir() {
        return Err(WorkspaceError::NotADir(dir));
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type()?;
        entries.push(DirEntry {
            name: name.clone(),
            path: rel_path(root, &entry.path()),
            is_dir: file_type.is_dir(),
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

/// Reads a file (text) inside the workspace.
pub fn read_file(root: &Path, rel: &str) -> Result<String, WorkspaceError> {
    let path = resolve(root, Some(rel))?;
    if path.is_dir() {
        return Err(WorkspaceError::IsDir(path));
    }
    if !path.exists() {
        return Err(WorkspaceError::NotFound(path));
    }
    Ok(fs::read_to_string(&path)?)
}

/// Writes a file (text) inside the workspace, creating parents.
pub fn write_file(root: &Path, rel: &str, content: &str) -> Result<(), WorkspaceError> {
    let path = resolve(root, Some(rel))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    Ok(())
}

/// Resolves a relative path against the root, refusing traversal outside.
pub fn resolve(root: &Path, rel: Option<&str>) -> Result<PathBuf, WorkspaceError> {
    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let candidate = match rel {
        None | Some("") | Some(".") => root_canon.clone(),
        Some(rel) => {
            let joined = root_canon.join(rel);
            // normalize .. components
            normalize(&joined)
        }
    };
    if !candidate.starts_with(&root_canon) {
        return Err(WorkspaceError::Escape(candidate));
    }
    Ok(candidate)
}

/// Lexically normalizes `..` without touching the filesystem. Preserves the
/// root/prefix components so the result stays comparable with the canonical
/// workspace root on Windows (`C:\…` stays prefixed).
fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<std::path::Component> = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            other => parts.push(other),
        }
    }
    parts.into_iter().collect()
}

/// The workspace-relative path of an absolute path inside the root.
fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("README.md"), "# t").unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap(); // hidden
        dir
    }

    #[test]
    fn lists_dir_sorted_dirs_first() {
        let dir = workspace();
        let entries = list_dir(dir.path(), None).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["src", "README.md"]); // hidden .git excluded
        assert!(entries[0].is_dir);
    }

    #[test]
    fn reads_and_writes_files() {
        let dir = workspace();
        assert_eq!(read_file(dir.path(), "src/main.rs").unwrap(), "fn main() {}");
        write_file(dir.path(), "src/lib.rs", "pub fn f() {}").unwrap();
        assert_eq!(read_file(dir.path(), "src/lib.rs").unwrap(), "pub fn f() {}");
    }

    #[test]
    fn refuses_traversal() {
        let dir = workspace();
        let err = read_file(dir.path(), "../outside.txt");
        assert!(matches!(err, Err(WorkspaceError::Escape(_))));
    }

    #[test]
    fn resolve_normalizes_dotdot() {
        let dir = workspace();
        let p = resolve(dir.path(), Some("src/../README.md")).unwrap();
        assert!(p.ends_with("README.md"));
    }

    #[test]
    fn missing_file_is_not_found() {
        let dir = workspace();
        assert!(matches!(read_file(dir.path(), "nope.txt"), Err(WorkspaceError::NotFound(_))));
    }
}
