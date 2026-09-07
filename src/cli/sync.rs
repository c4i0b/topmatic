use crate::paths::Paths;
use crate::systemd::SystemdCtl;

pub fn run() -> anyhow::Result<()> {
    let paths = Paths::from_env();
    let (config, issues) = crate::config::load_validated(&paths)?;
    let _ = crate::config::write_example_if_changed(&paths);
    let topmatic_bin = std::env::current_exe()?;
    let ctl = super::user_ctl()?;

    let report = crate::systemd::sync::sync(&config, &topmatic_bin, &ctl);
    for issue in &issues {
        eprintln!(
            "skipped invalid profile: {issue} (edit {} to fix)",
            paths.config_file().display()
        );
    }
    for pruned in &report.pruned_drop_ins {
        println!("pruned stray files from {pruned} schedule drop-in");
    }
    if report.templates_installed {
        println!(
            "installed systemd unit templates in {}",
            ctl.unit_dir().display()
        );
    }
    for profile in &report.updated_profiles {
        println!("updated schedule for {profile}");
    }
    for orphan in &report.removed_orphans {
        println!("removed orphan timer {orphan}");
    }
    for foreign in &report.ignored_foreign {
        println!("left foreign timer {foreign} untouched");
    }
    for error in &report.errors {
        eprintln!("error: {error}");
    }
    if report.reloaded {
        println!("systemd user manager reloaded");
    }
    if report.is_clean() && report.errors.is_empty() {
        println!("already in sync");
    }
    if let Some(false) = ctl.linger_enabled() {
        println!(
            "hint: enable lingering so timers fire without an open session: loginctl enable-linger"
        );
    }
    if report.errors.is_empty() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
