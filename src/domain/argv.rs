use std::ffi::OsString;
use std::path::Path;

use super::overlay::ResolvedSteps;

pub fn topgrade_argv(
    steps: &ResolvedSteps,
    topgrade_config: &Path,
    dry_run: bool,
) -> Vec<OsString> {
    let mut argv: Vec<OsString> = vec![
        "topgrade".into(),
        "--config".into(),
        topgrade_config.as_os_str().to_os_string(),
    ];
    if dry_run {
        argv.push("--dry-run".into());
    } else {
        argv.push("--run-type".into());
        argv.push("damp".into());
    }
    argv.push("--log-filter".into());
    argv.push("info".into());
    if let ResolvedSteps::Explicit(steps) = steps
        && !steps.is_empty()
    {
        argv.push("--only".into());
        argv.extend(steps.iter().map(|s| s.as_str().into()));
    }
    argv
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::overlay::ResolvedSteps;

    fn explicit(steps: &[&str]) -> ResolvedSteps {
        ResolvedSteps::Explicit(steps.iter().map(|s| s.to_string()).collect())
    }

    fn strings(argv: &[OsString]) -> Vec<String> {
        argv.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn composes_config_dry_run_and_steps() {
        let argv = topgrade_argv(
            &explicit(&["flatpak", "cargo"]),
            Path::new("/cfg/topgrade.toml"),
            true,
        );
        assert_eq!(
            strings(&argv),
            vec![
                "topgrade",
                "--config",
                "/cfg/topgrade.toml",
                "--dry-run",
                "--log-filter",
                "info",
                "--only",
                "flatpak",
                "cargo",
            ]
        );
    }

    #[test]
    fn dry_run_never_pairs_with_the_damp_run_type() {
        let argv = topgrade_argv(
            &explicit(&["flatpak"]),
            Path::new("/cfg/topgrade.toml"),
            true,
        );
        let rendered = strings(&argv);
        assert!(!rendered.contains(&"--run-type".to_string()));
        assert!(!rendered.contains(&"damp".to_string()));
    }

    #[test]
    fn real_runs_print_each_command_as_it_executes() {
        let argv = topgrade_argv(
            &explicit(&["flatpak"]),
            Path::new("/cfg/topgrade.toml"),
            false,
        );
        let rendered = strings(&argv);
        assert!(!rendered.contains(&"--dry-run".to_string()));
        let damp = rendered.iter().position(|arg| arg == "--run-type").unwrap();
        assert_eq!(rendered[damp + 1], "damp");
        assert!(rendered.contains(&"--log-filter".to_string()));
        assert_eq!(
            rendered[rendered
                .iter()
                .position(|arg| arg == "--log-filter")
                .unwrap()
                + 1],
            "info"
        );
    }

    #[test]
    fn policy_flags_live_in_the_generated_config_not_argv() {
        let argv = topgrade_argv(
            &explicit(&["flatpak"]),
            Path::new("/cfg/topgrade.toml"),
            false,
        );
        let rendered = strings(&argv);
        for policy_flag in ["--yes", "--cleanup", "--no-ask-retry", "--auto-retry"] {
            assert!(
                !rendered.contains(&policy_flag.to_string()),
                "{policy_flag} belongs to the generated topgrade.toml"
            );
        }
    }

    #[test]
    fn adds_dry_run_when_requested() {
        let argv = topgrade_argv(
            &explicit(&["flatpak"]),
            Path::new("/cfg/topgrade.toml"),
            true,
        );
        assert!(strings(&argv).contains(&"--dry-run".to_string()));
    }

    #[test]
    fn skips_only_flag_without_steps() {
        let argv = topgrade_argv(&explicit(&[]), Path::new("/cfg/topgrade.toml"), false);
        assert!(!strings(&argv).contains(&"--only".to_string()));
    }
}
