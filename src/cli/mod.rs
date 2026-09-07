mod doctor;
mod edit;
mod guard;
mod list;
mod reset;
mod run;
mod sync;

#[cfg(test)]
mod guard_tests;

use crate::systemd::RealSystemdCtl;
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
    #[command(about = "Run a profile headlessly (used by systemd units)")]
    Run {
        profile: String,
        #[arg(long)]
        dry_run: bool,
    },
    #[command(about = "Converge systemd units to the config, repairing drift")]
    Sync,
    #[command(about = "Diagnose the setup and auto-repair what sync can fix")]
    Doctor {
        #[arg(long, help = "Quarantine a broken config and write a fresh one")]
        repair: bool,
    },
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

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    guard::block_root()?;
    match cli.command {
        Some(Command::Run { profile, dry_run }) => run::run(&profile, dry_run),
        Some(Command::Sync) => sync::run(),
        Some(Command::Doctor { repair }) => doctor::run(repair),
        Some(Command::Reset { all }) => reset::run(all),
        Some(Command::List) => list::run(),
        Some(Command::Edit) => edit::run(),
        None => crate::tui::run(),
    }
}

pub(crate) fn user_ctl() -> anyhow::Result<RealSystemdCtl> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
    Ok(RealSystemdCtl::new(home))
}
