use std::path::{Path, PathBuf};

pub fn find_in_path(name: &str, path_value: &str) -> Option<PathBuf> {
    path_value
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_executable_in_first_matching_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("tool");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let path_value = format!("/nonexistent:{}", tmp.path().display());
        assert_eq!(find_in_path("tool", &path_value), Some(bin));
    }

    #[test]
    fn returns_none_when_missing_or_empty_path() {
        assert_eq!(find_in_path("tool", "/nonexistent"), None);
        assert_eq!(find_in_path("tool", ""), None);
    }
}
