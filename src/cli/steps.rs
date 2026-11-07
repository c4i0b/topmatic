use std::collections::BTreeSet;

use crate::domain::overlay::ResolvedSteps;
use crate::paths::Paths;

pub fn run() -> anyhow::Result<()> {
    let paths = Paths::from_env();
    let catalog = catalog_from_installed_topgrade();
    let (config, issues) = crate::config::load_validated(&paths)?;
    for issue in &issues {
        eprintln!("skipped invalid profile: {issue}");
    }

    let covered: BTreeSet<String> = config
        .profiles
        .iter()
        .flat_map(
            |profile| match crate::domain::overlay::resolved_steps(profile, &catalog) {
                ResolvedSteps::Everything { .. } => catalog.clone(),
                ResolvedSteps::Explicit(steps) => steps,
            },
        )
        .collect();

    println!(
        "{} user-level steps this machine's topgrade can update",
        catalog.len()
    );
    println!("* = covered by at least one of your profiles");
    println!();
    println!(
        "{}",
        crate::domain::steps::format_catalog_listing(&catalog, &covered)
    );
    Ok(())
}

fn catalog_from_installed_topgrade() -> Vec<String> {
    let path_value = std::env::var("PATH").unwrap_or_default();
    crate::runner::resolve::find_in_path("topgrade", &path_value)
        .as_deref()
        .and_then(crate::runner::resolve::live_catalog)
        .unwrap_or_else(crate::domain::presets::fallback_catalog)
}
