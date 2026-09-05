use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::domain::profile::{NotifyPolicy, Profile, Scope, sanitize_name};
use crate::domain::schedule::{Schedule, SchedulePreset, Weekday};
use crate::domain::steps::StepEntry;
use crate::systemd::validate_on_calendar;

use super::input::LineEdit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Name,
    Steps,
    Schedule,
    Options,
    Actions,
}

const SECTIONS: [Section; 5] = [
    Section::Name,
    Section::Steps,
    Section::Schedule,
    Section::Options,
    Section::Actions,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresetKind {
    Hourly,
    EveryNHours,
    Daily,
    Weekly,
    Custom,
}

const PRESET_ORDER: [PresetKind; 5] = [
    PresetKind::Hourly,
    PresetKind::EveryNHours,
    PresetKind::Daily,
    PresetKind::Weekly,
    PresetKind::Custom,
];

impl SchedulePreset {
    fn kind(&self) -> PresetKind {
        match self {
            SchedulePreset::Hourly => PresetKind::Hourly,
            SchedulePreset::EveryNHours { .. } => PresetKind::EveryNHours,
            SchedulePreset::Daily { .. } => PresetKind::Daily,
            SchedulePreset::Weekly { .. } => PresetKind::Weekly,
            SchedulePreset::Custom { .. } => PresetKind::Custom,
        }
    }

    fn hour(&self) -> u32 {
        match self {
            SchedulePreset::Daily { hour, .. } | SchedulePreset::Weekly { hour, .. } => *hour,
            _ => 12,
        }
    }

    fn minute(&self) -> u32 {
        match self {
            SchedulePreset::Daily { minute, .. } | SchedulePreset::Weekly { minute, .. } => *minute,
            _ => 0,
        }
    }

    fn weekday(&self) -> Weekday {
        match self {
            SchedulePreset::Weekly { weekday, .. } => *weekday,
            _ => Weekday::Mon,
        }
    }
}

pub struct EditorState {
    pub creating: bool,
    pub name: LineEdit,
    pub filter: LineEdit,
    pub custom: LineEdit,
    pub catalog: Vec<StepEntry>,
    pub selected_steps: BTreeSet<String>,
    pub schedule: Schedule,
    pub cleanup: bool,
    pub notify: NotifyPolicy,
    pub enabled: bool,
    pub section: Section,
    pub list_index: usize,
    pub schedule_index: usize,
    pub option_index: usize,
    pub action_index: usize,
}

pub enum EditorEvent {
    None,
    Cancel,
    RequestSave,
}

impl EditorState {
    pub fn new(original: Option<&Profile>, catalog: Vec<StepEntry>) -> Self {
        match original {
            Some(profile) => Self {
                creating: false,
                name: LineEdit::new(profile.name.clone()),
                filter: LineEdit::new(String::new()),
                custom: LineEdit::new(
                    if let SchedulePreset::Custom { calendar } = &profile.schedule.preset {
                        calendar.clone()
                    } else {
                        String::new()
                    },
                ),
                catalog,
                selected_steps: profile.steps.iter().cloned().collect(),
                schedule: profile.schedule.clone(),
                cleanup: profile.cleanup,
                notify: profile.notify,
                enabled: profile.enabled,
                section: Section::Steps,
                list_index: 0,
                schedule_index: 0,
                option_index: 0,
                action_index: 0,
            },
            None => Self {
                creating: true,
                name: LineEdit::new(String::new()),
                filter: LineEdit::new(String::new()),
                custom: LineEdit::new(String::new()),
                catalog,
                selected_steps: BTreeSet::new(),
                schedule: Schedule::default(),
                cleanup: true,
                notify: NotifyPolicy::OnFailure,
                enabled: true,
                section: Section::Name,
                list_index: 0,
                schedule_index: 0,
                option_index: 0,
                action_index: 0,
            },
        }
    }

    pub fn filtered_steps(&self) -> Vec<&StepEntry> {
        let needle = self.filter.value.to_lowercase();
        let mut entries: Vec<&StepEntry> = self
            .catalog
            .iter()
            .filter(|entry| needle.is_empty() || entry.id.contains(&needle))
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.category.is_some()));
        entries
    }

    pub fn to_profile(&self) -> Result<Profile, String> {
        let name = sanitize_name(&self.name.value).map_err(|error| error.to_string())?;
        if self.selected_steps.is_empty() {
            return Err("select at least one step".to_string());
        }
        let preset = match &self.schedule.preset {
            SchedulePreset::Custom { calendar } => {
                let calendar = calendar.trim();
                if self.custom.value.trim().is_empty() && calendar.is_empty() {
                    return Err("custom OnCalendar expression is empty".to_string());
                }
                SchedulePreset::Custom {
                    calendar: if self.custom.value.trim().is_empty() {
                        calendar.to_string()
                    } else {
                        self.custom.value.trim().to_string()
                    },
                }
            }
            other => other.clone(),
        };
        Ok(Profile {
            name,
            steps: self.selected_steps.iter().cloned().collect(),
            schedule: Schedule {
                preset,
                randomized_delay_sec: self.schedule.randomized_delay_sec,
            },
            cleanup: self.cleanup,
            notify: self.notify,
            enabled: self.enabled,
            scope: Scope::User,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> EditorEvent {
        if key.code == KeyCode::Esc {
            return EditorEvent::Cancel;
        }
        if key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::BackTab {
            self.section = prev_section(self.section);
            self.reset_indices();
            return EditorEvent::None;
        }
        if key.code == KeyCode::Tab {
            self.section = next_section(self.section);
            self.reset_indices();
            return EditorEvent::None;
        }
        match self.section {
            Section::Name => self.handle_name_key(key),
            Section::Steps => self.handle_steps_key(key),
            Section::Schedule => self.handle_schedule_key(key),
            Section::Options => self.handle_options_key(key),
            Section::Actions => self.handle_actions_key(key),
        }
    }

    fn reset_indices(&mut self) {
        self.list_index = 0;
        self.schedule_index = 0;
        self.option_index = 0;
        self.action_index = 0;
    }

    fn handle_name_key(&mut self, key: KeyEvent) -> EditorEvent {
        if self.creating {
            self.name.handle_key(key);
        }
        EditorEvent::None
    }

    fn handle_steps_key(&mut self, key: KeyEvent) -> EditorEvent {
        match key.code {
            KeyCode::Up => self.list_index = self.list_index.saturating_sub(1),
            KeyCode::Down => {
                let len = self.filtered_steps().len();
                if self.list_index + 1 < len {
                    self.list_index += 1;
                }
            }
            KeyCode::Char(' ') => {
                if let Some(entry) = self.filtered_steps().get(self.list_index) {
                    let id = entry.id.clone();
                    if self.selected_steps.contains(&id) {
                        self.selected_steps.remove(&id);
                    } else {
                        self.selected_steps.insert(id);
                    }
                }
            }
            _ => self.filter.handle_key(key),
        }
        EditorEvent::None
    }

    fn handle_schedule_key(&mut self, key: KeyEvent) -> EditorEvent {
        let rows = self.schedule_rows();
        match key.code {
            KeyCode::Up => self.schedule_index = self.schedule_index.saturating_sub(1),
            KeyCode::Down => {
                if self.schedule_index + 1 < rows {
                    self.schedule_index += 1;
                }
            }
            _ => self.adjust_schedule(key),
        }
        EditorEvent::None
    }

    fn schedule_rows(&self) -> usize {
        2 + match self.schedule.preset.kind() {
            PresetKind::Hourly => 0,
            PresetKind::EveryNHours => 1,
            PresetKind::Daily => 2,
            PresetKind::Weekly => 3,
            PresetKind::Custom => 1,
        }
    }

    fn adjust_schedule(&mut self, key: KeyEvent) {
        let delta = match key.code {
            KeyCode::Left | KeyCode::Up => -1i64,
            KeyCode::Right | KeyCode::Down => 1,
            _ => {
                if matches!(self.schedule.preset.kind(), PresetKind::Custom)
                    && self.custom_row_selected()
                {
                    self.custom.handle_key(key);
                }
                return;
            }
        };
        let field = self.schedule_field();
        match field {
            ScheduleField::Preset => self.cycle_preset(delta),
            ScheduleField::Hours => {
                if let SchedulePreset::EveryNHours { hours } = &mut self.schedule.preset {
                    *hours = clamp((*hours as i64 + delta) as u32, 1, 23);
                }
            }
            ScheduleField::Hour => set_hour(&mut self.schedule.preset, delta),
            ScheduleField::Minute => set_minute(&mut self.schedule.preset, delta),
            ScheduleField::Weekday => cycle_weekday(&mut self.schedule.preset, delta),
            ScheduleField::Custom => {}
            ScheduleField::Delay => {
                self.schedule.randomized_delay_sec =
                    (self.schedule.randomized_delay_sec as i64 + delta * 60).max(0) as u64;
            }
        }
    }

    fn custom_row_selected(&self) -> bool {
        matches!(self.schedule_field(), ScheduleField::Custom)
    }

    fn schedule_field(&self) -> ScheduleField {
        let kind = self.schedule.preset.kind();
        let mut row = self.schedule_index;
        if row == 0 {
            return ScheduleField::Preset;
        }
        row -= 1;
        match kind {
            PresetKind::Hourly => ScheduleField::Delay,
            PresetKind::EveryNHours => {
                if row == 0 {
                    ScheduleField::Hours
                } else {
                    ScheduleField::Delay
                }
            }
            PresetKind::Daily => match row {
                0 => ScheduleField::Hour,
                1 => ScheduleField::Minute,
                _ => ScheduleField::Delay,
            },
            PresetKind::Weekly => match row {
                0 => ScheduleField::Weekday,
                1 => ScheduleField::Hour,
                2 => ScheduleField::Minute,
                _ => ScheduleField::Delay,
            },
            PresetKind::Custom => {
                if row == 0 {
                    ScheduleField::Custom
                } else {
                    ScheduleField::Delay
                }
            }
        }
    }

    fn cycle_preset(&mut self, delta: i64) {
        let current = self.schedule.preset.kind();
        let mut index = PRESET_ORDER
            .iter()
            .position(|kind| *kind == current)
            .unwrap_or(2) as i64;
        index = (index + delta).rem_euclid(PRESET_ORDER.len() as i64);
        let target = PRESET_ORDER[index as usize];
        let preset = &self.schedule.preset;
        self.schedule.preset = match target {
            PresetKind::Hourly => SchedulePreset::Hourly,
            PresetKind::EveryNHours => SchedulePreset::EveryNHours { hours: 6 },
            PresetKind::Daily => SchedulePreset::Daily {
                hour: preset.hour(),
                minute: preset.minute(),
            },
            PresetKind::Weekly => SchedulePreset::Weekly {
                weekday: preset.weekday(),
                hour: preset.hour(),
                minute: preset.minute(),
            },
            PresetKind::Custom => SchedulePreset::Custom {
                calendar: self.custom.value.clone(),
            },
        };
        self.schedule_index = 0;
    }

    fn handle_options_key(&mut self, key: KeyEvent) -> EditorEvent {
        match key.code {
            KeyCode::Up => self.option_index = self.option_index.saturating_sub(1),
            KeyCode::Down => self.option_index = (self.option_index + 1).min(2),
            KeyCode::Left | KeyCode::Right => {
                let delta = if key.code == KeyCode::Left { 2 } else { 1 };
                match self.option_index {
                    0 => self.cleanup = !self.cleanup,
                    1 => {
                        let index = (self.notify.index() + delta) % 3;
                        self.notify = NotifyPolicy::from_index(index);
                    }
                    _ => self.enabled = !self.enabled,
                }
            }
            KeyCode::Char(' ') => match self.option_index {
                0 => self.cleanup = !self.cleanup,
                2 => self.enabled = !self.enabled,
                _ => {}
            },
            _ => {}
        }
        EditorEvent::None
    }

    fn handle_actions_key(&mut self, key: KeyEvent) -> EditorEvent {
        match key.code {
            KeyCode::Left | KeyCode::Right => {
                self.action_index = 1 - self.action_index;
                EditorEvent::None
            }
            KeyCode::Enter => {
                if self.action_index == 0 {
                    EditorEvent::RequestSave
                } else {
                    EditorEvent::Cancel
                }
            }
            _ => EditorEvent::None,
        }
    }
}

enum ScheduleField {
    Preset,
    Hours,
    Hour,
    Minute,
    Weekday,
    Custom,
    Delay,
}

fn next_section(section: Section) -> Section {
    let index = SECTIONS.iter().position(|s| *s == section).unwrap_or(0);
    SECTIONS[(index + 1) % SECTIONS.len()]
}

fn prev_section(section: Section) -> Section {
    let index = SECTIONS.iter().position(|s| *s == section).unwrap_or(0);
    SECTIONS[(index + SECTIONS.len() - 1) % SECTIONS.len()]
}

fn clamp(value: u32, low: u32, high: u32) -> u32 {
    value.max(low).min(high)
}

fn set_hour(preset: &mut SchedulePreset, delta: i64) {
    match preset {
        SchedulePreset::Daily { hour, .. } | SchedulePreset::Weekly { hour, .. } => {
            *hour = ((*hour as i64 + delta).rem_euclid(24)) as u32;
        }
        _ => {}
    }
}

fn set_minute(preset: &mut SchedulePreset, delta: i64) {
    match preset {
        SchedulePreset::Daily { minute, .. } | SchedulePreset::Weekly { minute, .. } => {
            *minute = ((*minute as i64 + delta).rem_euclid(60)) as u32;
        }
        _ => {}
    }
}

fn cycle_weekday(preset: &mut SchedulePreset, delta: i64) {
    if let SchedulePreset::Weekly { weekday, .. } = preset {
        let index = Weekday::ALL
            .iter()
            .position(|day| *day == *weekday)
            .unwrap_or(0) as i64;
        *weekday = Weekday::ALL[((index + delta).rem_euclid(7)) as usize];
    }
}

pub fn validate_draft(profile: &Profile) -> Result<(), String> {
    if let SchedulePreset::Custom { calendar } = &profile.schedule.preset {
        validate_on_calendar(calendar).map_err(|error| format!("invalid OnCalendar: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::steps::catalog;

    const HELP: &str = include_str!("../../tests/fixtures/topgrade_help.txt");

    fn catalog_entries() -> Vec<StepEntry> {
        catalog(HELP)
    }

    fn new_editor() -> EditorState {
        EditorState::new(None, catalog_entries())
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn rejects_invalid_drafts() {
        let mut editor = new_editor();
        let error = editor.to_profile().unwrap_err();
        assert!(error.contains("profile name"));

        editor.name = LineEdit::new("flatpak-daily");
        let error = editor.to_profile().unwrap_err();
        assert!(error.contains("at least one step"));

        editor.selected_steps.insert("flatpak".to_string());
        editor.schedule.preset = SchedulePreset::Custom {
            calendar: String::new(),
        };
        editor.custom = LineEdit::new("  ");
        let error = editor.to_profile().unwrap_err();
        assert!(error.contains("OnCalendar"));
    }

    #[test]
    fn builds_valid_profile_from_draft() {
        let mut editor = new_editor();
        editor.name = LineEdit::new("flatpak-daily");
        editor.selected_steps.insert("flatpak".to_string());
        editor.selected_steps.insert("cargo".to_string());
        let profile = editor.to_profile().unwrap();
        assert_eq!(profile.name, "flatpak-daily");
        assert_eq!(profile.steps, vec!["cargo", "flatpak"]);
        assert!(profile.cleanup);
        assert_eq!(profile.notify, NotifyPolicy::OnFailure);
        assert!(profile.enabled);
        assert_eq!(profile.schedule.preset.on_calendar(), "*-*-* 12:00:00");
    }

    #[test]
    fn filter_matches_substring_and_prioritizes_curated() {
        let mut editor = new_editor();
        editor.filter = LineEdit::new("pa");
        let entries = editor.filtered_steps();
        let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
        assert!(ids.contains(&"flatpak".to_string()));
        assert!(ids.contains(&"bun_packages".to_string()));
        assert!(!ids.iter().any(|id| id == "cargo"));
        assert!(entries.first().unwrap().category.is_some());

        editor.filter = LineEdit::new("flatpak");
        assert_eq!(
            editor.filtered_steps().first().unwrap().id,
            "flatpak".to_string()
        );
    }

    #[test]
    fn preset_cycling_carries_time_fields() {
        let mut editor = new_editor();
        editor.schedule.preset = SchedulePreset::Daily {
            hour: 9,
            minute: 30,
        };
        editor.cycle_preset(1);
        match &editor.schedule.preset {
            SchedulePreset::Weekly {
                weekday,
                hour,
                minute,
            } => {
                assert_eq!(*weekday, Weekday::Mon);
                assert_eq!((*hour, *minute), (9, 30));
            }
            other => panic!("unexpected preset {other:?}"),
        }
    }

    #[test]
    fn toggling_steps_updates_selection() {
        let mut editor = new_editor();
        editor.section = Section::Steps;
        editor.filter = LineEdit::new("flatpak");
        editor.handle_key(key(KeyCode::Char(' ')));
        assert!(editor.selected_steps.contains("flatpak"));
        editor.handle_key(key(KeyCode::Char(' ')));
        assert!(!editor.selected_steps.contains("flatpak"));
    }
}
