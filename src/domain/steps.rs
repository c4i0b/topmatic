pub const PRIVILEGED_STEPS: &[&str] = &[
    "system",
    "firmware",
    "snap",
    "audit",
    "config_update",
    "mandb",
    "containers",
    "waydroid",
    "toolbx",
    "distrobox",
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
        assert!(!is_privileged("flatpak"));
        assert!(!is_privileged("cargo"));
    }

    #[test]
    fn catalog_hides_privileged_steps_from_selection() {
        let steps = catalog(HELP);
        assert!(!steps.iter().any(|s| is_privileged(s)));
        assert!(!steps.contains(&"system".to_string()));
        assert!(!steps.contains(&"firmware".to_string()));
        assert!(steps.contains(&"flatpak".to_string()));
    }
}
