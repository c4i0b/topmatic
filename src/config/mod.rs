use serde::{Deserialize, Serialize};

use crate::domain::profile::{Profile, sanitize_name};
use crate::domain::schedule::SchedulePreset;
use crate::paths::Paths;

const HEADER: &str =
    "# topmatic configuration. Edit freely; topmatic reconciles systemd on next open.\n\n";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<Profile>,
}

pub fn validate_profile(profile: &Profile) -> Vec<String> {
    let mut errors = Vec::new();
    if sanitize_name(&profile.name).is_err() {
        errors.push(format!(
            "invalid name {:?}: must match [a-zA-Z0-9][a-zA-Z0-9_.-]*",
            profile.name
        ));
    }
    if profile.steps.is_empty() {
        errors.push("no steps selected".to_string());
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
    let raw = load(paths)?;
    let mut config = AppConfig::default();
    let mut issues = Vec::new();
    for profile in raw.profiles {
        let errors = validate_profile(&profile);
        if errors.is_empty() {
            config.upsert(profile);
        } else {
            issues.push(format!("{}: {}", profile.name, errors.join("; ")));
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
    let path = paths.config_file();
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&text)?)
}

pub fn save(paths: &Paths, config: &AppConfig) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.config_dir)?;
    let body = toml::to_string_pretty(config)?;
    let content = format!("{HEADER}{body}");
    let target = paths.config_file();
    let tmp = target.with_extension("toml.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &target)?;
    Ok(())
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
            steps: vec!["flatpak".to_string()],
            schedule: Schedule::default(),
            cleanup: true,
            notify: Default::default(),
            scope: Default::default(),
        }
    }

    #[test]
    fn upsert_replaces_by_name_and_appends_new() {
        let mut config = AppConfig::default();
        let mut p1 = sample_profile("alpha");
        p1.cleanup = false;
        config.upsert(p1.clone());
        config.upsert(sample_profile("beta"));
        let mut p1_updated = sample_profile("alpha");
        p1_updated.steps = vec!["cargo".to_string()];
        config.upsert(p1_updated);
        assert_eq!(config.profiles.len(), 2);
        assert_eq!(config.profile("alpha").unwrap().steps, vec!["cargo"]);
        assert!(config.profile("alpha").unwrap().cleanup);
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
}
