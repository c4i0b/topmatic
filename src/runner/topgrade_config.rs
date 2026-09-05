pub fn render() -> String {
    [
        "# Managed by topmatic. This file isolates topmatic from your own topgrade",
        "# configuration; manual edits will be overwritten.",
        "skip_notify = true",
        "no_retry = true",
        "no_self_update = true",
    ]
    .join("\n")
        + "\n"
}

pub fn write_if_changed(path: &std::path::Path) -> std::io::Result<()> {
    let content = render();
    if let Ok(existing) = std::fs::read_to_string(path)
        && existing == content
    {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unattended_defaults() {
        let text = render();
        assert!(text.contains("skip_notify = true"));
        assert!(text.contains("no_retry = true"));
        assert!(text.contains("no_self_update = true"));
    }

    #[test]
    fn write_if_changed_creates_and_preserves() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("topgrade.toml");
        write_if_changed(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), render());
        let first_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        write_if_changed(&path).unwrap();
        let second_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(first_mtime, second_mtime);
    }
}
