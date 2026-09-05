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

pub const CATEGORY_FLATPAK: &str = "Flatpak";
pub const CATEGORY_RUNTIMES: &str = "Runtimes & languages";
pub const CATEGORY_TOOLS: &str = "Editors & tools";
pub const CATEGORY_REPOS: &str = "Dotfiles & repos";

const RUNTIME_EXACT: &[&str] = &[
    "cargo",
    "rustup",
    "node",
    "pnpm",
    "yarn",
    "deno",
    "go",
    "gem",
    "ruby_gems",
    "pyenv",
    "uv",
    "pipx",
    "pipxu",
    "poetry",
    "conda",
    "mamba",
    "pixi",
    "julia",
    "juliaup",
    "stack",
    "opam",
    "elan",
    "sdkman",
    "mise",
    "asdf",
    "zigup",
    "zvm",
    "vcpkg",
    "tlmgr",
    "raco",
    "haxelib",
    "volta_packages",
];

const RUNTIME_STEMS: &[&str] = &["pip", "julia", "bun", "rust", "ruby", "zig"];

const TOOLS_EXACT: &[&str] = &[
    "vim",
    "emacs",
    "micro",
    "kakoune",
    "atom",
    "tmux",
    "atuin",
    "tldr",
    "ghcup",
    "aqua",
    "bob",
    "gcloud",
    "ollama",
    "opencode",
    "claude_code",
    "codex",
    "spicetify",
    "getnf",
    "bin",
    "shell",
    "helm",
    "krew",
    "github_cli_extensions",
    "claude_code_plugins",
];

const TOOLS_STEMS: &[&str] = &[
    "vscode",
    "vscodium",
    "jetbrains",
    "helix",
    "cursor",
    "windsurf",
];

const REPOS_EXACT: &[&str] = &[
    "chezmoi",
    "yadm",
    "rcm",
    "fossil",
    "stew",
    "custom_commands",
    "git_repos",
    "myrepos",
];

const REPOS_STEMS: &[&str] = &["repo"];

pub const FALLBACK_STEPS: &[&str] = &[
    "flatpak",
    "cargo",
    "rustup",
    "node",
    "pnpm",
    "yarn",
    "bun",
    "deno",
    "go",
    "uv",
    "pipx",
    "pyenv",
    "poetry",
    "mise",
    "vim",
    "emacs",
    "helix",
    "vscode",
    "vscodium",
    "tmux",
    "atuin",
    "tldr",
    "opencode",
    "jetbrains_toolbox",
    "chezmoi",
    "yadm",
    "git_repos",
    "custom_commands",
];

fn matches_token(step: &str, token: &str) -> bool {
    if step == token {
        return true;
    }
    let Some(rest) = step.strip_prefix(token) else {
        return false;
    };
    rest.starts_with('_') || rest.chars().all(|c| c.is_ascii_digit())
}

fn matches_any(step: &str, exact: &[&str], stems: &[&str], stem_contains: bool) -> bool {
    if exact.contains(&step) {
        return true;
    }
    if stem_contains {
        return stems.iter().any(|token| step.contains(token));
    }
    stems.iter().any(|token| matches_token(step, token))
}

pub fn curated_category(step: &str) -> Option<&'static str> {
    if step == "flatpak" {
        return Some(CATEGORY_FLATPAK);
    }
    if matches_any(step, REPOS_EXACT, REPOS_STEMS, true) {
        return Some(CATEGORY_REPOS);
    }
    if matches_any(step, RUNTIME_EXACT, RUNTIME_STEMS, false) {
        return Some(CATEGORY_RUNTIMES);
    }
    if matches_any(step, TOOLS_EXACT, TOOLS_STEMS, false) {
        return Some(CATEGORY_TOOLS);
    }
    None
}

#[derive(Debug, Clone)]
pub struct StepEntry {
    pub id: String,
    pub category: Option<&'static str>,
}

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

pub fn catalog(help_text: &str) -> Vec<StepEntry> {
    parse_steps(help_text)
        .into_iter()
        .map(|id| StepEntry {
            category: curated_category(&id),
            id,
        })
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
    fn catalog_marks_curated_categories() {
        let entries = catalog(HELP);
        let flatpak = entries.iter().find(|e| e.id == "flatpak").unwrap();
        assert_eq!(flatpak.category, Some("Flatpak"));
        let cargo = entries.iter().find(|e| e.id == "cargo").unwrap();
        assert_eq!(cargo.category, Some("Runtimes & languages"));
        let unknown = entries.iter().find(|e| e.id == "winget").unwrap();
        assert_eq!(unknown.category, None);
    }

    #[test]
    fn classification_is_pattern_based_for_future_steps() {
        let cases = [
            ("flatpak", Some("Flatpak")),
            ("pipxu", Some("Runtimes & languages")),
            ("pip3", Some("Runtimes & languages")),
            ("pip_review_local", Some("Runtimes & languages")),
            ("juliaup", Some("Runtimes & languages")),
            ("ruby_gems", Some("Runtimes & languages")),
            ("pip_something_new", Some("Runtimes & languages")),
            ("rust_future", Some("Runtimes & languages")),
            ("jetbrains_idea", Some("Editors & tools")),
            ("jetbrains_next_ide", Some("Editors & tools")),
            ("vscode_insiders", Some("Editors & tools")),
            ("helix_db", Some("Editors & tools")),
            ("cursor_agent", Some("Editors & tools")),
            ("ghcup", Some("Editors & tools")),
            ("gcloud", Some("Editors & tools")),
            ("atuin", Some("Editors & tools")),
            ("git_repos", Some("Dotfiles & repos")),
            ("myrepos", Some("Dotfiles & repos")),
            ("chezmoi", Some("Dotfiles & repos")),
            ("something_repos_new", Some("Dotfiles & repos")),
            ("winget", None),
            ("gearlever", None),
        ];
        for (step, expected) in cases {
            assert_eq!(curated_category(step), expected, "step {step}");
        }
    }

    #[test]
    fn every_current_user_level_step_falls_into_a_family_or_none() {
        let entries = catalog(HELP);
        let bucketed = entries.iter().filter(|e| e.category.is_some()).count();
        assert!(bucketed > 80, "expected broad coverage, got {bucketed}");
        let privileged = entries.iter().filter(|e| is_privileged(&e.id)).count();
        assert!(privileged > 0);
        for entry in entries.iter().filter(|e| is_privileged(&e.id)) {
            assert_eq!(entry.category, None, "{} must not be curated", entry.id);
        }
    }

    #[test]
    fn fallback_steps_all_classify() {
        for step in FALLBACK_STEPS {
            assert!(
                curated_category(step).is_some(),
                "fallback step {step} lost its category"
            );
        }
    }

    #[test]
    fn flags_privileged_steps() {
        assert!(is_privileged("system"));
        assert!(is_privileged("firmware"));
        assert!(!is_privileged("flatpak"));
        assert!(!is_privileged("cargo"));
    }
}
