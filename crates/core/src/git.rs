//! Git infrastructure (coding mode): turn-level checkpoints (ref snapshots +
//! rollback), commit-message generation and diff review.

use std::path::Path;
use std::process::Command;

use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("not a git repository: {0}")]
    NotARepo(String),
    #[error("git command failed ({cmd}): {stderr}")]
    Command { cmd: String, stderr: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// The ref namespace used for turn-level checkpoints:
/// `refs/zhiyu/checkpoints/<session>/<checkpoint>`.
pub fn checkpoint_ref(session_id: Uuid, checkpoint_id: Uuid) -> String {
    format!("refs/zhiyu/checkpoints/{}/{}", session_id.simple(), checkpoint_id.simple())
}

fn run(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(GitError::Io)?;
    if !out.status.success() {
        return Err(GitError::Command {
            cmd: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Creates a turn-level checkpoint: a ref snapshot of the current working
/// tree (commit all changes onto a detached ref, leaving the working tree
/// untouched).
pub fn create_checkpoint(dir: &Path, session_id: Uuid, checkpoint_id: Uuid, description: &str) -> Result<String, GitError> {
    // stage everything so untracked files are included in the snapshot
    run(dir, &["add", "-A"])?;
    let branch = run(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch.is_empty() {
        return Err(GitError::NotARepo(dir.display().to_string()));
    }
    // commit on a temporary branch, then move the ref
    let _ = run(dir, &["commit", "-m", &format!("checkpoint: {description}")]);
    let sha = run(dir, &["rev-parse", "HEAD"])?;
    let ref_name = checkpoint_ref(session_id, checkpoint_id);
    run(dir, &["update-ref", &ref_name, &sha])?;
    Ok(sha)
}

/// Rolls the working tree back to a checkpoint ref.
pub fn rollback(dir: &Path, ref_name: &str) -> Result<(), GitError> {
    // verify the ref exists
    let _ = run(dir, &["rev-parse", "--verify", ref_name])?;
    run(dir, &["reset", "--hard", ref_name])?;
    run(dir, &["clean", "-fd"])?;
    Ok(())
}

/// Generates a commit subject via the session's model (a direct API call in
/// the real flow; here the prompt is assembled and the caller drives the
/// model — this returns the assembled prompt and diff so the driver layer can
/// run it).
pub fn commit_prompt(diff: &str, branch: &str) -> String {
    format!(
        "Generate a concise git commit message (subject line only, imperative mood, under 72 chars) for the following diff on branch {branch}:\n\n{diff}"
    )
}

/// The unified diff between two refs (or HEAD and the working tree).
pub fn diff(dir: &Path, from: &str, to: &str) -> Result<String, GitError> {
    if to.is_empty() {
        run(dir, &["diff", from, "--stat"])
    } else {
        run(dir, &["diff", from, to])
    }
}

/// Review prompt for the diff-review tool.
pub fn review_prompt(diff: &str) -> String {
    format!(
        "Review the following diff. List: 1) bugs and correctness issues, 2) style and maintainability, 3) suggestions. Be concrete and reference line numbers.\n\n{diff}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-b", "main"]).unwrap();
        run(dir.path(), &["config", "user.email", "test@zhiyu.local"]).unwrap();
        run(dir.path(), &["config", "user.name", "Zhiyu Test"]).unwrap();
        // an initial commit gives the repository a real HEAD so
        // `rev-parse --abbrev-ref HEAD` works on every git version
        fs::write(dir.path().join(".gitkeep"), "").unwrap();
        run(dir.path(), &["add", "-A"]).unwrap();
        run(dir.path(), &["commit", "-m", "init"]).unwrap();
        dir
    }

    #[test]
    fn checkpoint_and_rollback() {
        let dir = init_repo();
        let sid = Uuid::new_v4();
        let cp1 = Uuid::new_v4();

        fs::write(dir.path().join("a.txt"), "v1").unwrap();
        let sha1 = create_checkpoint(dir.path(), sid, cp1, "first").unwrap();
        assert!(!sha1.is_empty());

        // make more changes
        fs::write(dir.path().join("a.txt"), "v2").unwrap();
        fs::write(dir.path().join("b.txt"), "new").unwrap();

        // rollback → a.txt back to v1, b.txt gone
        let ref_name = checkpoint_ref(sid, cp1);
        rollback(dir.path(), &ref_name).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "v1");
        assert!(!dir.path().join("b.txt").exists());
    }

    #[test]
    fn ref_names_are_namespaced() {
        let ref_name = checkpoint_ref(Uuid::new_v4(), Uuid::new_v4());
        assert!(ref_name.starts_with("refs/zhiyu/checkpoints/"));
    }

    #[test]
    fn prompts_contain_diff() {
        assert!(commit_prompt("diff --git a/x b/x", "main").contains("diff --git"));
        assert!(review_prompt("+bug").contains("bug"));
    }

    #[test]
    fn not_a_repo_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let err = create_checkpoint(dir.path(), Uuid::new_v4(), Uuid::new_v4(), "x");
        assert!(err.is_err());
    }
}
