use crate::paths::Paths;
use crate::systemd::SystemdCtl;

pub fn run() -> anyhow::Result<()> {
    let paths = Paths::from_env();
    let path_env = std::env::var("PATH").unwrap_or_default();
    let mut failures = 0;

    match crate::runner::resolve::find_in_path("topgrade", &path_env) {
        Some(bin) => {
            let version = std::process::Command::new(&bin)
                .arg("--version")
                .output()
                .ok()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                .unwrap_or_default();
            println!("ok:   topgrade {version}");
        }
        None => {
            failures += 1;
            println!("FAIL: topgrade not found in PATH");
            println!("      install it with: cargo install topgrade");
            println!("      or via your distro package manager / AUR / brew");
        }
    }

    match crate::runner::resolve::find_in_path("notify-send", &path_env) {
        Some(_) => println!("ok:   notify-send available"),
        None => println!("warn: notify-send missing, notifications are silently skipped"),
    }

    let systemd_ok = std::process::Command::new("systemctl")
        .args(["--user", "is-system-running"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if systemd_ok {
        println!("ok:   systemd user manager reachable");
    } else {
        failures += 1;
        println!("FAIL: systemd user manager not reachable");
    }

    let ctl = super::user_ctl()?;
    match ctl.linger_enabled() {
        Some(true) => println!("ok:   lingering enabled"),
        Some(false) => println!(
            "warn: lingering off, timers only fire with an open session (L in the TUI or loginctl enable-linger)"
        ),
        None => println!("warn: linger state unknown"),
    }

    let (config, issues) = crate::config::load_validated(&paths)?;
    let _ = crate::config::write_example_if_changed(&paths);
    for issue in &issues {
        failures += 1;
        println!("FAIL: {issue}");
    }
    let resolved = config.defaults.resolved();
    println!(
        "ok:   run policy: {} retries, {} base delay, {} budget, {} network wait, {} default jitter",
        resolved.retries,
        humantime::format_duration(resolved.retry_base_delay),
        humantime::format_duration(resolved.retry_budget),
        humantime::format_duration(resolved.network_wait),
        humantime::format_duration(resolved.default_jitter),
    );
    println!(
        "ok:   config at {} ({} valid profile(s))",
        paths.config_file().display(),
        config.profiles.len()
    );

    let topmatic_bin = std::env::current_exe()?;
    println!("running sync (auto-repairs unit drift)…");
    let report = crate::systemd::sync::sync(&config, &topmatic_bin, &ctl);
    for profile in &report.updated_profiles {
        println!("ok:   repaired schedule drop-in for {profile}");
    }
    for pruned in &report.pruned_drop_ins {
        println!("ok:   pruned stray files from {pruned} drop-in");
    }
    for orphan in &report.removed_orphans {
        println!("ok:   removed orphan timer {orphan}");
    }
    for foreign in &report.ignored_foreign {
        println!("info: left foreign timer {foreign} untouched");
    }
    for error in &report.errors {
        failures += 1;
        println!("FAIL: {error}");
    }
    if report.is_clean() {
        println!("ok:   systemd units in sync with config");
    }

    for profile in &config.profiles {
        let last = crate::runner::read_status(&paths, &profile.name)
            .ok()
            .flatten()
            .map(|outcome| outcome.last_run_summary())
            .unwrap_or_else(|| "never".to_string());
        println!("info: {:<24} last run: {last}", profile.name);
    }

    if failures > 0 {
        println!("{failures} problem(s) found");
        std::process::exit(1);
    }
    println!("all checks passed");
    Ok(())
}
