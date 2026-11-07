pub const PRIVILEGED_STEPS: &[&str] = &[
    "system",
    "firmware",
    "snap",
    "audit",
    "config_update",
    "mandb",
    "waydroid",
    "self_update",
    "restarts",
];

pub fn parse_steps(help_text: &str) -> Vec<String> {
    let mut steps: Vec<String> = Vec::new();
    let mut in_list = false;
    for line in help_text.lines() {
        let segment = match (in_list, line.find("[possible values: ")) {
            (false, Some(pos)) => {
                in_list = true;
                &line[pos + "[possible values: ".len()..]
            }
            (true, _) => line,
            (false, None) => continue,
        };
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let (values, still_open) = match segment.rfind(']') {
            Some(pos) if pos + 1 == segment.len() => (&segment[..pos], false),
            _ => (segment, true),
        };
        for value in values.split(", ") {
            let value = value.trim().trim_end_matches(',').trim();
            if !value.is_empty() && !steps.iter().any(|s| s == value) {
                steps.push(value.to_string());
            }
        }
        if !still_open {
            in_list = false;
        }
    }
    steps
}

pub fn catalog(help_text: &str) -> Vec<String> {
    parse_steps(help_text)
        .into_iter()
        .filter(|step| !is_privileged(step))
        .collect()
}

pub fn is_privileged(step: &str) -> bool {
    PRIVILEGED_STEPS.contains(&step)
}

pub fn format_catalog_listing(
    catalog: &[String],
    covered: &std::collections::BTreeSet<String>,
) -> String {
    const COLUMNS: usize = 4;
    const WIDTH: usize = 22;
    let mut lines = Vec::new();
    for chunk in catalog.chunks(COLUMNS) {
        let cells: Vec<String> = chunk
            .iter()
            .map(|step| {
                let marker = if covered.contains(step) { "*" } else { "" };
                format!("{step}{marker}")
            })
            .collect();
        let mut line = String::new();
        for (index, cell) in cells.iter().enumerate() {
            if index > 0 {
                line.push(' ');
            }
            if index + 1 == cells.len() {
                line.push_str(cell);
            } else {
                line.push_str(&format!("{cell:<WIDTH$}"));
            }
        }
        lines.push(line.trim_end().to_string());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELP: &str = include_str!("../../tests/fixtures/topgrade_help.txt");

    #[test]
    fn parses_full_list_from_real_help_output() {
        let steps = parse_steps(HELP);
        assert!(steps.len() > 150, "got {} steps", steps.len());
        assert!(steps.contains(&"flatpak".to_string()));
        assert!(steps.contains(&"cargo".to_string()));
        assert!(steps.contains(&"custom_commands".to_string()));
        let mut sorted = steps.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), steps.len(), "duplicates found");
    }

    #[test]
    fn parses_wrapped_and_single_line_lists() {
        let help = "\
      --only <STEP>...\n          Perform only the specified steps\n          \n          [possible values: am, android_studio, antigravity,\n          app_man, aqua, asdf]\n      --disable <STEP>...\n          [possible values: am, asdf]\n";
        let steps = parse_steps(help);
        assert_eq!(
            steps,
            vec![
                "am",
                "android_studio",
                "antigravity",
                "app_man",
                "aqua",
                "asdf"
            ]
        );
    }

    #[test]
    fn ignores_text_without_possible_values() {
        assert!(parse_steps("no lists here\nat all").is_empty());
    }

    #[test]
    fn flags_privileged_steps() {
        assert!(is_privileged("system"));
        assert!(is_privileged("firmware"));
        assert!(
            is_privileged("snap"),
            "sudo snap refresh in topgrade source"
        );
        assert!(
            is_privileged("restarts"),
            "sudo needrestart in topgrade source"
        );
        assert!(!is_privileged("flatpak"));
        assert!(!is_privileged("cargo"));
    }

    #[test]
    fn user_level_container_steps_are_not_privileged() {
        assert!(
            !is_privileged("containers"),
            "docker/podman pulls never sudo unless use_sudo is set"
        );
        assert!(
            !is_privileged("distrobox"),
            "distrobox upgrade runs rootless"
        );
        assert!(
            !is_privileged("toolbx"),
            "toolbx updates run inside user containers"
        );
    }

    #[test]
    fn catalog_hides_privileged_steps_from_selection() {
        let steps = catalog(HELP);
        assert!(!steps.iter().any(|s| is_privileged(s)));
        assert!(!steps.contains(&"system".to_string()));
        assert!(!steps.contains(&"firmware".to_string()));
        assert!(steps.contains(&"flatpak".to_string()));
        assert!(steps.contains(&"distrobox".to_string()));
        assert!(steps.contains(&"containers".to_string()));
        assert!(steps.contains(&"toolbx".to_string()));
    }

    #[test]
    fn listing_marks_covered_steps_and_wraps() {
        let catalog: Vec<String> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let covered: std::collections::BTreeSet<String> =
            ["b".to_string(), "e".to_string()].into_iter().collect();
        let text = format_catalog_listing(&catalog, &covered);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines.len(),
            2,
            "five steps in four columns wrap to two rows"
        );
        assert!(
            lines[0].starts_with("a  "),
            "uncovered stays plain: {:?}",
            lines[0]
        );
        assert!(
            lines[0].contains("b*"),
            "covered carries a star: {:?}",
            lines[0]
        );
        assert_eq!(lines[1], "e*");
    }

    #[test]
    fn listing_of_empty_catalog_is_empty() {
        let covered = std::collections::BTreeSet::new();
        assert_eq!(format_catalog_listing(&[], &covered), "");
    }
}
