use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::domain::profile::{Profile, sanitize_name};
use crate::domain::schedule::SchedulePreset;
use crate::paths::Paths;
use crate::runner::retry::RetryPolicy;

const HEADER: &str =
    "# topmatic configuration. Edit freely; topmatic reconciles systemd on next open.\n\n";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default, skip_serializing_if = "Defaults::is_empty")]
    pub defaults: Defaults,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TomlDuration(pub Duration);

impl Serialize for TomlDuration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        humantime::format_duration(self.0)
            .to_string()
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TomlDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        humantime::parse_duration(&text)
            .map(TomlDuration)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_delay: Option<TomlDuration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub give_up_after: Option<TomlDuration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_wait: Option<TomlDuration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub random_delay: Option<TomlDuration>,
}

impl Defaults {
    pub fn is_empty(&self) -> bool {
        *self == Defaults::default()
    }

    pub fn resolved(&self) -> ResolvedDefaults {
        let hardcoded = ResolvedDefaults::hardcoded();
        if !validate_defaults(self).is_empty() {
            return hardcoded;
        }
        ResolvedDefaults {
            retries: self.retries.unwrap_or(hardcoded.retries),
            retry_delay: self
                .retry_delay
                .map(|d| d.0)
                .unwrap_or(hardcoded.retry_delay),
            give_up_after: self
                .give_up_after
                .map(|d| d.0)
                .unwrap_or(hardcoded.give_up_after),
            network_wait: self
                .network_wait
                .map(|d| d.0)
                .unwrap_or(hardcoded.network_wait),
            random_delay: self
                .random_delay
                .map(|d| d.0)
                .unwrap_or(hardcoded.random_delay),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedDefaults {
    pub retries: u32,
    pub retry_delay: Duration,
    pub give_up_after: Duration,
    pub network_wait: Duration,
    pub random_delay: Duration,
}

impl ResolvedDefaults {
    pub fn hardcoded() -> Self {
        Self {
            retries: 3,
            retry_delay: Duration::from_secs(2 * 60),
            give_up_after: Duration::from_secs(45 * 60),
            network_wait: Duration::from_secs(10 * 60),
            random_delay: Duration::from_secs(5 * 60),
        }
    }
}

impl From<&ResolvedDefaults> for RetryPolicy {
    fn from(defaults: &ResolvedDefaults) -> Self {
        Self {
            max_retries: defaults.retries,
            base_delay: defaults.retry_delay,
            delay_factor: 3,
            budget: defaults.give_up_after,
            network_cap: defaults.network_wait,
            poll: Duration::from_secs(10),
        }
    }
}

pub fn validate_defaults(defaults: &Defaults) -> Vec<String> {
    let mut errors = Vec::new();
    match defaults.retries {
        None => {}
        Some(retries) if retries > 10 => errors.push(format!("retries {retries} exceeds 10")),
        Some(_) => {}
    }
    let mut positive = |value: Option<TomlDuration>, name: &str| {
        if let Some(duration) = value
            && duration.0.is_zero()
        {
            errors.push(format!("{name} must be greater than zero"));
        }
    };
    positive(defaults.retry_delay, "retry_delay");
    positive(defaults.give_up_after, "give_up_after");
    positive(defaults.network_wait, "network_wait");
    positive(defaults.random_delay, "random_delay");
    if let (Some(delay), Some(limit)) = (defaults.retry_delay, defaults.give_up_after)
        && limit.0 < delay.0
    {
        errors.push("give_up_after is shorter than retry_delay".to_string());
    }
    errors
}

pub fn render_example() -> String {
    let defaults = ResolvedDefaults::hardcoded();
    let duration = |value: Duration| humantime::format_duration(value).to_string();
    [
        "# topmatic defaults - copy uncommented lines into config.toml to override.".to_string(),
        "# This file is regenerated automatically; manual edits are overwritten.".to_string(),
        String::new(),
        "[defaults]".to_string(),
        format!("# retries = {}", defaults.retries),
        format!(
            "# retry_delay = \"{}\"{}",
            duration(defaults.retry_delay),
            "   # wait before the first retry; grows 3x each time"
        ),
        format!(
            "# give_up_after = \"{}\"{}",
            duration(defaults.give_up_after),
            " # stop insisting after this much total time"
        ),
        format!(
            "# network_wait = \"{}\"{}",
            duration(defaults.network_wait),
            "  # wait for the network before starting"
        ),
        format!(
            "# random_delay = \"{}\"{}",
            duration(defaults.random_delay),
            "   # run within this much of the schedule time, never exactly on it"
        ),
        String::new(),
        "# A profile picks the topgrade steps to update and how often.".to_string(),
        "# [[profiles]]".to_string(),
        "# name = \"daily\"".to_string(),
        "# steps = [\"cargo\", \"flatpak\"]".to_string(),
        "# [profiles.schedule]".to_string(),
        "# preset = \"daily\"".to_string(),
        "# hour = 0".to_string(),
        "# minute = 0".to_string(),
        String::new(),
        "# Broken config? Run `topmatic doctor --repair`.".to_string(),
    ]
    .join("\n")
        + "\n"
}

pub fn backup_copy(paths: &Paths) -> std::io::Result<PathBuf> {
    let source = paths.config_file();
    let target = crate::util::unique_sibling(&source, "bak");
    std::fs::copy(&source, &target)?;
    Ok(target)
}

pub fn latest_backup(paths: &Paths) -> Option<PathBuf> {
    let config_file = paths.config_file();
    let dir = config_file.parent()?;
    let prefix = format!("{}.bak-", config_file.file_name()?.to_string_lossy());
    let mut backups: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        })
        .collect();
    backups.sort();
    backups.pop()
}

pub fn repair_broken(paths: &Paths) -> anyhow::Result<Option<PathBuf>> {
    let source = paths.config_file();
    if !source.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&source)?;
    if toml::from_str::<toml::Value>(&text).is_ok() {
        return Ok(None);
    }
    let quarantine = crate::util::unique_sibling(&source, "broken");
    std::fs::rename(&source, &quarantine)?;
    save(paths, &AppConfig::default())?;
    Ok(Some(quarantine))
}

