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
    crate::util::write_file_if_changed(path, &render()).map(|_| ())
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

    #[test]
    fn rendered_base_config_is_valid_toml() {
        let parsed: toml::Value =
            toml::from_str(&render()).expect("base config must be valid TOML");
        assert_eq!(
            parsed.get("skip_notify").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(parsed.get("no_retry").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            parsed.get("no_self_update").and_then(|v| v.as_bool()),
            Some(true)
        );
    }
}
