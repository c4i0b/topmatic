use crate::paths::Paths;

pub fn run(all: bool) -> anyhow::Result<()> {
    let paths = Paths::from_env();
    let ctl = super::user_ctl()?;

    let report = crate::systemd::reset::reset(&ctl, &paths, all);
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
