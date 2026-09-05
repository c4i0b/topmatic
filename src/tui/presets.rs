use crate::domain::steps::is_privileged;

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
        description: "runtimes, languages, editors and dev tooling",
        suggested_name: "dev-daily",
    },
    Preset {
        label: "Flatpak",
        description: "Flatpak apps and runtimes, with auto-clean",
        suggested_name: "flatpak-daily",
    },
];

const DEV_TOOLS: &[&str] = &[
    "cargo",
    "rustup",
    "node",
    "pnpm",
    "yarn",
    "bun",
    "deno",
    "go",
    "gem",
    "pipx",
    "pip3",
    "pyenv",
    "uv",
    "poetry",
    "mise",
    "asdf",
    "sdkman",
    "vim",
    "emacs",
    "helix",
    "vscode",
    "vscodium",
    "tmux",
    "atuin",
    "tldr",
    "ghcup",
    "aqua",
    "bob",
    "github_cli_extensions",
    "opencode",
];

pub fn steps_for(index: usize, catalog: &[String]) -> Vec<String> {
    match index {
        0 => catalog
            .iter()
            .filter(|step| !is_privileged(step))
            .cloned()
            .collect(),
        1 => DEV_TOOLS
            .iter()
            .filter(|step| catalog.iter().any(|known| known == *step))
            .map(|step| step.to_string())
            .collect(),
        2 => vec!["flatpak".to_string()],
        _ => Vec::new(),
    }
}

pub fn fallback_catalog() -> Vec<String> {
    let mut steps: Vec<String> = DEV_TOOLS.iter().map(|s| s.to_string()).collect();
    steps.push("flatpak".to_string());
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELP: &str = include_str!("../../tests/fixtures/topgrade_help.txt");

    fn catalog() -> Vec<String> {
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
        assert!(!steps.contains(&"system".to_string()));
        assert!(steps.contains(&"flatpak".to_string()));
        assert_eq!(
            steps.len(),
            entries.iter().filter(|s| !is_privileged(s)).count()
        );
    }

    #[test]
    fn dev_tools_preset_matches_known_steps_only() {
        let steps = steps_for(1, &catalog());
        assert!(steps.contains(&"cargo".to_string()));
        assert!(!steps.contains(&"flatpak".to_string()));
        assert!(!steps.contains(&"winget".to_string()));
    }

    #[test]
    fn out_of_range_preset_is_empty() {
        assert!(steps_for(99, &catalog()).is_empty());
    }

    #[test]
    fn fallback_catalog_has_no_duplicates() {
        let steps = fallback_catalog();
        let mut unique = steps.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), steps.len());
    }
}
