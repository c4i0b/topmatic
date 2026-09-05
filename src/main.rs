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
    /// Run a profile headlessly (used by systemd units)
    Run {
        profile: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Converge systemd units to the config
    Sync,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Run { profile, dry_run }) => cmd_run(&profile, dry_run),
        Some(Command::Sync) => cmd_sync(),
        None => {
            eprintln!("TUI arrives in milestone M5");
            Ok(())
        }
    }
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
