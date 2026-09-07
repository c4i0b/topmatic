use crate::paths::Paths;
use crate::systemd::SystemdCtl;

pub fn run() -> anyhow::Result<()> {
    let paths = Paths::from_env();
    let (config, issues) = crate::config::load_validated(&paths)?;
    for issue in &issues {
        eprintln!("skipped invalid profile: {issue}");
    }
    if config.profiles.is_empty() {
        println!("no profiles yet; run topmatic to create one");
        return Ok(());
    }
    let ctl = super::user_ctl()?;
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
        let last = crate::runner::read_status(&paths, &profile.name)
            .ok()
            .flatten()
            .map(|outcome| outcome.last_run_summary())
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
