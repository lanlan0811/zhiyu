//! Skill library: discovers `SKILL.md` files across the standard roots
//! (user-level + project-level), parses their frontmatter and lists them for
//! the agent to read.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The name a skill definition file must carry.
pub const SKILL_FILE: &str = "SKILL.md";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    /// Skill id (the directory name).
    pub id: String,
    /// Name from frontmatter, if any.
    pub name: Option<String>,
    /// Description from frontmatter, if any.
    pub description: Option<String>,
    /// Absolute path to the SKILL.md.
    pub path: PathBuf,
    /// Root category: user-level roots vs project-level.
    pub origin: SkillOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillOrigin {
    User,
    Project,
}

/// Discovers skills under `roots` (each a directory to scan recursively).
pub fn discover(roots: &[PathBuf]) -> Vec<Skill> {
    let mut skills = BTreeMap::new();
    for root in roots {
        scan_root(root, &mut skills);
    }
    skills.into_values().collect()
}

fn scan_root(root: &Path, out: &mut BTreeMap<String, Skill>) {
    if !root.is_dir() {
        return;
    }
    for entry in walkdir::WalkDir::new(root).max_depth(4).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.file_name() == SKILL_FILE {
            let dir = entry.path().parent().unwrap_or(entry.path());
            let id = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".into());
            let (name, description) = parse_frontmatter(&entry.path());
            out.insert(
                id.clone(),
                Skill {
                    id,
                    name,
                    description,
                    path: entry.path().to_path_buf(),
                    origin: SkillOrigin::User,
                },
            );
        }
    }
}

/// Parses the `name:` / `description:` frontmatter lines of a SKILL.md.
fn parse_frontmatter(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return (None, None);
    };
    let mut name = None;
    let mut description = None;
    if let Some(rest) = content.strip_prefix("---") {
        for line in rest.lines().take(30).skip_while(|l| !l.trim().is_empty() || *l == "---") {
            if line.trim() == "---" {
                break;
            }
            if let Some(v) = line.trim().strip_prefix("name:") {
                name = Some(v.trim().trim_matches('"').trim_matches('\'').to_string());
            } else if let Some(v) = line.trim().strip_prefix("description:") {
                description = Some(v.trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }
    }
    (name, description)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn skill_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("skills");
        fs::create_dir_all(root.join("control-browser")).unwrap();
        fs::write(
            root.join("control-browser").join(SKILL_FILE),
            "---\nname: control-browser\ndescription: 浏览器控制方法论\n---\n# Content",
        )
        .unwrap();
        fs::create_dir_all(root.join("web-gui-tester")).unwrap();
        fs::write(root.join("web-gui-tester").join(SKILL_FILE), "---\ndescription: GUI 黑盒测试\n---\n# Content").unwrap();
        fs::create_dir_all(root.join("no-frontmatter")).unwrap();
        fs::write(root.join("no-frontmatter").join(SKILL_FILE), "# No frontmatter").unwrap();
        dir
    }

    #[test]
    fn discovers_skills_and_parses_frontmatter() {
        let dir = skill_dir();
        let skills = discover(&[dir.path().join("skills")]);
        assert_eq!(skills.len(), 3);
        let cb = skills.iter().find(|s| s.id == "control-browser").unwrap();
        assert_eq!(cb.name.as_deref(), Some("control-browser"));
        assert_eq!(cb.description.as_deref(), Some("浏览器控制方法论"));
        let wgt = skills.iter().find(|s| s.id == "web-gui-tester").unwrap();
        assert_eq!(wgt.name, None); // no name, only description
        assert_eq!(wgt.description.as_deref(), Some("GUI 黑盒测试"));
        let nf = skills.iter().find(|s| s.id == "no-frontmatter").unwrap();
        assert_eq!(nf.name, None);
        assert_eq!(nf.description, None);
    }

    #[test]
    fn missing_root_is_empty() {
        assert!(discover(&[PathBuf::from("C:/definitely/not/here")]).is_empty());
    }
}
