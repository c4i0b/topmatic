pub fn render() -> String {
    [
        "# Managed by topmatic. This file isolates topmatic from your own topgrade",
        "# configuration; manual edits will be overwritten.",
        "assume_yes = true",
        "cleanup = true",
        "ask_retry = false",
        "auto_retry = 1",
        "notify_end = \"never\"",
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
        let parsed: toml::Value = toml::from_str(&render()).unwrap();
        assert_eq!(
            parsed.get("assume_yes").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(parsed.get("cleanup").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            parsed.get("ask_retry").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            parsed.get("auto_retry").and_then(|v| v.as_integer()),
            Some(1)
        );
        assert_eq!(
            parsed.get("notify_end").and_then(|v| v.as_str()),
            Some("never")
        );
        assert_eq!(
            parsed.get("no_self_update").and_then(|v| v.as_bool()),
            Some(true)
        );
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
    fn never_uses_deprecated_or_legacy_keys() {
        let text = render();
        assert!(!text.contains("skip_notify"), "deprecated in topgrade");
        assert!(!text.contains("no_retry"), "legacy alias of ask_retry");
    }
}
