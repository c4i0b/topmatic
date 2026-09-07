pub const PRESET_IDS: &[&str] = &["all", "dev-tools", "flatpak"];

pub const DEV_TOOLS: &[&str] = &[
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

pub const ALL_STEPS: &[&str] = &[
    "am",
    "android_studio",
    "antigravity",
    "app_man",
    "aqua",
    "asdf",
    "atom",
    "atuin",
    "auto_cpufreq",
    "bin",
    "bob",
    "brew_cask",
    "brew_formula",
    "bun",
    "bun_packages",
    "cargo",
    "certbot",
    "chezmoi",
    "chocolatey",
    "choosenim",
    "cinnamon_spices",
    "clam_av_db",
    "claude_code",
    "claude_code_plugins",
    "codex",
    "colima",
    "composer",
    "conda",
    "cursor",
    "cursor_agent",
    "custom_commands",
    "deb_get",
    "deno",
    "dkp_pacman",
    "dotnet",
    "elan",
    "emacs",
    "falconf",
    "flatpak",
    "flutter",
    "fossil",
    "gcloud",
    "gearlever",
    "gem",
    "getnf",
    "ghcup",
    "git_repos",
    "github_cli_extensions",
    "gnome_shell_extensions",
    "go",
    "guix",
    "haxelib",
    "helix",
    "helix_db",
    "helm",
    "home_manager",
    "hyprpm",
    "install_release",
    "jetbrains_aqua",
    "jetbrains_clion",
    "jetbrains_datagrip",
    "jetbrains_dataspell",
    "jetbrains_gateway",
    "jetbrains_goland",
    "jetbrains_idea",
    "jetbrains_mps",
    "jetbrains_phpstorm",
    "jetbrains_pycharm",
    "jetbrains_rider",
    "jetbrains_rubymine",
    "jetbrains_rustrover",
    "jetbrains_toolbox",
    "jetbrains_webstorm",
    "jetpack",
    "julia",
    "juliaup",
    "kakoune",
    "krew",
    "lensfun",
    "lure",
    "macports",
    "mamba",
    "mas",
    "maza",
    "micro",
    "microsoft_office",
    "microsoft_store",
    "miktex",
    "mise",
    "myrepos",
    "nix",
    "nix_helper",
    "node",
    "ollama",
    "opam",
    "opencode",
    "pacdef",
    "pacstall",
    "pearl",
    "pi",
    "pip3",
    "pip_review",
    "pip_review_local",
    "pipupgrade",
    "pipx",
    "pipxu",
    "pixi",
    "pkg",
    "pkgfile",
    "pkgin",
    "pkgit",
    "platformio_core",
    "pnpm",
    "poetry",
    "powershell",
    "protonplus",
    "protonup",
    "pyenv",
    "raco",
    "rcm",
    "remotes",
    "rtcl",
    "ruby_gems",
    "rustup",
    "rye",
    "scoop",
    "sdkman",
    "sera",
    "sheldon",
    "shell",
    "skills",
    "soar",
    "sparkle",
    "spicetify",
    "stack",
    "stew",
    "tldr",
    "tlmgr",
    "tmux",
    "tpack",
    "typst",
    "uv",
    "vagrant",
    "vcpkg",
    "vim",
    "vite_plus",
    "volta_packages",
    "vscode",
    "vscode_insiders",
    "vscodium",
    "vscodium_insiders",
    "windsurf",
    "winget",
    "wsl",
    "wsl_update",
    "xcodes",
    "yadm",
    "yarn",
    "yazi",
    "zerobrew",
    "zigup",
    "zvm",
];

pub fn known(steps: &[&str], catalog: &[String]) -> Vec<String> {
    steps
        .iter()
        .filter(|step| catalog.iter().any(|known| known == *step))
        .map(|step| step.to_string())
        .collect()
}

pub fn steps_for(index: usize, catalog: &[String]) -> Vec<String> {
    match index {
        0 => known(ALL_STEPS, catalog),
        1 => known(DEV_TOOLS, catalog),
        2 => vec!["flatpak".to_string()],
        _ => Vec::new(),
    }
}

pub fn fallback_catalog() -> Vec<String> {
    ALL_STEPS.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::steps::is_privileged;

    const HELP: &str = include_str!("../../tests/fixtures/topgrade_help.txt");

    fn catalog() -> Vec<String> {
        crate::domain::steps::catalog(HELP)
    }

    #[test]
    fn flatpak_preset_selects_only_flatpak() {
        assert_eq!(steps_for(2, &catalog()), vec!["flatpak".to_string()]);
    }

    #[test]
    fn all_preset_is_an_explicit_list_without_privileged_steps() {
        assert!(ALL_STEPS.len() > 150);
        for step in ALL_STEPS {
            assert!(!is_privileged(step), "{step} must not be in the all preset");
        }
        let catalog = catalog();
        assert_eq!(steps_for(0, &catalog).len(), catalog.len());
    }

    #[test]
    fn dev_tools_preset_matches_known_steps_only() {
        let steps = steps_for(1, &catalog());
        assert!(steps.contains(&"cargo".to_string()));
        assert!(!steps.contains(&"flatpak".to_string()));
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

    #[test]
    fn preset_ids_are_stable_and_match_the_tui_order() {
        assert_eq!(PRESET_IDS, &["all", "dev-tools", "flatpak"]);
    }
}
