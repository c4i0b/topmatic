use std::fs;
use std::path::{Path, PathBuf};

pub fn write_file_if_changed(path: &Path, content: &str) -> std::io::Result<bool> {
    if let Ok(existing) = fs::read_to_string(path)
        && existing == content
    {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(true)
}

pub fn unique_sibling(path: &Path, suffix: &str) -> PathBuf {
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S%.3f");
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut candidate = path.with_file_name(format!("{file_name}.{suffix}-{timestamp}"));
    let mut counter = 0u32;
    while candidate.exists() {
        counter += 1;
        candidate = path.with_file_name(format!("{file_name}.{suffix}-{timestamp}-{counter}"));
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_sibling_never_collides_and_shares_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let first = unique_sibling(&path, "broken");
        fs::write(&first, "x").unwrap();
        let second = unique_sibling(&path, "broken");
        assert_ne!(first, second);
        assert_eq!(first.parent(), Some(tmp.path()));
        assert!(
            first
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("config.toml.broken-")
        );
    }

    #[test]
    fn writes_new_file_and_reports_change() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a").join("f.conf");
        assert!(write_file_if_changed(&path, "x").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "x");
    }

    #[test]
    fn skips_write_when_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("f.conf");
        assert!(write_file_if_changed(&path, "x").unwrap());
        assert!(!write_file_if_changed(&path, "x").unwrap());
    }
}
