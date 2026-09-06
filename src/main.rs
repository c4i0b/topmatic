use clap::{Parser, Subcommand};
use topmatic::systemd::SystemdCtl;

#[derive(Parser)]
#[command(
    name = "topmatic",
    version,
    about = "User-level scheduled updates via topgrade"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Run a profile headlessly (used by systemd units)")]
    Run {
        profile: String,
        #[arg(long)]
        dry_run: bool,
    },
    #[command(about = "Converge systemd units to the config, repairing drift")]
    Sync,
    #[command(about = "Diagnose the setup and auto-repair what sync can fix")]
    Doctor,
    #[command(
        about = "Remove all topmatic units, state and schedules (config becomes .bak with --all)"
    )]
    Reset {
        #[arg(long)]
        all: bool,
    },
    #[command(about = "List profiles with timer status")]
    List,
    #[command(about = "Edit the config with $EDITOR, then sync")]
    Edit,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Run { profile, dry_run }) => cmd_run(&profile, dry_run),
        Some(Command::Sync) => cmd_sync(),
        Some(Command::Doctor) => cmd_doctor(),
        Some(Command::Reset { all }) => cmd_reset(all),
        Some(Command::List) => cmd_list(),
        Some(Command::Edit) => cmd_edit(),
        None => topmatic::tui::run(),
    }
}