pub fn write_example_if_changed(paths: &Paths) -> std::io::Result<()> {
    crate::util::write_file_if_changed(&paths.example_config_file(), &render_example()).map(|_| ())
}

#[derive(Debug, PartialEq, Eq)]
pub enum SavePlan {
    Create,
    Replace,
    RenameFrom(String),
}

pub fn save_plan(
    config: &AppConfig,
    original_name: Option<&str>,
    new_name: &str,
) -> Result<SavePlan, String> {
    match original_name {
        Some(original) if original == new_name => {
            if config.profile(new_name).is_some() {
                Ok(SavePlan::Replace)
            } else {
                Ok(SavePlan::Create)
            }
        }
        Some(original) => {
            if config.profile(new_name).is_some() {
                Err(format!("profile {new_name} already exists"))
            } else {
                Ok(SavePlan::RenameFrom(original.to_string()))
            }
        }
        None => {
            if config.profile(new_name).is_some() {
                Err(format!("profile {new_name} already exists"))
            } else {
                Ok(SavePlan::Create)
            }
        }
    }
}

pub fn validate_profile(profile: &Profile) -> Vec<String> {
    let mut errors = Vec::new();
    if sanitize_name(&profile.name).is_err() {
        errors.push(format!(
            "invalid name {:?}: must match [a-zA-Z0-9][a-zA-Z0-9_.-]*",
            profile.name
        ));
    }
    let carries_base = profile.base.is_some();
    if profile.steps.is_empty() && !carries_base {
        errors.push("no steps selected".to_string());
    }
    for step in &profile.steps {
        if step.trim().is_empty() {
            errors.push("step id must not be empty".to_string());
        } else if step.starts_with('-') {
            errors.push(format!("step id must not start with '-': {step}"));
        } else if step.chars().any(char::is_whitespace) {
            errors.push(format!("step id must not contain whitespace: {step}"));
        }
    }
    match &profile.schedule.preset {
        SchedulePreset::EveryNHours { hours } if !(1..=23).contains(hours) => {
            errors.push(format!("every-n-hours out of range (1..=23): {hours}"));
        }
        SchedulePreset::Daily { hour, minute } | SchedulePreset::Weekly { hour, minute, .. }
            if *hour > 23 || *minute > 59 =>
        {
            errors.push(format!("time out of range: {hour:02}:{minute:02}"));
        }
        SchedulePreset::Custom { calendar } if calendar.trim().is_empty() => {
            errors.push("custom OnCalendar expression is empty".to_string());
        }
        _ => {}
    }
    errors
}

