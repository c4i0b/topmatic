use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::domain::profile::{NotifyPolicy, Profile, Scope, sanitize_name};
use crate::domain::schedule::{Schedule, SchedulePreset, Weekday, quick_choices};
use crate::systemd::validate_on_calendar;

use super::input::{FilterState, LineEdit};

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
    Steps,
    Schedule,
    Options,
}

const SECTIONS: [Section; 3] = [Section::Steps, Section::Schedule, Section::Options];

pub struct EditorState {
    pub creating: bool,
    pub original_name: Option<String>,
    pub suggested_name: Option<String>,
    pub name_popup: Option<LineEdit>,
    pub steps_filter: FilterState,
    pub steps_scroll: usize,
    pub custom: LineEdit,
    pub catalog: Vec<String>,
    pub selected_steps: BTreeSet<String>,
    pub schedule: Schedule,
    pub cleanup: bool,
    pub notify: NotifyPolicy,
    pub section: Section,
    pub list_index: usize,
    pub schedule_index: usize,
    pub option_index: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditorEvent {
    None,
    Cancel,
    RequestSave,
    Quit,
}

impl EditorState {
    pub fn new(original: Option<&Profile>, catalog: Vec<String>) -> Self {
        match original {
            Some(profile) => Self {
                creating: false,
                original_name: Some(profile.name.clone()),
                suggested_name: None,
                name_popup: None,
                steps_filter: FilterState::new(),
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
            },
            None => Self {
                creating: true,
                original_name: None,
                suggested_name: None,
                name_popup: None,
                steps_filter: FilterState::new(),
                steps_scroll: 0,
                custom: LineEdit::new(String::new()),
                catalog,
                selected_steps: BTreeSet::new(),
                schedule: quick_choices()[0].1.clone(),
                cleanup: true,
                notify: NotifyPolicy::OnFailure,
                section: Section::Steps,
                list_index: 0,
                schedule_index: 0,
                option_index: 0,
            },
        }
    }

    pub fn from_preset(catalog: Vec<String>, steps: Vec<String>, suggested_name: &str) -> Self {
        let mut editor = Self::new(None, catalog);
        editor.selected_steps = steps.into_iter().collect();
        editor.suggested_name = Some(suggested_name.to_string());
        editor
    }

