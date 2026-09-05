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
    #[command(about = "Converge systemd units to the config")]
    Sync,
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
        Some(Command::List) => cmd_list(),
        Some(Command::Edit) => cmd_edit(),
        None => topmatic::tui::run(),
    }
}

fn cmd_list() -> anyhow::Result<()> {
    let paths = topmatic::paths::Paths::from_env();
    let config = topmatic::config::load(&paths)?;
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
            if profile.enabled { "active" } else { "paused" },
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
            "# topmatic configuration\n\n[[profiles]]\n",
        )?;
    }
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor)
        .arg(paths.config_file())
        .status()?;
    if !status.success() {
        anyhow::bail!("editor {editor:?} exited with {status}");
    }
    cmd_sync()
}

fn cmd_sync() -> anyhow::Result<()> {
    let paths = topmatic::paths::Paths::from_env();
    let config = topmatic::config::load(&paths)?;
    let topmatic_bin = std::env::current_exe()?;
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
    let ctl = topmatic::systemd::RealSystemdCtl::new(home);

    let report = topmatic::systemd::sync::sync(&config, &topmatic_bin, &ctl);
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
    let config = topmatic::config::load(&paths)?;
    let profile = config
        .profile(name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "profile {name:?} not found in {}",
                paths.config_file().display()
            )
        })?
        .clone();

    let path_env = std::env::var("PATH").unwrap_or_default();
    let topgrade_bin = topmatic::runner::resolve::find_in_path("topgrade", &path_env)
        .ok_or_else(|| anyhow::anyhow!("topgrade not found in PATH"))?;

    let notify: Box<dyn topmatic::runner::notify::NotifyBackend> =
        match topmatic::runner::resolve::find_in_path("notify-send", &path_env) {
            Some(bin) => Box::new(topmatic::runner::notify::NotifySend::new(bin)),
            None => Box::new(topmatic::runner::notify::NullNotify),
        };

    let outcome = topmatic::runner::run(&profile, &topgrade_bin, &paths, notify.as_ref(), dry_run)?;
    if outcome.skipped {
        eprintln!("another run of {name:?} is already in progress");
    }
    std::process::exit(i32::from(!outcome.success));
}
