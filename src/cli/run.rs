use crate::paths::Paths;

pub fn run(name: &str, dry_run: bool) -> anyhow::Result<()> {
    let paths = Paths::from_env();
    let (config, issues) = crate::config::load_validated(&paths)?;
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
    let topgrade_bin = crate::runner::resolve::find_in_path("topgrade", &path_env)
        .ok_or_else(|| anyhow::anyhow!("topgrade not found in PATH"))?;

    let notify: Box<dyn crate::runner::notify::NotifyBackend> =
        match crate::runner::resolve::find_in_path("notify-send", &path_env) {
            Some(bin) => Box::new(crate::runner::notify::NotifySend::new(bin)),
            None => Box::new(crate::runner::notify::NullNotify),
        };

    let outcome = crate::runner::run(&profile, &topgrade_bin, &paths, notify.as_ref(), dry_run)?;
    if outcome.skipped {
        eprintln!("another run of {name:?} is already in progress");
        return Ok(());
    }
    std::process::exit(i32::from(!outcome.success));
}