    pub fn filtered_steps(&self) -> Vec<&String> {
        self.catalog
            .iter()
            .filter(|id| self.steps_filter.matches(id))
            .collect()
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

    pub fn final_name(&self) -> String {
        self.name_popup
            .as_ref()
            .map(|edit| edit.value.trim().to_string())
            .unwrap_or_default()
    }

    pub fn to_profile(&self, name: &str) -> Result<Profile, String> {
        let name = sanitize_name(name).map_err(|error| error.to_string())?;
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
        if self.name_popup.is_some() {
            return self.handle_popup_key(key);
        }
        if key.code == KeyCode::Esc && !self.steps_filter_active() {
            return EditorEvent::Cancel;
        }
        if matches!(key.code, KeyCode::Char('q' | 'Q')) && !self.text_entry_focused() {
            return EditorEvent::Quit;
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
        if key.code == KeyCode::Enter && !self.steps_filter_active() {
            self.open_name_popup();
            return EditorEvent::None;
        }
        match self.section {
            Section::Steps => self.handle_steps_key(key),
            Section::Schedule => self.handle_schedule_key(key),
            Section::Options => self.handle_options_key(key),
        }
    }

    fn handle_popup_key(&mut self, key: KeyEvent) -> EditorEvent {
        match key.code {
            KeyCode::Esc => {
                self.name_popup = None;
                EditorEvent::None
            }
            KeyCode::Enter => {
                if sanitize_name(&self.final_name()).is_ok() {
                    EditorEvent::RequestSave
                } else {
                    EditorEvent::None
                }
            }
            _ => {
                if let Some(popup) = self.name_popup.as_mut() {
                    popup.handle_key(key);
                }
                EditorEvent::None
            }
        }
    }

    fn open_name_popup(&mut self) {
        let prefill = if self.creating {
            self.suggested_name.clone().unwrap_or_default()
        } else {
            self.original_name.clone().unwrap_or_default()
        };
        self.name_popup = Some(LineEdit::new(prefill));
    }

    fn reset_indices(&mut self) {
        self.list_index = 0;
        self.schedule_index = 0;
        self.option_index = 0;
    }

    fn steps_filter_active(&self) -> bool {
        self.section == Section::Steps && self.steps_filter.active
    }

    fn text_entry_focused(&self) -> bool {
        match self.section {
            Section::Steps => self.steps_filter.active,
            Section::Schedule => matches!(self.schedule_field(), ScheduleField::Custom),
            Section::Options => false,
        }
    }

    fn handle_steps_key(&mut self, key: KeyEvent) -> EditorEvent {
        if self.steps_filter.active {
            match key.code {
                KeyCode::Up => self.move_steps_selection(-1),
                KeyCode::Down => self.move_steps_selection(1),
                _ => {
                    self.steps_filter.handle(key);
                    self.list_index = 0;
                    self.steps_scroll = 0;
                }
            }
            return EditorEvent::None;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => self.move_steps_selection(-1),
            KeyCode::Down | KeyCode::Char('j' | 'J') => self.move_steps_selection(1),
            KeyCode::Char('/') => self.steps_filter.start(),
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
        if let Some(id) = self.filtered_steps().get(index) {
            let id = id.to_string();
            if self.selected_steps.contains(&id) {
                self.selected_steps.remove(&id);
            } else {
                self.selected_steps.insert(id);
            }
        }
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
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                let delta = if key.code == KeyCode::Left { -1 } else { 1 };
                self.adjust_schedule(delta);
            }
            _ => {
                if matches!(self.schedule_field(), ScheduleField::Custom) {
                    self.custom.handle_key(key);
                }
            }
        }
        EditorEvent::None
    }

    fn adjust_schedule(&mut self, delta: i64) {
        match self.schedule_field() {
            ScheduleField::Choice(index) => {
                let (_, choice) = quick_choices()[index].clone();
                self.schedule = choice;
            }
            ScheduleField::Custom => {}
            ScheduleField::Hours => {
                if let SchedulePreset::EveryNHours { hours } = &mut self.schedule.preset {
                    *hours = ((*hours as i64 + delta).clamp(1, 23)) as u32;
                }
            }
            ScheduleField::Hour => {
                if let SchedulePreset::Daily { hour, .. } | SchedulePreset::Weekly { hour, .. } =
                    &mut self.schedule.preset
                {
                    *hour = ((*hour as i64 + delta).rem_euclid(24)) as u32;
                }
            }
            ScheduleField::Minute => {
                if let SchedulePreset::Daily { minute, .. }
                | SchedulePreset::Weekly { minute, .. } = &mut self.schedule.preset
                {
                    *minute = ((*minute as i64 + delta * 5).rem_euclid(60)) as u32;
                }
            }
            ScheduleField::Weekday => {
                if let SchedulePreset::Weekly { weekday, .. } = &mut self.schedule.preset {
                    let index = Weekday::ALL
                        .iter()
                        .position(|day| *day == *weekday)
                        .unwrap_or(0) as i64;
                    *weekday = Weekday::ALL[((index + delta).rem_euclid(7)) as usize];
                }
            }
            ScheduleField::Jitter => {
                self.schedule.randomized_delay_sec =
                    (self.schedule.randomized_delay_sec as i64 + delta * 300).max(0) as u64;
            }
        }
    }

    fn schedule_rows(&self) -> usize {
        let choices = quick_choices().len();
        let contextual = match &self.schedule.preset {
            SchedulePreset::EveryNHours { .. } => 1,
            SchedulePreset::Daily { .. } => 2,
            SchedulePreset::Weekly { .. } => 3,
            SchedulePreset::Custom { .. } => 0,
            SchedulePreset::LegacySpread { .. } => 0,
        };
        choices + 1 + contextual + 1
    }

    fn schedule_field(&self) -> ScheduleField {
        let choices = quick_choices().len();
        let index = self.schedule_index;
        if index < choices {
            return ScheduleField::Choice(index);
        }
        if index == choices {
            return ScheduleField::Custom;
        }
        let contextual = self.schedule_rows() - choices - 2;
        let row = index - choices - 1;
        if row >= contextual {
            return ScheduleField::Jitter;
        }
        match &self.schedule.preset {
            SchedulePreset::EveryNHours { .. } => ScheduleField::Hours,
            SchedulePreset::Daily { .. } => match row {
                0 => ScheduleField::Hour,
                _ => ScheduleField::Minute,
            },
            SchedulePreset::Weekly { .. } => match row {
                0 => ScheduleField::Weekday,
                1 => ScheduleField::Hour,
                _ => ScheduleField::Minute,
            },
            _ => ScheduleField::Jitter,
        }
    }

    fn handle_options_key(&mut self, key: KeyEvent) -> EditorEvent {
        match key.code {
            KeyCode::Up => self.option_index = self.option_index.saturating_sub(1),
            KeyCode::Down => self.option_index = (self.option_index + 1).min(1),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                let forward = key.code != KeyCode::Left;
                match self.option_index {
                    0 => self.cleanup = forward || !self.cleanup,
                    _ => {
                        let delta = if forward { 1 } else { 2 };
                        let index = (self.notify.index() + delta) % 3;
                        self.notify = NotifyPolicy::from_index(index);
                    }
                }
            }
            _ => {}
        }
        EditorEvent::None
    }
}

enum ScheduleField {
    Choice(usize),
    Custom,
    Hours,
    Hour,
    Minute,
    Weekday,
    Jitter,
}

fn next_section(section: Section) -> Section {
    let index = SECTIONS.iter().position(|s| *s == section).unwrap_or(0);
    SECTIONS[(index + 1) % SECTIONS.len()]
}

fn prev_section(section: Section) -> Section {
    let index = SECTIONS.iter().position(|s| *s == section).unwrap_or(0);
    SECTIONS[(index + SECTIONS.len() - 1) % SECTIONS.len()]
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
    use crate::domain::schedule::matches_quick_choice;
    use crate::domain::steps::catalog;

    const HELP: &str = include_str!("../../tests/fixtures/topgrade_help.txt");

    fn catalog_entries() -> Vec<String> {
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
        let error = editor.to_profile("bad name").unwrap_err();
        assert!(error.contains("profile name"));

        let error = editor.to_profile("flatpak-daily").unwrap_err();
        assert!(error.contains("at least one step"));

        editor.selected_steps.insert("flatpak".to_string());
        editor.schedule.preset = SchedulePreset::Custom {
            calendar: String::new(),
        };
        editor.custom = LineEdit::new("  ");
        let error = editor.to_profile("flatpak-daily").unwrap_err();
        assert!(error.contains("OnCalendar"));
    }

    #[test]
    fn builds_valid_profile_from_draft() {
        let mut editor = new_editor();
        editor.selected_steps.insert("flatpak".to_string());
        editor.selected_steps.insert("cargo".to_string());
        let profile = editor.to_profile("flatpak-daily").unwrap();
        assert_eq!(profile.name, "flatpak-daily");
        assert_eq!(profile.steps, vec!["cargo", "flatpak"]);
        assert!(profile.cleanup);
        assert_eq!(profile.notify, NotifyPolicy::OnFailure);
        assert_eq!(profile.schedule.preset.on_calendar(), "*-*-* 00:00:00");
        assert_eq!(profile.schedule.randomized_delay_sec, 1_800);
    }

    #[test]
    fn new_editor_defaults_to_first_quick_choice() {
        let editor = new_editor();
        assert_eq!(matches_quick_choice(&editor.schedule), Some(0));
    }

    #[test]
    fn selecting_a_quick_choice_applies_it() {
        let mut editor = new_editor();
        editor.section = Section::Schedule;
        editor.schedule_index = 3;
        editor.handle_key(key(KeyCode::Right));
        assert!(matches!(
            editor.schedule.preset,
            SchedulePreset::EveryNHours { .. }
        ));
        assert_eq!(matches_quick_choice(&editor.schedule), Some(3));

        editor.schedule_index = 2;
        editor.handle_key(key(KeyCode::Char(' ')));
        assert!(matches!(
            editor.schedule.preset,
            SchedulePreset::Weekly { .. }
        ));
    }

    #[test]
    fn contextual_rows_adjust_values() {
        let mut editor = new_editor();
        editor.section = Section::Schedule;
        editor.schedule_index = 1;
        editor.handle_key(key(KeyCode::Right));
        match &editor.schedule.preset {
            SchedulePreset::Daily { hour, .. } => assert_eq!(*hour, 12),
            other => panic!("unexpected preset {other:?}"),
        }

        let hour_row = quick_choices().len() + 1;
        editor.schedule_index = hour_row;
        editor.handle_key(key(KeyCode::Left));
        match &editor.schedule.preset {
            SchedulePreset::Daily { hour, .. } => assert_eq!(*hour, 11),
            other => panic!("unexpected preset {other:?}"),
        }

        editor.schedule_index = hour_row + 1;
        editor.handle_key(key(KeyCode::Right));
        match &editor.schedule.preset {
            SchedulePreset::Daily { minute, .. } => assert_eq!(*minute, 5),
            other => panic!("unexpected preset {other:?}"),
        }
    }

    #[test]
    fn jitter_row_adjusts_in_five_minute_steps() {
        let mut editor = new_editor();
        editor.section = Section::Schedule;
        editor.schedule_index = editor.schedule_rows() - 1;
        editor.handle_key(key(KeyCode::Right));
        assert_eq!(editor.schedule.randomized_delay_sec, 2_100);
        editor.handle_key(key(KeyCode::Left));
        editor.handle_key(key(KeyCode::Left));
        assert_eq!(editor.schedule.randomized_delay_sec, 1_500);
    }

    #[test]
    fn options_cycle_explicit_choices() {
        let mut editor = new_editor();
        editor.section = Section::Options;
        editor.handle_key(key(KeyCode::Left));
        assert!(!editor.cleanup, "left on cleanup turns it off");
        editor.handle_key(key(KeyCode::Right));
        assert!(editor.cleanup);

        editor.option_index = 1;
        editor.handle_key(key(KeyCode::Right));
        assert_eq!(editor.notify, NotifyPolicy::Never);
        editor.handle_key(key(KeyCode::Right));
        assert_eq!(editor.notify, NotifyPolicy::Always);
        editor.handle_key(key(KeyCode::Right));
        assert_eq!(editor.notify, NotifyPolicy::OnFailure);
    }

    #[test]
    fn filter_is_opt_in_and_shared_component() {
        let mut editor = new_editor();
        editor.section = Section::Steps;

        editor.handle_key(key(KeyCode::Char('x')));
        assert_eq!(editor.steps_filter.text(), "");

        editor.handle_key(key(KeyCode::Char('/')));
        editor.handle_key(key(KeyCode::Char('c')));
        editor.handle_key(key(KeyCode::Char('a')));
        assert_eq!(editor.steps_filter.text(), "ca");
        assert!(editor.steps_filter.active);

        editor.handle_key(key(KeyCode::Enter));
        assert!(!editor.steps_filter.active);
        assert_eq!(editor.steps_filter.text(), "ca");
        assert!(
            editor.name_popup.is_none(),
            "Enter while filtering must not open the popup"
        );

        editor.handle_key(key(KeyCode::Char('/')));
        editor.handle_key(key(KeyCode::Esc));
        assert_eq!(editor.steps_filter.text(), "");
    }

    #[test]
    fn quit_works_outside_text_entry_and_types_inside_it() {
        let mut editor = new_editor();
        editor.section = Section::Options;
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::Quit
        );

        editor.section = Section::Steps;
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::Quit,
            "q quits while browsing steps (filter is opt-in)"
        );
        editor.handle_key(key(KeyCode::Char('/')));
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::None
        );
        assert_eq!(editor.steps_filter.text(), "q");
    }

    #[test]
    fn enter_opens_name_popup_and_enter_again_requests_save() {
        let mut editor = new_editor();
        editor.selected_steps.insert("flatpak".to_string());
        editor.handle_key(key(KeyCode::Enter));
        assert!(editor.name_popup.is_some());
        assert_eq!(editor.final_name(), "");

        editor.handle_key(key(KeyCode::Char('f')));
        editor.handle_key(key(KeyCode::Char('l')));
        assert_eq!(
            editor.handle_key(key(KeyCode::Enter)),
            EditorEvent::RequestSave
        );
    }

    #[test]
    fn popup_rejects_invalid_names_by_staying_open() {
        let mut editor = new_editor();
        editor.selected_steps.insert("flatpak".to_string());
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(editor.handle_key(key(KeyCode::Enter)), EditorEvent::None);
        assert!(editor.name_popup.is_some());
    }

    #[test]
    fn editing_prefills_popup_with_current_name() {
        let profile = Profile {
            name: "all-user-daily".to_string(),
            steps: vec!["flatpak".to_string()],
            schedule: Schedule::default(),
            cleanup: true,
            notify: NotifyPolicy::OnFailure,
            scope: Scope::User,
        };
        let mut editor = EditorState::new(Some(&profile), catalog_entries());
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.final_name(), "all-user-daily");
        assert_eq!(
            editor.handle_key(key(KeyCode::Enter)),
            EditorEvent::RequestSave
        );
    }

    #[test]
    fn from_preset_prefills_popup_with_suggested_name() {
        let mut editor = EditorState::from_preset(
            catalog_entries(),
            vec!["flatpak".to_string()],
            "flatpak-daily",
        );
        assert!(editor.creating);
        assert!(editor.selected_steps.contains("flatpak"));
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.final_name(), "flatpak-daily");
        let profile = editor.to_profile(&editor.final_name()).unwrap();
        assert_eq!(profile.name, "flatpak-daily");
        assert_eq!(profile.steps, vec!["flatpak"]);
    }

    #[test]
    fn steps_selection_scrolls_beyond_the_visible_window() {
        let mut editor = new_editor();
        editor.section = Section::Steps;
        let total = editor.filtered_steps().len();
        assert!(total > STEPS_VISIBLE);

        for _ in 0..total + 5 {
            editor.handle_key(key(KeyCode::Down));
        }
        assert_eq!(editor.list_index, total - 1);
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
        editor.steps_filter.edit = LineEdit::new("flatpak");
        editor.handle_key(key(KeyCode::Char(' ')));
        assert!(editor.selected_steps.contains("flatpak"));
        editor.handle_key(key(KeyCode::Char(' ')));
        assert!(!editor.selected_steps.contains("flatpak"));
    }
}