pub fn load_validated(paths: &Paths) -> anyhow::Result<(AppConfig, Vec<String>)> {
    let path = paths.config_file();
    if !path.exists() {
        return Ok((AppConfig::default(), Vec::new()));
    }
    let text = std::fs::read_to_string(&path)?;
    let value: toml::Value = toml::from_str(&text).map_err(|error| {
        anyhow::anyhow!(
            "invalid config at {}: {error} — run `topmatic doctor --repair` to quarantine it and start fresh",
            path.display()
        )
    })?;
    let mut config = AppConfig::default();
    let mut issues = Vec::new();
    if let Some(raw) = value.get("defaults") {
        match Defaults::deserialize(raw.clone()) {
            Ok(defaults) => {
                let errors = validate_defaults(&defaults);
                if errors.is_empty() {
                    config.defaults = defaults;
                } else {
                    issues.push(format!(
                        "invalid [defaults] ({}): using hardcoded defaults",
                        errors.join("; ")
                    ));
                }
            }
            Err(error) => issues.push(format!(
                "invalid [defaults] ({error}): using hardcoded defaults"
            )),
        }
    }
    let random_delay = config.defaults.resolved().random_delay.as_secs();
    let Some(profiles) = value.get("profiles").and_then(|v| v.as_array()) else {
        return Ok((config, issues));
    };
    for (index, entry) in profiles.iter().enumerate() {
        let has_jitter = entry
            .get("schedule")
            .and_then(|schedule| schedule.get("randomized_delay_sec"))
            .is_some();
        match Profile::deserialize(entry.clone()) {
            Ok(mut profile) => {
                profile.schedule.preset = profile.schedule.preset.clone().normalized();
                if !has_jitter {
                    profile.schedule.randomized_delay_sec = random_delay;
                }
                if config.profile(&profile.name).is_some() {
                    issues.push(format!(
                        "profile {} defined twice (kept the last)",
                        profile.name
                    ));
                }
                let errors = validate_profile(&profile);
                if errors.is_empty() {
                    config.upsert(profile);
                } else {
                    issues.push(format!("{}: {}", profile.name, errors.join("; ")));
                }
            }
            Err(error) => {
                let name = match entry.get("name").and_then(|n| n.as_str()) {
                    Some(name) => name.to_string(),
                    None => format!("#{index}"),
                };
                issues.push(format!("invalid profile {name}: {error}"));
            }
        }
    }
    Ok((config, issues))
}

impl AppConfig {
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn profile_mut(&mut self, name: &str) -> Option<&mut Profile> {
        self.profiles.iter_mut().find(|p| p.name == name)
    }

    pub fn upsert(&mut self, profile: Profile) {
        match self.profiles.iter_mut().find(|p| p.name == profile.name) {
            Some(existing) => *existing = profile,
            None => self.profiles.push(profile),
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.profiles.len();
        self.profiles.retain(|p| p.name != name);
        before != self.profiles.len()
    }
}

pub fn load(paths: &Paths) -> anyhow::Result<AppConfig> {
    Ok(load_validated(paths)?.0)
}

pub fn save(paths: &Paths, config: &AppConfig) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.config_dir)?;
    let body = toml::to_string_pretty(config)?;
    let content = format!("{HEADER}{body}");
    let target = paths.config_file();
    let tmp = target.with_extension("toml.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &target)?;
    write_example_if_changed(paths)?;
    Ok(())
}

