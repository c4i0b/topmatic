use std::fs;
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;

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
