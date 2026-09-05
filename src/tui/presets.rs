use crate::domain::steps::{CATEGORY_RUNTIMES, CATEGORY_TOOLS, StepEntry, is_privileged};

pub struct Preset {
    pub label: &'static str,
    pub description: &'static str,
    pub suggested_name: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        label: "Everything user-level",
        description: "all non-privileged steps from your installed topgrade",
        suggested_name: "all-user-daily",
    },
    Preset {
        label: "Dev tools",
        description: "runtimes & languages + editors & tools (cargo, node, uv, vim, vscode…)",
        suggested_name: "dev-daily",
    },
    Preset {
        label: "Flatpak",
        description: "Flatpak apps and runtimes, with auto-clean",
        suggested_name: "flatpak-daily",
    },
];

pub fn steps_for(index: usize, catalog: &[StepEntry]) -> Vec<String> {
    match index {
        i if i < PRESETS.len() => match PRESETS[i].suggested_name {
            "all-user-daily" => catalog
                .iter()
                .filter(|entry| !is_privileged(&entry.id))
                .map(|entry| entry.id.clone())
                .collect(),
            "dev-daily" => catalog
                .iter()
                .filter(|entry| {
                    entry.category == Some(CATEGORY_RUNTIMES)
                        || entry.category == Some(CATEGORY_TOOLS)
                })
                .map(|entry| entry.id.clone())
                .collect(),
            _ => vec!["flatpak".to_string()],
        },
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELP: &str = include_str!("../../tests/fixtures/topgrade_help.txt");

    fn catalog() -> Vec<StepEntry> {
        crate::domain::steps::catalog(HELP)
    }

    #[test]
    fn flatpak_preset_selects_only_flatpak() {
        assert_eq!(steps_for(2, &catalog()), vec!["flatpak".to_string()]);
    }

    #[test]
    fn all_user_preset_covers_everything_non_privileged() {
        let entries = catalog();
        let steps = steps_for(0, &entries);
        let expected: Vec<String> = entries
            .iter()
            .filter(|entry| !is_privileged(&entry.id))
            .map(|entry| entry.id.clone())
            .collect();
        assert_eq!(steps, expected);
        assert!(!steps.contains(&"system".to_string()));
        assert!(steps.contains(&"flatpak".to_string()));
        assert!(steps.len() > 100);
    }

    #[test]
    fn dev_tools_preset_matches_curated_families() {
        let steps = steps_for(1, &catalog());
        assert!(steps.contains(&"cargo".to_string()));
        assert!(steps.contains(&"vim".to_string()));
        assert!(!steps.contains(&"flatpak".to_string()));
        assert!(!steps.contains(&"winget".to_string()));
    }

    #[test]
    fn out_of_range_preset_is_empty() {
        assert!(steps_for(99, &catalog()).is_empty());
    }
}