pub fn template() -> String {
    "# topmatic configuration.\n\
     # Add profiles with the TUI (or hand-edit below); topmatic reconciles systemd on next open.\n\
     #\n\
     # Example:\n\
     # [[profiles]]\n\
     # name = \"daily\"\n\
     # steps = [\"cargo\", \"flatpak\"]\n\
     # [profiles.schedule]\n\
     # preset = \"daily\"\n\
     # randomized_delay_sec = 1800\n"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::schedule::Schedule;
    use std::path::PathBuf;

    fn paths() -> Paths {
        Paths::with_bases(PathBuf::from("/cfg"), PathBuf::from("/state"))
    }

    fn sample_profile(name: &str) -> Profile {
        Profile {
            name: name.to_string(),
            base: None,
            extra_steps: Vec::new(),
            excluded_steps: Vec::new(),
            steps: vec!["flatpak".to_string()],
            schedule: Schedule::default(),
            notify: Default::default(),
            scope: Default::default(),
        }
    }

    #[test]
    fn upsert_replaces_by_name_and_appends_new() {
        let mut config = AppConfig::default();
        let p1 = sample_profile("alpha");
        config.upsert(p1.clone());
        config.upsert(sample_profile("beta"));
        let mut p1_updated = sample_profile("alpha");
        p1_updated.steps = vec!["cargo".to_string()];
        config.upsert(p1_updated);
        assert_eq!(config.profiles.len(), 2);
        assert_eq!(config.profile("alpha").unwrap().steps, vec!["cargo"]);
    }

    #[test]
    fn remove_reports_whether_profile_existed() {
        let mut config = AppConfig::default();
        config.upsert(sample_profile("alpha"));
        assert!(config.remove("alpha"));
        assert!(!config.remove("alpha"));
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn durations_parse_human_units_and_round_trip() {
        let defaults: Defaults =
            toml::from_str("retries = 2\nretry_delay = \"90s\"\nnetwork_wait = \"2min 30s\"")
                .unwrap();
        assert_eq!(defaults.retries, Some(2));
        assert_eq!(
            defaults.retry_delay.map(|d| d.0),
            Some(Duration::from_secs(90))
        );
        assert_eq!(
            defaults.network_wait.map(|d| d.0),
            Some(Duration::from_secs(150))
        );
        let text = toml::to_string(&defaults).unwrap();
        let back: Defaults = toml::from_str(&text).unwrap();
        assert_eq!(defaults, back);
    }

    #[test]
    fn garbage_durations_are_reported_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(
            paths.config_file(),
            "[defaults]\nretry_delay = \"two weeks \"\n[[profiles]]\nname = \"ok\"\nsteps = [\"flatpak\"]\n[profiles.schedule]\npreset = \"daily\"\nhour = 0\nminute = 0\n",
        )
        .unwrap();

        let (config, issues) = load_validated(&paths).unwrap();
        assert_eq!(
            config.defaults,
            Defaults::default(),
            "garbage falls back wholesale"
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("invalid [defaults]"));
        assert_eq!(config.profiles.len(), 1, "profiles still load");
    }

    #[test]
    fn empty_defaults_resolve_to_hardcoded_and_overrides_apply() {
        assert_eq!(
            Defaults::default().resolved(),
            ResolvedDefaults::hardcoded()
        );

        let defaults: Defaults = toml::from_str("retries = 1\ngive_up_after = \"9min\"").unwrap();
        let resolved = defaults.resolved();
        assert_eq!(resolved.retries, 1);
        assert_eq!(resolved.give_up_after, Duration::from_secs(9 * 60));
        assert_eq!(
            resolved.retry_delay,
            ResolvedDefaults::hardcoded().retry_delay
        );
    }

    #[test]
    fn invalid_combinations_reject_the_whole_table() {
        let defaults: Defaults =
            toml::from_str("retries = 2\nretry_delay = \"10min\"\ngive_up_after = \"5min\"")
                .unwrap();
        assert!(!validate_defaults(&defaults).is_empty());
        assert_eq!(defaults.resolved(), ResolvedDefaults::hardcoded());

        let too_many: Defaults = toml::from_str("retries = 11").unwrap();
        assert!(!validate_defaults(&too_many).is_empty());

        let zero: Defaults = toml::from_str("network_wait = \"0s\"").unwrap();
        assert!(!validate_defaults(&zero).is_empty());
    }

    #[test]
    fn retry_policy_default_never_drifts_from_hardcoded_resolution() {
        assert_eq!(
            crate::runner::retry::RetryPolicy::from(&ResolvedDefaults::hardcoded()),
            crate::runner::retry::RetryPolicy::default()
        );
    }

    #[test]
    fn example_file_rewrites_stale_content_and_leaves_fresh_ones_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(paths.example_config_file(), "stale garbage").unwrap();

        write_example_if_changed(&paths).unwrap();
        assert_eq!(
            std::fs::read_to_string(paths.example_config_file()).unwrap(),
            render_example()
        );
        let first = std::fs::metadata(paths.example_config_file())
            .unwrap()
            .modified()
            .unwrap();
        write_example_if_changed(&paths).unwrap();
        let second = std::fs::metadata(paths.example_config_file())
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(first, second, "idempotent regeneration keeps mtime");
        assert!(render_example().contains("# retries = 3"));
        assert!(render_example().contains("random_delay"));
        assert!(
            render_example().contains("grows 3x"),
            "the increment must be visible"
        );
    }

    #[test]
    fn save_also_writes_the_example_file() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        let mut config = AppConfig::default();
        config.upsert(sample_profile("alpha"));
        save(&paths, &config).unwrap();
        assert!(paths.example_config_file().is_file());
    }

    #[test]
    fn defaults_round_trip_through_disk_without_polluting_empty_configs() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        let mut config = AppConfig {
            defaults: toml::from_str("retries = 2\nrandom_delay = \"3min\"").unwrap(),
            profiles: Vec::new(),
        };
        config.upsert(sample_profile("alpha"));
        save(&paths, &config).unwrap();
        let text = std::fs::read_to_string(paths.config_file()).unwrap();
        assert!(text.contains("[defaults]"));
        assert!(text.contains("retries = 2"));

        let clean = AppConfig::default();
        save(&paths, &clean).unwrap();
        let clean_text = std::fs::read_to_string(paths.config_file()).unwrap();
        assert!(
            !clean_text.contains("[defaults]"),
            "empty tables stay out of the file"
        );
    }

    #[test]
    fn missing_profile_jitter_inherits_the_global_default() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(
            paths.config_file(),
            "[defaults]\nrandom_delay = \"9min\"\n\n[[profiles]]\nname = \"bare\"\nsteps = [\"flatpak\"]\n[profiles.schedule]\npreset = \"daily\"\nhour = 0\nminute = 0\n\n[[profiles]]\nname = \"explicit\"\nsteps = [\"cargo\"]\n[profiles.schedule]\npreset = \"daily\"\nhour = 0\nminute = 0\nrandomized_delay_sec = 120\n",
        )
        .unwrap();

        let (config, issues) = load_validated(&paths).unwrap();
        assert!(issues.is_empty());
        assert_eq!(
            config
                .profile("bare")
                .unwrap()
                .schedule
                .randomized_delay_sec,
            540
        );
        assert_eq!(
            config
                .profile("explicit")
                .unwrap()
                .schedule
                .randomized_delay_sec,
            120,
            "explicit profile values win over the global default"
        );
    }

    #[test]
    fn broken_config_error_points_at_doctor_repair() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(paths.config_file(), "[[profiles]\nbroken").unwrap();

        let error = load_validated(&paths).unwrap_err().to_string();
        assert!(error.contains("invalid config at"));
        assert!(error.contains("doctor --repair"));
    }

    #[test]
    fn repair_quarantines_broken_and_writes_a_fresh_config() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(paths.config_file(), "definitely not [ toml").unwrap();

        let quarantine = repair_broken(&paths)
            .unwrap()
            .expect("broken file quarantined");
        assert!(quarantine.is_file(), "the broken original is preserved");
        assert!(
            quarantine
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("config.toml.broken-")
        );
        let (config, issues) = load_validated(&paths).unwrap();
        assert_eq!(config, AppConfig::default());
        assert!(issues.is_empty());
        assert!(paths.example_config_file().is_file());

        assert_eq!(
            repair_broken(&paths).unwrap(),
            None,
            "a valid config is left alone"
        );
    }

    #[test]
    fn backup_copy_snapshots_before_editing_and_latest_backup_finds_it() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(paths.config_file(), "current").unwrap();

        let backup = backup_copy(&paths).unwrap();
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "current");
        assert_eq!(latest_backup(&paths).as_deref(), Some(backup.as_path()));
        assert!(
            paths.config_file().is_file(),
            "the original stays for the editor"
        );
    }

    #[test]
    fn example_profiles_skeleton_is_valid_when_uncommented() {
        let skeleton: String = render_example()
            .lines()
            .filter(|line| line.starts_with("# [["))
            .map(|line| line.trim_start_matches("# ").to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(skeleton.contains("[[profiles]]"));
        let parsed: toml::Value = toml::from_str(&skeleton).unwrap();
        assert!(parsed.get("profiles").is_some_and(|p| p.is_array()));
    }

    #[test]
    fn template_parses_as_an_empty_config_and_documents_profiles() {
        let text = template();
        assert!(text.contains("[[profiles]]"));
        let parsed: AppConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, AppConfig::default());
    }

    #[test]
    fn round_trips_through_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        let mut config = AppConfig::default();
        config.upsert(sample_profile("flatpak-daily"));
        save(&paths, &config).unwrap();
        let loaded = load(&paths).unwrap();
        assert_eq!(loaded, config);
        let text = std::fs::read_to_string(paths.config_file()).unwrap();
        assert!(text.starts_with("# topmatic configuration"));
    }

    #[test]
    fn load_normalizes_legacy_spread_schedules() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(
            paths.config_file(),
            "[[profiles]]\nname = \"old\"\nsteps = [\"flatpak\"]\n[profiles.schedule]\npreset = \"spread\"\nperiod = \"daily\"\nrandomized_delay_sec = 600\n",
        )
        .unwrap();

        let loaded = load(&paths).unwrap();
        assert_eq!(
            loaded.profiles[0].schedule.preset,
            crate::domain::schedule::SchedulePreset::Daily { hour: 0, minute: 0 }
        );
        assert_eq!(loaded.profiles[0].schedule.randomized_delay_sec, 600);
    }

    #[test]
    fn load_returns_empty_config_when_file_missing() {
        let loaded = load(&paths()).unwrap();
        assert_eq!(loaded, AppConfig::default());
    }

    fn profile_named(name: &str) -> Profile {
        Profile {
            name: name.to_string(),
            ..sample_profile("x")
        }
    }

    #[test]
    fn validator_rejects_hand_edited_nonsense() {
        let mut bad_hours = sample_profile("alpha");
        bad_hours.schedule.preset = SchedulePreset::Daily {
            hour: 25,
            minute: 61,
        };
        let mut bad_every = sample_profile("alpha");
        bad_every.schedule.preset = SchedulePreset::EveryNHours { hours: 0 };
        let mut no_steps = sample_profile("alpha");
        no_steps.steps = Vec::new();

        assert_eq!(validate_profile(&profile_named("has space")).len(), 1);
        assert_eq!(validate_profile(&profile_named("-lead")).len(), 1);
        assert!(!validate_profile(&bad_hours).is_empty());
        assert!(!validate_profile(&bad_every).is_empty());
        assert!(!validate_profile(&no_steps).is_empty());
        assert!(validate_profile(&sample_profile("flatpak-daily")).is_empty());
    }

    #[test]
    fn load_validated_filters_invalid_profiles_and_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        let mut raw = AppConfig::default();
        raw.upsert(sample_profile("good-one"));
        let mut bad = sample_profile("bad job");
        bad.schedule.preset = SchedulePreset::Custom {
            calendar: "  ".into(),
        };
        raw.upsert(bad);
        save(&paths, &raw).unwrap();

        let (valid, issues) = load_validated(&paths).unwrap();
        assert_eq!(valid.profiles.len(), 1);
        assert_eq!(valid.profiles[0].name, "good-one");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].starts_with("bad job"));
        assert!(issues[0].contains("invalid name"));
        assert!(issues[0].contains("OnCalendar"));
    }

    #[test]
    fn save_plan_replaces_same_name_and_blocks_duplicates() {
        let mut config = AppConfig::default();
        config.upsert(sample_profile("alpha"));
        assert_eq!(
            save_plan(&config, Some("alpha"), "alpha"),
            Ok(SavePlan::Replace)
        );
        assert_eq!(
            save_plan(&config, None, "alpha"),
            Err("profile alpha already exists".to_string())
        );
        assert_eq!(
            save_plan(&config, Some("alpha"), "beta"),
            Ok(SavePlan::RenameFrom("alpha".to_string()))
        );
        assert_eq!(
            save_plan(&config, Some("alpha"), "alpha"),
            Ok(SavePlan::Replace)
        );
        config.upsert(sample_profile("beta"));
        assert!(
            save_plan(&config, Some("alpha"), "beta").is_err(),
            "rename onto an existing profile must stay a no-op"
        );
        assert_eq!(save_plan(&config, None, "gamma"), Ok(SavePlan::Create));
    }

    #[test]
    fn load_validated_survives_structural_errors_per_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(
            paths.config_file(),
            "[[profiles]]\nname = \"good\"\nsteps = [\"flatpak\"]\n[profiles.schedule]\npreset = \"daily\"\nhour = 5\nminute = 0\n\n[[profiles]]\nnot_a_profile_here = true\n\n[[profiles]]\nname = \"dup\"\nsteps = [\"cargo\"]\n[profiles.schedule]\npreset = \"daily\"\nhour = 5\nminute = 0\n\n[[profiles]]\nname = \"dup\"\nsteps = [\"flatpak\"]\n[profiles.schedule]\npreset = \"daily\"\nhour = 5\nminute = 0\n",
        )
        .unwrap();

        let (valid, issues) = load_validated(&paths).unwrap();
        assert_eq!(valid.profiles.len(), 2, "good and last dup survive");
        assert_eq!(valid.profiles[1].name, "dup");
        assert_eq!(valid.profiles[1].steps, vec!["flatpak".to_string()]);
        let joined = issues.join("\n");
        assert!(joined.contains("invalid profile #1"));
        assert!(joined.contains("dup defined twice"));
    }

    #[test]
    fn validator_rejects_bad_step_identifiers() {
        let mut leading_dash = sample_profile("a");
        leading_dash.steps = vec!["-help".to_string()];
        let mut spaces = sample_profile("b");
        spaces.steps = vec!["cargo install".to_string()];
        let mut empty = sample_profile("c");
        empty.steps = vec!["".to_string()];
        assert!(!validate_profile(&leading_dash).is_empty());
        assert!(!validate_profile(&spaces).is_empty());
        assert!(!validate_profile(&empty).is_empty());
    }
}
