use serde::{Deserialize, Serialize};

use super::schedule::Schedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NotifyPolicy {
    Always,
    #[default]
    OnFailure,
    Never,
}

impl NotifyPolicy {
    pub const ALL: [NotifyPolicy; 3] = [
        NotifyPolicy::Always,
        NotifyPolicy::OnFailure,
        NotifyPolicy::Never,
    ];

    pub fn from_index(index: usize) -> Self {
        Self::ALL[index % Self::ALL.len()]
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|policy| *policy == self)
            .unwrap_or(1)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub extra_steps: Vec<String>,
    #[serde(default)]
    pub excluded_steps: Vec<String>,
    #[serde(default)]
    pub schedule: Schedule,
    #[serde(default)]
    pub notify: NotifyPolicy,
}

impl Serialize for Profile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let overlay = self.base.is_some();
        let mut state = serializer.serialize_struct("Profile", 8)?;
        state.serialize_field("name", &self.name)?;
        if !self.steps.is_empty() {
            state.serialize_field("steps", &self.steps)?;
        }
        if let Some(base) = &self.base {
            state.serialize_field("base", base)?;
        }
        if !self.extra_steps.is_empty() {
            state.serialize_field("extra_steps", &self.extra_steps)?;
        }
        if !self.excluded_steps.is_empty() {
            state.serialize_field("excluded_steps", &self.excluded_steps)?;
        }
        let schedule_is_default = self.schedule == Schedule::default();
        if !overlay || !schedule_is_default {
            state.serialize_field("schedule", &self.schedule)?;
        }
        if !overlay || self.notify != NotifyPolicy::default() {
            state.serialize_field("notify", &self.notify)?;
        }
        state.end()
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error(
    "invalid profile name {0:?}: must start with a letter or digit and contain only letters, digits, '_', '.', '-'"
)]
pub struct InvalidNameError(pub String);

pub fn sanitize_name(input: &str) -> Result<String, InvalidNameError> {
    let trimmed = input.trim();
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return Err(InvalidNameError(trimmed.to_string())),
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')) {
        return Err(InvalidNameError(trimmed.to_string()));
    }
    if trimmed.chars().count() > 64 {
        return Err(InvalidNameError(
            "name longer than 64 characters".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_toml(extra_fields: &str) -> String {
        let schedule_toml = toml::to_string(&Schedule::default()).unwrap();
        format!(
            "name = 'flatpak-daily'\nsteps = ['flatpak']\n{extra_fields}\n[schedule]\n{}",
            schedule_toml.trim()
        )
    }

    fn minimal_toml() -> String {
        profile_toml("")
    }

    #[test]
    fn accepts_valid_names() {
        for name in ["flatpak-daily", "a", "Dev.Tools_2", "weekly1"] {
            assert_eq!(sanitize_name(name).unwrap(), name);
        }
        assert_eq!(sanitize_name("  padded  ").unwrap(), "padded");
    }

    #[test]
    fn rejects_invalid_names() {
        for name in ["", "-x", ".hidden", "has space", "dígito", "a/b", "a:b"] {
            assert!(sanitize_name(name).is_err(), "{name} should be rejected");
        }
    }

    #[test]
    fn rejects_names_longer_than_64_chars() {
        let long = "a".repeat(65);
        assert!(sanitize_name(&long).is_err());
        let ok = "a".repeat(64);
        assert!(sanitize_name(&ok).is_ok());
    }

    #[test]
    fn deserializing_minimal_profile_applies_defaults() {
        let profile: Profile = toml::from_str(&minimal_toml()).unwrap();
        assert_eq!(profile.name, "flatpak-daily");
        assert_eq!(profile.steps, vec!["flatpak"]);
        assert_eq!(profile.notify, NotifyPolicy::OnFailure);
    }

    #[test]
    fn profile_round_trips_through_toml() {
        let profile: Profile = toml::from_str(&minimal_toml()).unwrap();
        let text = toml::to_string(&profile).unwrap();
        let back: Profile = toml::from_str(&text).unwrap();
        assert_eq!(profile, back);
    }

    #[test]
    fn explicit_values_override_defaults() {
        let text = profile_toml("notify = 'always'\nscope = 'system'");
        let profile: Profile = toml::from_str(&text).unwrap();
        assert_eq!(profile.notify, NotifyPolicy::Always);
    }
}
