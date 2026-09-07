use std::collections::BTreeSet;

use super::presets::PRESET_IDS;
use super::profile::{NotifyPolicy, Profile};
use super::schedule::Schedule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSteps {
    Everything { excluded: Vec<String> },
    Explicit(Vec<String>),
}

impl ResolvedSteps {
    pub fn is_empty(&self) -> bool {
        match self {
            ResolvedSteps::Everything { .. } => false,
            ResolvedSteps::Explicit(steps) => steps.is_empty(),
        }
    }
}

pub fn base_index(base: &str) -> Option<usize> {
    PRESET_IDS.iter().position(|id| *id == base)
}

pub fn preset_steps(base: &str, catalog: &[String]) -> Option<Vec<String>> {
    base_index(base).map(|index| super::presets::steps_for(index, catalog))
}

pub fn is_everything_base(base: &str) -> bool {
    base == "all"
}

pub fn resolved_steps(profile: &Profile, catalog: &[String]) -> ResolvedSteps {
    match profile.base.as_deref() {
        None => {
            if profile.steps.is_empty() {
                ResolvedSteps::Everything {
                    excluded: Vec::new(),
                }
            } else {
                ResolvedSteps::Explicit(profile.steps.clone())
            }
        }
        Some(base) if is_everything_base(base) => ResolvedSteps::Everything {
            excluded: profile.excluded_steps.clone(),
        },
        Some(base) => {
            let preset = preset_steps(base, catalog).unwrap_or_default();
            let mut merged: BTreeSet<String> = preset.into_iter().collect();
            for step in &profile.extra_steps {
                merged.insert(step.clone());
            }
            for step in &profile.excluded_steps {
                merged.remove(step);
            }
            ResolvedSteps::Explicit(merged.into_iter().collect())
        }
    }
}

pub fn everything_ignore(profile: &Profile) -> Vec<String> {
    let mut ignored: BTreeSet<String> = profile.excluded_steps.iter().cloned().collect();
    ignored.extend(
        super::steps::PRIVILEGED_STEPS
            .iter()
            .map(|step| step.to_string()),
    );
    ignored.into_iter().collect()
}

pub fn resolved_schedule(profile: &Profile) -> Schedule {
    profile.schedule.clone()
}

pub fn resolved_notify(profile: &Profile) -> NotifyPolicy {
    profile.notify
}

pub struct StepDelta {
    pub extra_steps: Vec<String>,
    pub excluded_steps: Vec<String>,
}

