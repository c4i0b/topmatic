pub fn render(ignore: &[String]) -> String {
    let mut lines: Vec<String> = vec![
        "# Managed by topmatic. This file isolates topmatic from your own topgrade".to_string(),
        "# configuration; manual edits will be overwritten.".to_string(),
        "assume_yes = true".to_string(),
        "cleanup = true".to_string(),
        "ask_retry = false".to_string(),
        "auto_retry = 1".to_string(),
        "notify_end = \"never\"".to_string(),
        "no_self_update = true".to_string(),
    ];
    if !ignore.is_empty() {
        let list = ignore
            .iter()
            .map(|step| format!("\"{step}\""))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("ignore = [{list}]"));
    }
    lines.join("\n") + "\n"
}

pub fn write_if_changed(path: &std::path::Path, ignore: &[String]) -> std::io::Result<()> {
    crate::util::write_file_if_changed(path, &render(ignore)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unattended_defaults() {
        let parsed: toml::Value = toml::from_str(&render(&[])).unwrap();
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
    fn exclusions_land_in_the_ignore_list() {
        let parsed: toml::Value =
            toml::from_str(&render(&["brew".to_string(), "wsl".to_string()])).unwrap();
        assert_eq!(
            parsed.get("ignore").and_then(|v| v.as_array()),
            Some(&vec![
                toml::Value::String("brew".to_string()),
                toml::Value::String("wsl".to_string())
            ])
        );
        assert!(
            !render(&[]).contains("ignore"),
            "no exclusions, no ignore key"
        );
    }

    #[test]
    fn write_if_changed_creates_and_preserves() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("topgrade.toml");
        write_if_changed(&path, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), render(&[]));
        let first_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        write_if_changed(&path, &[]).unwrap();
        let second_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(first_mtime, second_mtime);
    }

    #[test]
    fn never_uses_deprecated_or_legacy_keys() {
        let text = render(&[]);
        assert!(!text.contains("skip_notify"), "deprecated in topgrade");
        assert!(!text.contains("no_retry"), "legacy alias of ask_retry");
    }
}
