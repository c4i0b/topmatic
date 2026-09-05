use clap::{Parser, Subcommand};

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
        Some(Command::Sync) => {
            eprintln!("sync arrives in milestone M4");
            Ok(())
        }
        None => {
            eprintln!("TUI arrives in milestone M5");
            Ok(())
        }
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
