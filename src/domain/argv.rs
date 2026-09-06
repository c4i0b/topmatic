use std::ffi::OsString;
use std::path::Path;

use super::profile::Profile;

pub fn topgrade_argv(profile: &Profile, topgrade_config: &Path, dry_run: bool) -> Vec<OsString> {
    let mut argv: Vec<OsString> = vec![
        "topgrade".into(),
        "--config".into(),
        topgrade_config.as_os_str().to_os_string(),
        "--no-ask-retry".into(),
    ];
    argv.push("--cleanup".into());
    if dry_run {
        argv.push("--dry-run".into());
    }
    if !profile.steps.is_empty() {
        argv.push("--only".into());
        argv.extend(profile.steps.iter().map(|s| s.as_str().into()));
    }
    argv.push("--yes".into());
    argv
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::NotifyPolicy;
    use crate::domain::schedule::Schedule;

    fn profile(steps: &[&str]) -> Profile {
        Profile {
            name: "flatpak-daily".to_string(),
            steps: steps.iter().map(|s| s.to_string()).collect(),
            schedule: Schedule::default(),
            notify: NotifyPolicy::default(),
            scope: Default::default(),
        }
    }

    fn strings(argv: &[OsString]) -> Vec<String> {
        argv.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn composes_expected_order_with_cleanup_and_steps() {
        let argv = topgrade_argv(
            &profile(&["flatpak", "cargo"]),
            Path::new("/cfg/topgrade.toml"),
            false,
        );
        assert_eq!(
            strings(&argv),
            vec![
                "topgrade",
                "--config",
                "/cfg/topgrade.toml",
                "--no-ask-retry",
                "--cleanup",
                "--only",
                "flatpak",
                "cargo",
                "--yes",
            ]
        );
    }

    #[test]
    fn cleanup_is_always_passed() {
        let argv = topgrade_argv(
            &profile(&["flatpak"]),
            Path::new("/cfg/topgrade.toml"),
            false,
        );
        assert!(strings(&argv).contains(&"--cleanup".to_string()));
        assert!(!strings(&argv).contains(&"--dry-run".to_string()));
    }

    #[test]
    fn adds_dry_run_when_requested() {
        let argv = topgrade_argv(
            &profile(&["flatpak"]),
            Path::new("/cfg/topgrade.toml"),
            true,
        );
        assert!(strings(&argv).contains(&"--dry-run".to_string()));
    }

    #[test]
    fn skips_only_flag_without_steps() {
        let argv = topgrade_argv(&profile(&[]), Path::new("/cfg/topgrade.toml"), false);
        assert!(!strings(&argv).contains(&"--only".to_string()));
    }
}