fn cmd_doctor() -> anyhow::Result<()> {
    let paths = topmatic::paths::Paths::from_env();
    let path_env = std::env::var("PATH").unwrap_or_default();
    let mut failures = 0;

    match topmatic::runner::resolve::find_in_path("topgrade", &path_env) {
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

    match topmatic::runner::resolve::find_in_path("notify-send", &path_env) {
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

    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
    let ctl = topmatic::systemd::RealSystemdCtl::new(home);
    match ctl.linger_enabled() {
        Some(true) => println!("ok:   lingering enabled"),
        Some(false) => println!(
            "warn: lingering off, timers only fire with an open session (L in the TUI or loginctl enable-linger)"
        ),
        None => println!("warn: linger state unknown"),
    }

    let (config, issues) = topmatic::config::load_validated(&paths)?;
    for issue in &issues {
        failures += 1;
        println!("FAIL: invalid profile {issue}");
    }
    println!(
        "ok:   config at {} ({} valid profile(s))",
        paths.config_file().display(),
        config.profiles.len()
    );

    let topmatic_bin = std::env::current_exe()?;
    println!("running sync (auto-repairs unit drift)…");
    let report = topmatic::systemd::sync::sync(&config, &topmatic_bin, &ctl);
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
        let last = topmatic::runner::read_status(&paths, &profile.name)
            .ok()
            .flatten()
            .map(|outcome| {
                if outcome.skipped {
                    "skipped".to_string()
                } else if outcome.success {
                    outcome.finished_at.format("ok %d %b %H:%M").to_string()
                } else {
                    outcome.finished_at.format("FAILED %d %b %H:%M").to_string()
                }
            })
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

fn cmd_reset(all: bool) -> anyhow::Result<()> {
    let paths = topmatic::paths::Paths::from_env();
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
    let ctl = topmatic::systemd::RealSystemdCtl::new(home);

    let report = topmatic::systemd::sync::reset(&ctl, &paths, all);
    for orphan in &report.removed_orphans {
        println!("stopped and removed timer {orphan}");
    }
    if report.removed_units {
        println!("removed topmatic unit templates and schedule drop-ins");
    }
    if report.purged_state {
        println!(
            "purged run history and logs in {}",
            paths.state_dir.display()
        );
    }
    if let Some(backup) = &report.config_backup {
        println!("config moved to {}", backup.display());
    }
    Ok(())
}

fn cmd_list() -> anyhow::Result<()> {
    let paths = topmatic::paths::Paths::from_env();
    let (config, issues) = topmatic::config::load_validated(&paths)?;
    for issue in &issues {
        eprintln!("skipped invalid profile: {issue}");
    }
    if config.profiles.is_empty() {
        println!("no profiles yet; run topmatic to create one");
        return Ok(());
    }
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
    let ctl = topmatic::systemd::RealSystemdCtl::new(home);
    let header = format!(
        "{:<24} {:<7} {:<22} {:<20} {}",
        "PROFILE", "STATE", "SCHEDULE", "NEXT RUN", "LAST RUN"
    );
    println!("{header}");
    for profile in &config.profiles {
        let next = ctl
            .next_run(&profile.name)
            .map(|next| {
                next.with_timezone(&chrono::Local)
                    .format("%a %d %b %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| "-".to_string());
        let last = topmatic::runner::read_status(&paths, &profile.name)
            .ok()
            .flatten()
            .map(|outcome| {
                if outcome.skipped {
                    "skipped".to_string()
                } else if outcome.success {
                    outcome.finished_at.format("ok %d %b %H:%M").to_string()
                } else {
                    outcome.finished_at.format("FAILED %d %b %H:%M").to_string()
                }
            })
            .unwrap_or_else(|| "never".to_string());
        println!(
            "{:<24} {:<7} {:<22} {:<20} {}",
            profile.name,
            if ctl.timer_active(&profile.name) {
                "active"
            } else {
                "inactive"
            },
            profile.schedule.summary(),
            next,
            last
        );
    }
    Ok(())
}

fn cmd_edit() -> anyhow::Result<()> {
    let paths = topmatic::paths::Paths::from_env();
    if !paths.config_file().exists() {
        std::fs::create_dir_all(&paths.config_dir)?;
        std::fs::write(
            paths.config_file(),
            "# topmatic configuration.\n\
             # Add profiles with the TUI (or hand-edit below); topmatic reconciles systemd on next open.\n\
             #\n\
             # Example:\n\
             # [[profiles]]\n\
             # name = \"daily\"\n\
             # steps = [\"cargo\", \"flatpak\"]\n\
             # [profiles.schedule]\n\
             # preset = \"daily\"\n\
             # randomized_delay_sec = 1800\n",
        )?;
    }
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let mut split = editor.split_whitespace();
    let program = split.next().unwrap_or("vi");
    let mut command = std::process::Command::new(program);
    command.args(split).arg(paths.config_file());
    let status = command.status()?;
    if !status.success() {
        anyhow::bail!("editor {editor:?} exited with {status}");
    }
    cmd_sync()
}

fn cmd_sync() -> anyhow::Result<()> {
    let paths = topmatic::paths::Paths::from_env();
    let (config, issues) = topmatic::config::load_validated(&paths)?;
    let topmatic_bin = std::env::current_exe()?;
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
    let ctl = topmatic::systemd::RealSystemdCtl::new(home);

    let report = topmatic::systemd::sync::sync(&config, &topmatic_bin, &ctl);
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

fn cmd_run(name: &str, dry_run: bool) -> anyhow::Result<()> {
    let paths = topmatic::paths::Paths::from_env();
    let (config, issues) = topmatic::config::load_validated(&paths)?;
    let profile = match config.profile(name) {
        Some(profile) => profile.clone(),
        None => {
            if let Some(issue) = issues
                .iter()
                .find(|issue| issue.starts_with(&format!("{name}:")))
            {
                anyhow::bail!("profile {name:?} is invalid and was skipped: {issue}");
            }
            anyhow::bail!(
                "profile {name:?} not found in {}",
                paths.config_file().display()
            );
        }
    };

    let path_env = std::env::var("PATH").unwrap_or_default();
    let topgrade_bin = topmatic::runner::resolve::find_in_path("topgrade", &path_env)
        .ok_or_else(|| anyhow::anyhow!("topgrade not found in PATH"))?;

    let notify: Box<dyn topmatic::runner::notify::NotifyBackend> =
        match topmatic::runner::resolve::find_in_path("notify-send", &path_env) {
            Some(bin) => Box::new(topmatic::runner::notify::NotifySend::new(bin)),
            None => Box::new(topmatic::runner::notify::NullNotify),
        };

    let outcome = topmatic::runner::run(
        &profile,
        &topgrade_bin,
        &paths,
        notify.as_ref(),
        dry_run,
        false,
    )?;
    if outcome.skipped {
        eprintln!("another run of {name:?} is already in progress");
        return Ok(());
    }
    std::process::exit(i32::from(!outcome.success));
}
