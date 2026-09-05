use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<String>,
}

impl RepoEntry {
    pub fn pull_only(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            apply: None,
        }
    }

    pub fn with_apply(path: impl Into<String>, apply: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            apply: Some(apply.into()),
        }
    }
}

pub fn expand_home(path: &str, home: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else if path == "~" {
        home.to_string()
    } else {
        path.to_string()
    }
}

pub fn is_git_repo(path: &str, home: &str) -> bool {
    let expanded = expand_home(path, home);
    let path = std::path::Path::new(&expanded);
    path.is_dir() && path.join(".git").exists()
}

pub fn scan(root: &std::path::Path, max_depth: usize) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    walk(root, max_depth, &mut found);
    found.sort();
    found
}

fn walk(dir: &std::path::Path, depth: usize, found: &mut Vec<std::path::PathBuf>) {
    if depth == 0 || !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        if path.join(".git").exists() {
            found.push(path.clone());
        }
        walk(&path, depth - 1, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_home_prefixes() {
        assert_eq!(
            expand_home("~/dev/tool", "/home/caio"),
            "/home/caio/dev/tool"
        );
        assert_eq!(expand_home("~", "/home/caio"), "/home/caio");
        assert_eq!(expand_home("/abs/path", "/home/caio"), "/abs/path");
        assert_eq!(expand_home("~/a~b", "/home/caio"), "/home/caio/a~b");
    }

    #[test]
    fn detects_existing_git_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let repo = home.join("dev").join("tool");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        assert!(is_git_repo("~/dev/tool", home.to_str().unwrap()));
        assert!(is_git_repo(repo.to_str().unwrap(), home.to_str().unwrap()));
        assert!(!is_git_repo("~/dev/missing", home.to_str().unwrap()));
        let plain = home.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert!(!is_git_repo(
            plain.to_str().unwrap(),
            home.to_str().unwrap()
        ));
    }

    #[test]
    fn scan_finds_repos_skipping_noise() {
        let tmp = tempfile::tempdir().unwrap();
        for repo in ["a", "b/c", "skip"] {
            std::fs::create_dir_all(tmp.path().join(repo).join(".git")).unwrap();
        }
        std::fs::create_dir_all(tmp.path().join(".hidden").join(".git")).unwrap();
        std::fs::create_dir_all(tmp.path().join("node_modules").join("x").join(".git")).unwrap();
        std::fs::create_dir_all(tmp.path().join("notgit")).unwrap();

        let found = scan(tmp.path(), 3);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(tmp.path()).unwrap().display().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["a".to_string(), "b/c".to_string(), "skip".to_string()]
        );
    }

    #[test]
    fn scan_respects_depth_limit() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("x").join("y").join("z").join(".git")).unwrap();
        assert!(scan(tmp.path(), 2).is_empty());
        assert_eq!(scan(tmp.path(), 3).len(), 1);
    }

    #[test]
    fn repo_entries_round_trip_through_toml() {
        let entry = RepoEntry::with_apply("~/dot", "stow -t ~ bash");
        let text = toml::to_string(&entry).unwrap();
        let back: RepoEntry = toml::from_str(&text).unwrap();
        assert_eq!(entry, back);
        let pull = RepoEntry::pull_only("~/dev/tool");
        let text = toml::to_string(&pull).unwrap();
        assert!(!text.contains("apply"));
    }
}
