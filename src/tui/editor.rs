use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::domain::profile::{NotifyPolicy, Profile, Scope, sanitize_name};
use crate::domain::schedule::{Schedule, SchedulePreset, SpreadPeriod, Weekday};
use crate::domain::steps::StepEntry;
use crate::systemd::validate_on_calendar;

use super::input::LineEdit;

pub const STEPS_VISIBLE: usize = 10;

pub fn window_bounds(
    index: usize,
    len: usize,
    height: usize,
    scroll: &mut usize,
) -> (usize, usize) {
    if height == 0 {
        return (0, 0);
    }
    if index < *scroll {
        *scroll = index;
    } else if index >= *scroll + height {
        *scroll = index + 1 - height;
    }
    *scroll = (*scroll).min(len.saturating_sub(height));
    let end = (*scroll + height).min(len);
    (*scroll, end)
}

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
    Spread,
    Custom,
}

const PRESET_ORDER: [PresetKind; 6] = [
    PresetKind::Hourly,
    PresetKind::EveryNHours,
    PresetKind::Daily,
    PresetKind::Weekly,
    PresetKind::Spread,
    PresetKind::Custom,
];

impl SchedulePreset {
    fn kind(&self) -> PresetKind {
        match self {
            SchedulePreset::Hourly => PresetKind::Hourly,
            SchedulePreset::EveryNHours { .. } => PresetKind::EveryNHours,
            SchedulePreset::Daily { .. } => PresetKind::Daily,
            SchedulePreset::Weekly { .. } => PresetKind::Weekly,
            SchedulePreset::Spread { .. } => PresetKind::Spread,
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
    pub steps_filtering: bool,
    pub steps_scroll: usize,
    pub custom: LineEdit,
    pub catalog: Vec<StepEntry>,
    pub selected_steps: BTreeSet<String>,
    pub schedule: Schedule,
    pub cleanup: bool,
    pub notify: NotifyPolicy,
    pub section: Section,
    pub list_index: usize,
    pub schedule_index: usize,
    pub option_index: usize,
    pub action_index: usize,
    pub typed_digits: String,
    pub preset_origin: Option<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditorEvent {
    None,
    Cancel,
    RequestSave,
    Quit,
}

impl EditorState {
    pub fn new(original: Option<&Profile>, catalog: Vec<StepEntry>) -> Self {
        match original {
            Some(profile) => Self {
                creating: false,
                name: LineEdit::new(profile.name.clone()),
                filter: LineEdit::new(String::new()),
                steps_filtering: false,
                steps_scroll: 0,
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
                section: Section::Steps,
                list_index: 0,
                schedule_index: 0,
                option_index: 0,
                action_index: 0,
                typed_digits: String::new(),
                preset_origin: None,
            },
            None => Self {
                creating: true,
                name: LineEdit::new(String::new()),
                filter: LineEdit::new(String::new()),
                steps_filtering: false,
                steps_scroll: 0,
                custom: LineEdit::new(String::new()),
                catalog,
                selected_steps: BTreeSet::new(),
                schedule: Schedule::default(),
                cleanup: true,
                notify: NotifyPolicy::OnFailure,
                section: Section::Steps,
                list_index: 0,
                schedule_index: 0,
                option_index: 0,
                action_index: 0,
                typed_digits: String::new(),
                preset_origin: None,
            },
        }
    }

    pub fn from_preset(
        preset_index: usize,
        catalog: Vec<StepEntry>,
        steps: Vec<String>,
        suggested_name: &str,
    ) -> Self {
        let mut editor = Self::new(None, catalog);
        editor.name = LineEdit::new(suggested_name.to_string());
        editor.selected_steps = steps.into_iter().collect();
        editor.preset_origin = Some(preset_index);
        editor
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

    pub fn steps_window(&self) -> (usize, usize) {
        let mut scroll = self.steps_scroll;
        window_bounds(
            self.list_index,
            self.filtered_steps().len(),
            STEPS_VISIBLE,
            &mut scroll,
        )
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
        let steps: Vec<String> = self.selected_steps.iter().cloned().collect();
        Ok(Profile {
            name,
            steps,
            schedule: Schedule {
                preset,
                randomized_delay_sec: self.schedule.randomized_delay_sec,
            },
            cleanup: self.cleanup,
            notify: self.notify,
            scope: Scope::User,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> EditorEvent {
        if key.code == KeyCode::Esc && !(self.section == Section::Steps && self.steps_filtering) {
            return EditorEvent::Cancel;
        }
        if matches!(key.code, KeyCode::Char('q' | 'Q')) && !self.text_entry_focused() {
            return EditorEvent::Quit;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return EditorEvent::RequestSave;
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

    fn text_entry_focused(&self) -> bool {
        match self.section {
            Section::Name => self.creating,
            Section::Steps => self.steps_filtering,
            Section::Schedule => self.custom_row_selected(),
            Section::Options | Section::Actions => false,
        }
    }

    fn handle_name_key(&mut self, key: KeyEvent) -> EditorEvent {
        if self.creating {
            if key.code == KeyCode::Enter {
                self.section = Section::Steps;
                self.reset_indices();
            } else {
                self.name.handle_key(key);
            }
        }
        EditorEvent::None
    }

    fn handle_steps_key(&mut self, key: KeyEvent) -> EditorEvent {
        if self.steps_filtering {
            match key.code {
                KeyCode::Enter => self.steps_filtering = false,
                KeyCode::Esc => {
                    self.steps_filtering = false;
                    self.filter = LineEdit::new(String::new());
                    self.list_index = 0;
                    self.steps_scroll = 0;
                }
                KeyCode::Up => self.move_steps_selection(-1),
                KeyCode::Down => self.move_steps_selection(1),
                _ => {
                    self.filter.handle_key(key);
                    self.list_index = 0;
                    self.steps_scroll = 0;
                }
            }
            return EditorEvent::None;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => self.move_steps_selection(-1),
            KeyCode::Down | KeyCode::Char('j' | 'J') => self.move_steps_selection(1),
            KeyCode::Char('/') => self.steps_filtering = true,
            KeyCode::Char(' ') => self.toggle_step_at(self.list_index),
            _ => {}
        }
        EditorEvent::None
    }

    fn move_steps_selection(&mut self, delta: i64) {
        let len = self.filtered_steps().len();
        if len == 0 {
            return;
        }
        let next = (self.list_index as i64 + delta).clamp(0, len as i64 - 1) as usize;
        self.list_index = next;
        let (start, _) = window_bounds(self.list_index, len, STEPS_VISIBLE, &mut self.steps_scroll);
        self.steps_scroll = start;
    }

    fn toggle_step_at(&mut self, index: usize) {
        if let Some(entry) = self.filtered_steps().get(index) {
            let id = entry.id.clone();
            if self.selected_steps.contains(&id) {
                self.selected_steps.remove(&id);
            } else {
                self.selected_steps.insert(id);
            }
        }
    }

    fn handle_schedule_key(&mut self, key: KeyEvent) -> EditorEvent {
        let rows = self.schedule_rows();
        let numeric = matches!(
            self.schedule_field(),
            ScheduleField::Hours | ScheduleField::Hour | ScheduleField::Minute
        );
        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() && numeric => {
                if self.typed_digits.len() >= 2 {
                    self.typed_digits.clear();
                }
                self.typed_digits.push(c);
                if self.typed_digits.len() == 2 {
                    self.commit_typed();
                }
            }
            KeyCode::Backspace if numeric && !self.typed_digits.is_empty() => {
                self.typed_digits.pop();
            }
            KeyCode::Up => {
                self.commit_typed();
                self.schedule_index = self.schedule_index.saturating_sub(1);
            }
            KeyCode::Down => {
                self.commit_typed();
                if self.schedule_index + 1 < rows {
                    self.schedule_index += 1;
                }
            }
            _ => {
                if self.typed_digits.is_empty() {
                    self.adjust_schedule(key);
                } else {
                    self.commit_typed();
                }
            }
        }
        EditorEvent::None
    }

    fn commit_typed(&mut self) {
        if self.typed_digits.is_empty() {
            return;
        }
        let value: u32 = self.typed_digits.parse().unwrap_or(0);
        match self.schedule_field() {
            ScheduleField::Hours => {
                if let SchedulePreset::EveryNHours { hours } = &mut self.schedule.preset {
                    *hours = clamp(value.max(1), 1, 23);
                }
            }
            ScheduleField::Hour => set_hour_value(&mut self.schedule.preset, value.min(23)),
            ScheduleField::Minute => set_minute_value(&mut self.schedule.preset, value.min(59)),
            _ => {}
        }
        self.typed_digits.clear();
    }

    pub fn typed_hint(&self) -> String {
        if self.typed_digits.is_empty() {
            return String::new();
        }
        match self.schedule_field() {
            ScheduleField::Hours | ScheduleField::Hour | ScheduleField::Minute => {
                format!(" [{}…]", self.typed_digits)
            }
            _ => String::new(),
        }
    }

    fn schedule_rows(&self) -> usize {
        2 + match self.schedule.preset.kind() {
            PresetKind::Hourly => 0,
            PresetKind::EveryNHours => 1,
            PresetKind::Daily => 2,
            PresetKind::Weekly => 3,
            PresetKind::Spread => 1,
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
            ScheduleField::SpreadPeriod => cycle_spread_period(&mut self.schedule.preset, delta),
            ScheduleField::Custom => {}
            ScheduleField::Delay => {
                let step = if self.schedule.randomized_delay_sec >= 3600 {
                    3600
                } else {
                    60
                };
                self.schedule.randomized_delay_sec =
                    (self.schedule.randomized_delay_sec as i64 + delta * step as i64).max(0) as u64;
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
            PresetKind::Spread => {
                if row == 0 {
                    ScheduleField::SpreadPeriod
                } else {
                    ScheduleField::Delay
                }
            }
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
            PresetKind::Spread => SchedulePreset::Spread {
                period: SpreadPeriod::Daily,
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
            KeyCode::Down => self.option_index = (self.option_index + 1).min(1),
            KeyCode::Left | KeyCode::Right => {
                let delta = if key.code == KeyCode::Left { 2 } else { 1 };
                match self.option_index {
                    0 => self.cleanup = !self.cleanup,
                    _ => {
                        let index = (self.notify.index() + delta) % 3;
                        self.notify = NotifyPolicy::from_index(index);
                    }
                }
            }
            KeyCode::Char(' ') if self.option_index == 0 => {
                self.cleanup = !self.cleanup;
            }
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
    SpreadPeriod,
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

fn set_hour_value(preset: &mut SchedulePreset, value: u32) {
    match preset {
        SchedulePreset::Daily { hour, .. } | SchedulePreset::Weekly { hour, .. } => {
            *hour = value.min(23);
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

fn set_minute_value(preset: &mut SchedulePreset, value: u32) {
    match preset {
        SchedulePreset::Daily { minute, .. } | SchedulePreset::Weekly { minute, .. } => {
            *minute = value.min(59);
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

fn cycle_spread_period(preset: &mut SchedulePreset, delta: i64) {
    if let SchedulePreset::Spread { period } = preset {
        *period = match (*period, delta < 0) {
            (SpreadPeriod::Daily, false) => SpreadPeriod::Weekly,
            (SpreadPeriod::Weekly, true) => SpreadPeriod::Daily,
            (current, _) => current,
        };
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
        assert_eq!(profile.schedule.preset.on_calendar(), "daily");
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
    fn spread_period_toggles_between_daily_and_weekly() {
        let mut editor = new_editor();
        editor.schedule.preset = SchedulePreset::Spread {
            period: SpreadPeriod::Daily,
        };
        cycle_spread_period(&mut editor.schedule.preset, 1);
        assert_eq!(
            editor.schedule.preset,
            SchedulePreset::Spread {
                period: SpreadPeriod::Weekly
            }
        );
        cycle_spread_period(&mut editor.schedule.preset, -1);
        assert_eq!(
            editor.schedule.preset,
            SchedulePreset::Spread {
                period: SpreadPeriod::Daily
            }
        );
    }

    #[test]
    fn delay_adjusts_in_hour_steps_when_large() {
        let mut editor = new_editor();
        editor.schedule.randomized_delay_sec = 43_200;
        editor.section = Section::Schedule;
        editor.schedule_index = editor.schedule_rows() - 1;
        editor.adjust_schedule(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(editor.schedule.randomized_delay_sec, 39_600);
        editor.schedule.randomized_delay_sec = 900;
        editor.adjust_schedule(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(editor.schedule.randomized_delay_sec, 960);
    }

    #[test]
    fn typed_digits_set_time_fields() {
        let mut editor = new_editor();
        editor.schedule.preset = SchedulePreset::Daily {
            hour: 12,
            minute: 0,
        };
        editor.section = Section::Schedule;
        editor.schedule_index = 1;

        editor.handle_key(key(KeyCode::Char('0')));
        editor.handle_key(key(KeyCode::Char('9')));
        match &editor.schedule.preset {
            SchedulePreset::Daily { hour, .. } => assert_eq!(*hour, 9),
            other => panic!("unexpected preset {other:?}"),
        }
        assert!(editor.typed_digits.is_empty(), "two digits auto-commit");

        editor.handle_key(key(KeyCode::Char('4')));
        editor.handle_key(key(KeyCode::Down));
        match &editor.schedule.preset {
            SchedulePreset::Daily { hour, .. } => assert_eq!(*hour, 4),
            other => panic!("unexpected preset {other:?}"),
        }

        editor.handle_key(key(KeyCode::Up));
        editor.handle_key(key(KeyCode::Char('9')));
        editor.handle_key(key(KeyCode::Backspace));
        editor.handle_key(key(KeyCode::Char('7')));
        editor.handle_key(key(KeyCode::Right));
        match &editor.schedule.preset {
            SchedulePreset::Daily { hour, .. } => assert_eq!(*hour, 7),
            other => panic!("unexpected preset {other:?}"),
        }
    }

    #[test]
    fn typed_digits_clamp_out_of_range_values() {
        let mut editor = new_editor();
        editor.schedule.preset = SchedulePreset::Daily {
            hour: 12,
            minute: 0,
        };
        editor.section = Section::Schedule;
        editor.schedule_index = 1;
        editor.handle_key(key(KeyCode::Char('9')));
        editor.handle_key(key(KeyCode::Char('9')));
        match &editor.schedule.preset {
            SchedulePreset::Daily { hour, .. } => assert_eq!(*hour, 23),
            other => panic!("unexpected preset {other:?}"),
        }
    }

    #[test]
    fn quit_works_outside_text_entry_and_types_inside_it() {
        let mut editor = new_editor();
        editor.section = Section::Actions;
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::Quit
        );
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('Q'))),
            EditorEvent::Quit
        );

        editor.section = Section::Steps;
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::Quit,
            "q quits while browsing steps (filter is opt-in)"
        );
        editor.handle_key(key(KeyCode::Char('/')));
        assert!(editor.steps_filtering);
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::None
        );
        assert_eq!(editor.filter.value, "q");

        editor.section = Section::Schedule;
        editor.schedule.preset = SchedulePreset::Custom {
            calendar: String::new(),
        };
        editor.schedule_index = 1;
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::None
        );
        assert_eq!(editor.custom.value, "q");
    }

    #[test]
    fn steps_filter_is_opt_in_and_esc_clears() {
        let mut editor = new_editor();
        editor.section = Section::Steps;

        editor.handle_key(key(KeyCode::Char('x')));
        assert_eq!(
            editor.filter.value, "",
            "chars must not leak into the filter"
        );

        editor.handle_key(key(KeyCode::Char('/')));
        editor.handle_key(key(KeyCode::Char('c')));
        editor.handle_key(key(KeyCode::Char('a')));
        assert_eq!(editor.filter.value, "ca");
        assert!(editor.steps_filtering);

        editor.handle_key(key(KeyCode::Enter));
        assert!(!editor.steps_filtering);
        assert_eq!(editor.filter.value, "ca", "Enter keeps the filter applied");

        editor.handle_key(key(KeyCode::Char('/')));
        editor.handle_key(key(KeyCode::Esc));
        assert!(!editor.steps_filtering);
        assert_eq!(editor.filter.value, "", "Esc clears the filter");
    }

    #[test]
    fn steps_selection_scrolls_beyond_the_visible_window() {
        let mut editor = new_editor();
        editor.section = Section::Steps;
        let total = editor.filtered_steps().len();
        assert!(total > STEPS_VISIBLE, "catalog must exceed the window");

        for _ in 0..total + 5 {
            editor.handle_key(key(KeyCode::Down));
        }
        assert_eq!(editor.list_index, total - 1, "selection clamps at the end");
        let (start, end) = editor.steps_window();
        assert!(editor.list_index >= start && editor.list_index < end);

        for _ in 0..total + 5 {
            editor.handle_key(key(KeyCode::Up));
        }
        assert_eq!(editor.list_index, 0);
        assert_eq!(editor.steps_scroll, 0);
    }

    #[test]
    fn j_and_k_navigate_steps_without_the_filter() {
        let mut editor = new_editor();
        editor.section = Section::Steps;
        editor.handle_key(key(KeyCode::Char('j')));
        assert_eq!(editor.list_index, 1);
        editor.handle_key(key(KeyCode::Char('k')));
        assert_eq!(editor.list_index, 0);
    }

    #[test]
    fn ctrl_s_requests_save_from_any_section() {
        for section in [
            Section::Name,
            Section::Steps,
            Section::Schedule,
            Section::Options,
            Section::Actions,
        ] {
            let mut editor = new_editor();
            editor.section = section;
            let event = editor.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
            assert_eq!(event, EditorEvent::RequestSave, "section {section:?}");
        }

        let mut editor = new_editor();
        editor.section = Section::Steps;
        editor.handle_key(key(KeyCode::Char('s')));
        assert_eq!(editor.filter.value, "", "plain s must not save");
    }

    #[test]
    fn creating_a_profile_lands_on_the_steps_picker() {
        let editor = new_editor();
        assert_eq!(editor.section, Section::Steps);

        let mut editor = new_editor();
        editor.name = LineEdit::new("dev-daily");
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.section, Section::Steps);
        assert_eq!(editor.name.value, "dev-daily");
    }

    #[test]
    fn from_preset_prefills_name_steps_and_default_schedule() {
        let entries = catalog_entries();
        let steps = vec!["flatpak".to_string()];
        let editor = EditorState::from_preset(2, entries, steps, "flatpak-daily");
        assert!(editor.creating);
        assert_eq!(editor.name.value, "flatpak-daily");
        assert_eq!(editor.section, Section::Steps);
        assert!(editor.selected_steps.contains("flatpak"));
        assert_eq!(
            editor.schedule.preset,
            crate::domain::schedule::SchedulePreset::Spread {
                period: crate::domain::schedule::SpreadPeriod::Daily
            }
        );
        assert_eq!(editor.preset_origin, Some(2));
        let profile = editor.to_profile().unwrap();
        assert_eq!(profile.name, "flatpak-daily");
        assert_eq!(profile.steps, vec!["flatpak"]);
    }

    #[test]
    fn enter_in_the_name_field_moves_to_steps() {
        let mut editor = new_editor();
        editor.section = Section::Name;
        editor.handle_key(key(KeyCode::Char('d')));
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.section, Section::Steps);
        assert_eq!(editor.list_index, 0);
    }

    #[test]
    fn window_bounds_follow_the_selection() {
        let mut scroll = 0usize;
        assert_eq!(window_bounds(0, 100, 10, &mut scroll), (0, 10));
        assert_eq!(window_bounds(9, 100, 10, &mut scroll), (0, 10));
        assert_eq!(window_bounds(10, 100, 10, &mut scroll), (1, 11));
        assert_eq!(window_bounds(99, 100, 10, &mut scroll), (90, 100));
        assert_eq!(window_bounds(0, 100, 10, &mut scroll), (0, 10));
        let mut tiny = 5usize;
        assert_eq!(window_bounds(3, 4, 10, &mut tiny), (0, 4));
        let mut zero = 0usize;
        assert_eq!(window_bounds(2, 8, 0, &mut zero), (0, 0));
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