pub fn step_delta(
    base: Option<&str>,
    selected: &BTreeSet<String>,
    catalog: &[String],
) -> StepDelta {
    match base {
        None => StepDelta {
            extra_steps: Vec::new(),
            excluded_steps: Vec::new(),
        },
        Some(base) if is_everything_base(base) => StepDelta {
            extra_steps: Vec::new(),
            excluded_steps: catalog
                .iter()
                .filter(|step| !selected.contains(step.as_str()))
                .cloned()
                .collect(),
        },
        Some(base) => {
            let preset: BTreeSet<String> = preset_steps(base, catalog)
                .unwrap_or_default()
                .into_iter()
                .collect();
            StepDelta {
                extra_steps: selected.difference(&preset).cloned().collect(),
                excluded_steps: preset.difference(selected).cloned().collect(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<String> {
        ["cargo", "rustup", "node", "flatpak", "vim"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn profile(name: &str, base: Option<&str>, steps: &[&str]) -> Profile {
        Profile {
            name: name.to_string(),
            steps: steps.iter().map(|s| s.to_string()).collect(),
            base: base.map(str::to_string),
            extra_steps: Vec::new(),
            excluded_steps: Vec::new(),
            schedule: Schedule::default(),
            notify: NotifyPolicy::default(),
        }
    }

    fn set(steps: &[&str]) -> BTreeSet<String> {
        steps.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn baseless_profiles_resolve_to_explicit_or_everything() {
        let explicit = profile("custom", None, &["cargo"]);
        assert_eq!(
            resolved_steps(&explicit, &catalog()),
            ResolvedSteps::Explicit(vec!["cargo".to_string()])
        );
        let empty = profile("everything", None, &[]);
        assert_eq!(
            resolved_steps(&empty, &catalog()),
            ResolvedSteps::Everything {
                excluded: Vec::new()
            }
        );
    }

    #[test]
    fn all_base_resolves_to_everything_with_exclusions_only() {
        let mut p = profile("all-daily", Some("all"), &[]);
        p.excluded_steps = vec!["brew".to_string()];
        assert_eq!(
            resolved_steps(&p, &catalog()),
            ResolvedSteps::Everything {
                excluded: vec!["brew".to_string()]
            }
        );
        p.extra_steps = vec!["whatever".to_string()];
        assert_eq!(
            resolved_steps(&p, &catalog()),
            ResolvedSteps::Everything {
                excluded: vec!["brew".to_string()]
            },
            "extra is meaningless on the everything base and ignored"
        );
    }

    #[test]
    fn enumerated_base_merges_extra_and_excluded() {
        let mut p = profile("dev-daily", Some("dev-tools"), &[]);
        p.extra_steps = vec!["flatpak".to_string()];
        p.excluded_steps = vec!["node".to_string()];
        let resolved = resolved_steps(&p, &catalog());
        let ResolvedSteps::Explicit(steps) = resolved else {
            panic!("enumerated base must resolve explicitly");
        };
        assert!(steps.contains(&"cargo".to_string()));
        assert!(steps.contains(&"flatpak".to_string()), "extra is merged in");
        assert!(!steps.contains(&"node".to_string()), "excluded is removed");
    }

    #[test]
    fn step_delta_round_trips_through_the_editor_selection() {
        let base = "dev-tools";
        let selected = set(&["cargo", "rustup", "flatpak", "vim"]);
        let delta = step_delta(Some(base), &selected, &catalog());
        let mut p = profile("dev-daily", Some(base), &[]);
        p.extra_steps = delta.extra_steps.clone();
        p.excluded_steps = delta.excluded_steps.clone();
        assert_eq!(
            resolved_steps(&p, &catalog()),
            ResolvedSteps::Explicit(
                ["cargo", "flatpak", "rustup", "vim"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            ),
            "delta applied to the base reproduces the editor selection"
        );
    }

    #[test]
    fn delta_is_recomputed_fresh_so_removed_overrides_disappear() {
        let base = "dev-tools";
        let selected = set(&["cargo", "rustup", "node", "vim"]);
        let with_override = step_delta(Some(base), &selected, &catalog());
        assert!(!with_override.extra_steps.contains(&"flatpak".to_string()));

        let preset_only = set(&["cargo", "rustup", "node", "vim"]);
        let reverted = step_delta(Some(base), &preset_only, &catalog());
        assert!(
            reverted.extra_steps.is_empty() && reverted.excluded_steps.is_empty(),
            "matching the preset again yields an empty delta — fields omit from the config"
        );
    }

    #[test]
    fn empty_overlay_fields_are_omitted_from_serialization() {
        let mut p = profile("dev-daily", Some("dev-tools"), &[]);
        let text = toml::to_string(&p).unwrap();
        assert!(
            !text.contains("extra_steps") && !text.contains("excluded_steps"),
            "empty overlay vectors do not land in the config:\n{text}"
        );
        assert!(text.contains("base = \"dev-tools\""));

        p.extra_steps = vec!["flatpak".to_string()];
        p.excluded_steps = vec!["node".to_string()];
        let text = toml::to_string(&p).unwrap();
        assert!(text.contains("extra_steps") && text.contains("excluded_steps"));

        let plain = profile("custom", None, &["cargo"]);
        let text = toml::to_string(&plain).unwrap();
        assert!(
            !text.contains("base"),
            "baseless profiles omit the base key"
        );
    }

    #[test]
    fn everything_mode_auto_ignores_privileged_steps() {
        let mut p = profile("all-daily", Some("all"), &[]);
        p.excluded_steps = vec!["system".to_string()];
        let ignored = everything_ignore(&p);
        assert!(
            ignored.contains(&"system".to_string()),
            "user exclusions survive"
        );
        assert!(
            super::super::steps::PRIVILEGED_STEPS
                .iter()
                .all(|step| ignored.contains(&step.to_string())),
            "the whole privileged list lands in the ignore set — no-sudo premise holds"
        );
        assert_eq!(ignored, {
            let mut sorted = ignored.clone();
            sorted.sort();
            sorted
        });
    }

    #[test]
    fn everything_base_delta_treats_unselected_as_excluded() {
        let cat = catalog();
        let selected = set(&["cargo", "rustup", "node", "flatpak", "vim"]);
        assert!(
            step_delta(Some("all"), &selected, &cat)
                .excluded_steps
                .is_empty()
        );

        let without_vim = set(&["cargo", "rustup", "node", "flatpak"]);
        assert_eq!(
            step_delta(Some("all"), &without_vim, &cat).excluded_steps,
            vec!["vim".to_string()]
        );
    }
}
