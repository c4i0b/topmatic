use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::domain::profile::{NotifyPolicy, Profile, Scope, sanitize_name};
use crate::domain::schedule::{Schedule, SchedulePreset, Weekday, quick_choices};

use crate::systemd::validate_on_calendar;

use super::input::{FilterState, LineEdit};
use popup::{RowEditor, RowEditorEvent, SelectTarget};

pub mod popup;
pub(crate) mod render;

pub const STEPS_ROWS: usize = 10;

pub fn grid_window(
    index: usize,
    len: usize,
    columns: usize,
    rows: usize,
    scroll: &mut usize,
) -> (usize, usize) {
    let columns = columns.max(1);
    if rows == 0 || len == 0 {
        return (0, 0);
    }
    let total_rows = len.div_ceil(columns);
    let cursor_row = index / columns;
    if cursor_row < *scroll {
        *scroll = cursor_row;
    } else if cursor_row >= *scroll + rows {
        *scroll = cursor_row + 1 - rows;
    }
    *scroll = (*scroll).min(total_rows.saturating_sub(rows));
    let start = *scroll * columns;
    (start, (start + rows * columns).min(len))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Steps,
    Schedule,
    Options,
    Save,
}

const SECTIONS: [Section; 4] = [
    Section::Steps,
    Section::Schedule,
    Section::Options,
    Section::Save,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScheduleRow {
    Preset,
    Weekday,
}

pub struct EditorState {
    pub creating: bool,
    pub original: Option<Profile>,
    pub original_name: Option<String>,
    pub suggested_name: Option<String>,
    pub name_popup: Option<LineEdit>,
    pub confirmed_name: Option<String>,
    pub row_editor: Option<RowEditor>,
    pub steps_filter: FilterState,
    pub steps_scroll: usize,
    pub steps_columns: usize,
    pub catalog: Vec<String>,
    pub selected_steps: BTreeSet<String>,
    pub schedule: Schedule,
    pub notify: NotifyPolicy,
    pub section: Section,
    pub list_index: usize,
    pub schedule_index: usize,
    pub jitter_secs: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditorEvent {
    None,
    Cancel,
    RequestSave,
    Quit,
    Help,
}

impl EditorState {
    pub fn new(original: Option<&Profile>, catalog: Vec<String>, jitter_secs: u64) -> Self {
        let base_schedule = quick_choices(jitter_secs)[0].1.clone();
        match original {
            Some(profile) => {
                let mut schedule = profile.schedule.clone();
                schedule.preset = schedule.preset.clone().normalized();
                let mut original = profile.clone();
                original.schedule = schedule.clone();
                Self {
                    creating: false,
                    original: Some(original),
                    original_name: Some(profile.name.clone()),
                    suggested_name: None,
                    name_popup: None,
                    confirmed_name: None,
                    row_editor: None,
                    steps_filter: FilterState::new(),
                    steps_scroll: 0,
                    steps_columns: 1,
                    catalog,
                    selected_steps: profile.steps.iter().cloned().collect(),
                    schedule,
                    notify: profile.notify,
                    section: Section::Steps,
                    list_index: 0,
                    schedule_index: 0,
                    jitter_secs,
                }
            }
            None => Self {
                creating: true,
                original: None,
                original_name: None,
                suggested_name: None,
                name_popup: None,
                confirmed_name: None,
                row_editor: None,
                steps_filter: FilterState::new(),
                steps_scroll: 0,
                steps_columns: 1,
                catalog,
                selected_steps: BTreeSet::new(),
                schedule: base_schedule,
                notify: NotifyPolicy::OnFailure,
                section: Section::Steps,
                list_index: 0,
                schedule_index: 0,
                jitter_secs,
            },
        }
    }

    pub fn from_preset(
        catalog: Vec<String>,
        steps: Vec<String>,
        suggested_name: &str,
        jitter_secs: u64,
    ) -> Self {
        let mut editor = Self::new(None, catalog, jitter_secs);
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
        grid_window(
            self.list_index,
            self.filtered_steps().len(),
            self.steps_columns,
            STEPS_ROWS,
            &mut scroll,
        )
    }

    pub fn set_steps_columns(&mut self, columns: usize) {
        self.steps_columns = columns.max(1);
    }

    pub fn widest_step(&self) -> usize {
        self.filtered_steps()
            .iter()
            .map(|step| step.chars().count())
            .max()
            .unwrap_or(0)
    }

    pub fn steps_columns_for_width(&self, width: u16) -> usize {
        let cell = self.widest_step() + 9;
        ((width as usize) / cell).clamp(1, 6)
    }

    pub fn final_name(&self) -> String {
        if let Some(edit) = &self.name_popup {
            return edit.value.trim().to_string();
        }
        self.confirmed_name.clone().unwrap_or_default()
    }

    pub fn row_editor(&self) -> Option<&RowEditor> {
        self.row_editor.as_ref()
    }

    fn schedule_rows(&self) -> Vec<ScheduleRow> {
        let mut rows = vec![ScheduleRow::Preset];
        if matches!(&self.schedule.preset, SchedulePreset::Weekly { .. }) {
            rows.push(ScheduleRow::Weekday);
        }
        rows
    }

    pub fn is_dirty(&self) -> bool {
        let Some(original) = &self.original else {
            return true;
        };
        original.schedule != self.schedule
            || original.notify != self.notify
            || original.steps.iter().cloned().collect::<BTreeSet<String>>() != self.selected_steps
    }

    pub fn to_profile(&self, name: &str) -> Result<Profile, String> {
        let name = sanitize_name(name).map_err(|error| error.to_string())?;
        if self.selected_steps.is_empty() {
            return Err("select at least one step".to_string());
        }
        let preset = self.schedule.preset.clone();
        let steps: Vec<String> = self.selected_steps.iter().cloned().collect();
        Ok(Profile {
            name,
            steps,
            schedule: Schedule {
                preset,
                randomized_delay_sec: self.schedule.randomized_delay_sec,
            },
            notify: self.notify,
            scope: Scope::User,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> EditorEvent {
        if self.name_popup.is_some() {
            return self.handle_popup_key(key);
        }
        if self.row_editor.is_some() {
            return self.handle_row_editor_key(key);
        }
        if key.code == KeyCode::Char('?') && !self.steps_filter.is_engaged() {
            return EditorEvent::Help;
        }
        if key.code == KeyCode::Esc && !self.steps_filter.is_engaged() {
            return EditorEvent::Cancel;
        }
        if key.code == KeyCode::Esc && self.steps_filter.is_engaged() {
            self.steps_filter = FilterState::new();
            self.clamp_steps_selection();
            return EditorEvent::None;
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
        match self.section {
            Section::Steps => self.handle_steps_key(key),
            Section::Schedule => self.handle_schedule_key(key),
            Section::Options => self.handle_options_key(key),
            Section::Save => self.handle_save_key(key),
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
                    self.confirmed_name = Some(self.final_name());
                    self.name_popup = None;
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

    fn handle_row_editor_key(&mut self, key: KeyEvent) -> EditorEvent {
        let Some(popup) = self.row_editor.as_mut() else {
            return EditorEvent::None;
        };
        match popup.handle_key(key) {
            RowEditorEvent::Confirmed => self.confirm_row_editor(),
            RowEditorEvent::Cancelled => self.row_editor = None,
            RowEditorEvent::Changed => {}
        }
        EditorEvent::None
    }

    fn open_name_popup(&mut self) {
        let prefill = if self.creating {
            self.suggested_name.clone().unwrap_or_default()
        } else {
            self.original_name.clone().unwrap_or_default()
        };
        self.name_popup = Some(LineEdit::new(prefill));
    }

    pub fn reopen_name_popup(&mut self) {
        self.name_popup = Some(LineEdit::new(
            self.confirmed_name.clone().unwrap_or_default(),
        ));
    }

    fn reset_indices(&mut self) {
        self.list_index = 0;
        self.schedule_index = 0;
    }

    fn clamp_schedule_cursor(&mut self) {
        let rows = self.schedule_rows().len();
        self.schedule_index = self.schedule_index.min(rows.saturating_sub(1));
    }

    fn text_entry_focused(&self) -> bool {
        match self.section {
            Section::Steps => self.steps_filter.active,
            Section::Schedule | Section::Options | Section::Save => false,
        }
    }

    fn handle_steps_key(&mut self, key: KeyEvent) -> EditorEvent {
        if self.steps_filter.active {
            match key.code {
                KeyCode::Up => self.move_steps_selection(-1),
                KeyCode::Down => self.move_steps_selection(1),
                _ => {
                    self.steps_filter.handle(key);
                    self.clamp_steps_selection();
                }
            }
            return EditorEvent::None;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => self.move_steps_selection(-1),
            KeyCode::Down | KeyCode::Char('j' | 'J') => self.move_steps_selection(1),
            KeyCode::Left | KeyCode::Char('h' | 'H') => self.move_steps_horizontally(-1),
            KeyCode::Right | KeyCode::Char('l' | 'L') => self.move_steps_horizontally(1),
            KeyCode::Char('/') => self.steps_filter.start(),
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle_step_at(self.list_index),
            _ => {}
        }
        EditorEvent::None
    }

    fn move_steps_selection(&mut self, delta: i64) {
        let len = self.filtered_steps().len();
        if len == 0 {
            return;
        }
        let columns = self.steps_columns.max(1) as i64;
        let next = (self.list_index as i64 + delta * columns).clamp(0, len as i64 - 1) as usize;
        self.list_index = next;
        self.realign_steps_window();
    }

    fn move_steps_horizontally(&mut self, delta: i64) {
        let len = self.filtered_steps().len();
        if len == 0 {
            return;
        }
        let next = (self.list_index as i64 + delta).clamp(0, len as i64 - 1) as usize;
        self.list_index = next;
        self.realign_steps_window();
    }

    fn realign_steps_window(&mut self) {
        let len = self.filtered_steps().len();
        grid_window(
            self.list_index,
            len,
            self.steps_columns,
            STEPS_ROWS,
            &mut self.steps_scroll,
        );
    }

    fn clamp_steps_selection(&mut self) {
        let len = self.filtered_steps().len();
        self.list_index = if len == 0 {
            0
        } else {
            self.list_index.min(len - 1)
        };
        self.realign_steps_window();
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

    fn move_schedule_selection(&mut self, delta: i64) {
        let len = self.schedule_rows().len();
        let next = (self.schedule_index as i64 + delta).clamp(0, len as i64 - 1) as usize;
        self.schedule_index = next;
    }

    fn handle_schedule_key(&mut self, key: KeyEvent) -> EditorEvent {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => self.move_schedule_selection(-1),
            KeyCode::Down | KeyCode::Char('j' | 'J') => self.move_schedule_selection(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.open_row_editor(),
            _ => {}
        }
        EditorEvent::None
    }

    fn handle_options_key(&mut self, key: KeyEvent) -> EditorEvent {
        if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
            self.open_row_editor();
        }
        EditorEvent::None
    }

    fn handle_save_key(&mut self, key: KeyEvent) -> EditorEvent {
        if key.code == KeyCode::Enter {
            self.open_name_popup();
        }
        EditorEvent::None
    }

    fn open_row_editor(&mut self) {
        let popup = match self.section {
            Section::Schedule => {
                match self
                    .schedule_rows()
                    .get(self.schedule_index)
                    .copied()
                    .unwrap_or(ScheduleRow::Preset)
                {
                    ScheduleRow::Preset => {
                        let choices = quick_choices(self.jitter_secs);
                        let options: Vec<String> =
                            choices.iter().map(|(label, _)| label.to_string()).collect();
                        let current = choices
                            .iter()
                            .position(|(_, choice)| choice == &self.schedule)
                            .unwrap_or(0);
                        RowEditor::select("frequency", options, current, SelectTarget::Preset)
                    }
                    ScheduleRow::Weekday => {
                        let options = Weekday::ALL
                            .iter()
                            .map(|day| day.as_systemd().to_string())
                            .collect();
                        let current =
                            if let SchedulePreset::Weekly { weekday, .. } = &self.schedule.preset {
                                Weekday::ALL
                                    .iter()
                                    .position(|day| day == weekday)
                                    .unwrap_or(0)
                            } else {
                                0
                            };
                        RowEditor::select("weekday", options, current, SelectTarget::Weekday)
                    }
                }
            }
            Section::Options => {
                let options = NotifyPolicy::ALL
                    .iter()
                    .map(|policy| render::notify_label(*policy).to_string())
                    .collect();
                RowEditor::select("notify", options, self.notify.index(), SelectTarget::Notify)
            }
            Section::Save => {
                self.open_name_popup();
                return;
            }
            Section::Steps => {
                self.toggle_step_at(self.list_index);
                return;
            }
        };
        self.row_editor = Some(popup);
    }

    fn confirm_row_editor(&mut self) {
        let Some(popup) = self.row_editor.take() else {
            return;
        };
        match popup.target {
            SelectTarget::Preset => {
                if let Some((_, choice)) = quick_choices(self.jitter_secs).get(popup.index) {
                    self.schedule = choice.clone();
                    if self.creating
                        && let Some(name) = self.suggested_name.clone()
                        && let Some(base) = frequency_base(&name)
                    {
                        let suffix = match &self.schedule.preset {
                            SchedulePreset::Weekly { .. } => "-weekly".to_string(),
                            SchedulePreset::EveryNHours { hours } => format!("-{hours}h"),
                            SchedulePreset::Biweekly => "-biweekly".to_string(),
                            SchedulePreset::Monthly => "-monthly".to_string(),
                            _ => "-daily".to_string(),
                        };
                        self.suggested_name = Some(format!("{base}{suffix}"));
                    }
                }
            }
            SelectTarget::Weekday => {
                if let SchedulePreset::Weekly { weekday, .. } = &mut self.schedule.preset {
                    *weekday = Weekday::ALL[popup.index.min(Weekday::ALL.len() - 1)];
                }
            }
            SelectTarget::Notify => self.notify = NotifyPolicy::from_index(popup.index),
        }
        self.clamp_schedule_cursor();
    }
}

fn frequency_base(name: &str) -> Option<String> {
    ["-daily", "-weekly", "-6h", "-12h", "-biweekly", "-monthly"]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix).map(str::to_string))
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
    use crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC;
    use crate::domain::schedule::matches_quick_choice;
    use crate::domain::steps::catalog;

    const HELP: &str = include_str!("../../../tests/fixtures/topgrade_help.txt");

    fn catalog_entries() -> Vec<String> {
        catalog(HELP)
    }

    fn new_editor() -> EditorState {
        EditorState::new(None, catalog_entries(), DEFAULT_RANDOM_DELAY_SEC)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn editing_editor() -> EditorState {
        let profile = Profile {
            name: "all-daily".to_string(),
            steps: vec!["flatpak".to_string()],
            schedule: Schedule::default(),
            notify: NotifyPolicy::OnFailure,
            scope: Scope::User,
        };
        EditorState::new(Some(&profile), catalog_entries(), DEFAULT_RANDOM_DELAY_SEC)
    }

    #[test]
    fn suggested_name_follows_the_chosen_frequency() {
        let mut editor = EditorState::from_preset(
            catalog_entries(),
            vec!["flatpak".to_string()],
            "all-daily",
            DEFAULT_RANDOM_DELAY_SEC,
        );
        editor.section = Section::Schedule;

        let choose = |editor: &mut EditorState, downs: usize, ups: usize| {
            editor.handle_key(key(KeyCode::Enter));
            for _ in 0..downs {
                editor.handle_key(key(KeyCode::Down));
            }
            for _ in 0..ups {
                editor.handle_key(key(KeyCode::Up));
            }
            editor.handle_key(key(KeyCode::Enter));
        };

        choose(&mut editor, 3, 0);
        assert_eq!(editor.suggested_name.as_deref(), Some("all-weekly"));
        choose(&mut editor, 1, 0);
        assert_eq!(editor.suggested_name.as_deref(), Some("all-biweekly"));
        choose(&mut editor, 1, 0);
        assert_eq!(editor.suggested_name.as_deref(), Some("all-monthly"));
        choose(&mut editor, 0, 5);
        assert_eq!(editor.suggested_name.as_deref(), Some("all-daily"));
        choose(&mut editor, 2, 0);
        assert_eq!(editor.suggested_name.as_deref(), Some("all-12h"));

        let mut editing = editing_editor();
        editing.section = Section::Schedule;
        editing.handle_key(key(KeyCode::Enter));
        editing.handle_key(key(KeyCode::Down));
        editing.handle_key(key(KeyCode::Enter));
        assert_eq!(
            editing.original_name.as_deref(),
            Some("all-daily"),
            "editing keeps the profile name untouched"
        );
    }

    #[test]
    fn save_flow_confirms_name_on_the_single_enter() {
        let mut editor = editing_editor();
        editor.section = Section::Save;
        editor.handle_key(key(KeyCode::Enter));
        assert!(editor.name_popup.is_some());
        assert_eq!(
            editor.handle_key(key(KeyCode::Enter)),
            EditorEvent::RequestSave
        );
        assert!(editor.name_popup.is_none(), "name popup closes on save");
        assert_eq!(
            editor.confirmed_name.as_deref(),
            Some("all-daily"),
            "the confirmed name is kept for saving"
        );
    }

    #[test]
    fn name_popup_esc_and_invalid_name_stay_in_the_editor() {
        let mut editor = editing_editor();
        editor.section = Section::Save;
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.handle_key(key(KeyCode::Esc)), EditorEvent::None);
        assert!(editor.name_popup.is_none(), "Esc closes the name popup");
        assert!(matches!(editor.section, Section::Save));

        editor.handle_key(key(KeyCode::Enter));
        editor.name_popup = Some(LineEdit::new("bad name".to_string()));
        assert_eq!(
            editor.handle_key(key(KeyCode::Enter)),
            EditorEvent::None,
            "an invalid name does not save"
        );
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::None
        );
    }

    #[test]
    fn confirming_the_name_keeps_the_drafted_values_for_saving() {
        let mut editor = EditorState::from_preset(
            catalog_entries(),
            vec!["cargo".to_string(), "flatpak".to_string()],
            "all-daily",
            DEFAULT_RANDOM_DELAY_SEC,
        );
        editor.section = Section::Schedule;
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Enter));
        editor.section = Section::Save;
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.confirmed_name.as_deref(), Some("all-weekly"));
        let profile = editor.to_profile("all-weekly").unwrap();
        assert_eq!(profile.steps, vec!["cargo", "flatpak"]);
        assert_eq!(editor.notify, NotifyPolicy::OnFailure);
    }

    #[test]
    fn rejects_invalid_drafts() {
        let editor = new_editor();
        let error = editor.to_profile("bad name").unwrap_err();
        assert!(error.contains("profile name"));

        let error = editor.to_profile("flatpak-daily").unwrap_err();
        assert!(error.contains("at least one step"));

        let mut custom = new_editor();
        custom.selected_steps.insert("flatpak".to_string());
        custom.schedule.preset = SchedulePreset::Custom {
            calendar: "definitely not a calendar".to_string(),
        };
        let draft = custom.to_profile("flatpak-daily").unwrap();
        assert!(
            validate_draft(&draft).is_err(),
            "loaded custom calendars are still validated at save time"
        );
    }

    #[test]
    fn builds_valid_profile_from_draft() {
        let mut editor = new_editor();
        editor.selected_steps.insert("flatpak".to_string());
        editor.selected_steps.insert("cargo".to_string());
        let profile = editor.to_profile("flatpak-daily").unwrap();
        assert_eq!(profile.name, "flatpak-daily");
        assert_eq!(profile.steps, vec!["cargo", "flatpak"]);
        assert_eq!(profile.notify, NotifyPolicy::OnFailure);
        assert_eq!(profile.schedule.preset.on_calendar(), "*-*-* 00:00:00");
        assert_eq!(profile.schedule.randomized_delay_sec, 300);
    }

    #[test]
    fn new_editor_defaults_to_daily_midnight() {
        let editor = new_editor();
        assert_eq!(matches_quick_choice(&editor.schedule), Some(0));
    }

    #[test]
    fn user_flow_change_frequency_and_notify_then_save_persists() {
        let mut editor = editing_editor();

        editor.handle_key(key(KeyCode::Tab));
        assert_eq!(editor.section, Section::Schedule);
        editor.handle_key(key(KeyCode::Enter));
        assert!(
            editor.row_editor.is_some(),
            "Enter on preset row opens popup"
        );
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            editor.schedule.preset,
            SchedulePreset::Weekly { .. }
        ));
        assert!(editor.row_editor.is_none(), "confirm closes the popup");

        editor.handle_key(key(KeyCode::Tab));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.notify, NotifyPolicy::Never);

        editor.handle_key(key(KeyCode::Tab));
        assert_eq!(editor.section, Section::Save);
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.final_name(), "all-daily");
        assert_eq!(
            editor.handle_key(key(KeyCode::Enter)),
            EditorEvent::RequestSave,
            "the single name Enter saves immediately"
        );

        let saved = editor.to_profile(&editor.final_name()).unwrap();
        assert_eq!(
            saved.schedule.preset,
            SchedulePreset::Weekly {
                weekday: Weekday::Mon,
                hour: 0,
                minute: 0
            }
        );
        assert_eq!(saved.notify, NotifyPolicy::Never);
    }

    #[test]
    fn preset_popup_opens_on_current_choice() {
        let mut editor = new_editor();
        editor.section = Section::Schedule;
        editor.handle_key(key(KeyCode::Enter));
        let popup = editor.row_editor().unwrap();
        assert_eq!(popup.title, "frequency");
        assert_eq!(popup.index, 0);
        assert_eq!(
            popup.options,
            vec![
                "daily".to_string(),
                "every 6 hours".to_string(),
                "every 12 hours".to_string(),
                "weekly".to_string(),
                "every 2 weeks".to_string(),
                "monthly".to_string()
            ]
        );
    }

    #[test]
    fn schedule_rows_depend_on_preset() {
        let mut editor = new_editor();
        assert_eq!(editor.schedule_rows(), vec![ScheduleRow::Preset]);
        editor.schedule.preset = SchedulePreset::Weekly {
            weekday: Weekday::Mon,
            hour: 0,
            minute: 0,
        };
        assert_eq!(
            editor.schedule_rows(),
            vec![ScheduleRow::Preset, ScheduleRow::Weekday]
        );
        editor.schedule.preset = SchedulePreset::Daily { hour: 0, minute: 0 };
        assert_eq!(editor.schedule_rows(), vec![ScheduleRow::Preset]);
    }

    #[test]
    fn weekday_popup_applies_selection() {
        let mut editor = new_editor();
        editor.schedule = quick_choices(DEFAULT_RANDOM_DELAY_SEC)[3].1.clone();
        editor.section = Section::Schedule;
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Enter));
        match &editor.schedule.preset {
            SchedulePreset::Weekly { weekday, .. } => assert_eq!(*weekday, Weekday::Tue),
            other => panic!("unexpected preset {other:?}"),
        }
    }

    #[test]
    fn esc_closes_popup_without_applying() {
        let mut editor = new_editor();
        editor.section = Section::Schedule;
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Esc));
        assert!(editor.row_editor.is_none());
        assert_eq!(matches_quick_choice(&editor.schedule), Some(0));
    }

    #[test]
    fn cursor_clamps_when_preset_shrinks_rows() {
        let mut editor = new_editor();
        editor.schedule = quick_choices(DEFAULT_RANDOM_DELAY_SEC)[3].1.clone();
        editor.schedule_index = 1;
        assert_eq!(editor.schedule_rows().len(), 2);
        editor.schedule.preset = SchedulePreset::Daily { hour: 0, minute: 0 };
        editor.clamp_schedule_cursor();
        assert_eq!(
            editor.schedule_index, 0,
            "cursor follows the shrinking row list"
        );
    }

    #[test]
    fn options_section_has_a_single_notify_row() {
        let mut editor = new_editor();
        editor.section = Section::Options;
        editor.handle_key(key(KeyCode::Enter));
        assert!(editor.row_editor.is_some(), "Enter opens the notify popup");
    }

    #[test]
    fn notify_popup_applies_choice() {
        let mut editor = new_editor();
        editor.section = Section::Options;
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Up));
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.notify, NotifyPolicy::Always);
    }

    #[test]
    fn tab_cycles_all_four_sections_and_wraps() {
        let mut editor = new_editor();
        assert_eq!(editor.section, Section::Steps);
        editor.handle_key(key(KeyCode::Tab));
        assert_eq!(editor.section, Section::Schedule);
        editor.handle_key(key(KeyCode::Tab));
        assert_eq!(editor.section, Section::Options);
        editor.handle_key(key(KeyCode::Tab));
        assert_eq!(editor.section, Section::Save);
        editor.handle_key(key(KeyCode::Enter));
        assert!(editor.name_popup.is_some(), "Enter on save row opens popup");
        editor.handle_key(key(KeyCode::Esc));
        editor.handle_key(key(KeyCode::Tab));
        assert_eq!(editor.section, Section::Steps);
        editor.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(editor.section, Section::Save);
    }

    #[test]
    fn popup_rejects_invalid_names_by_staying_open() {
        let mut editor = editing_editor();
        editor.section = Section::Save;
        editor.handle_key(key(KeyCode::Enter));
        editor.name_popup = Some(LineEdit::new(""));
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.handle_key(key(KeyCode::Enter)), EditorEvent::None);
        assert!(editor.name_popup.is_some());
    }

    #[test]
    fn dirty_marker_starts_clean_and_marks_edits() {
        let mut editor = editing_editor();
        assert!(!editor.is_dirty(), "freshly opened profile is clean");
        editor.notify = NotifyPolicy::Never;
        assert!(editor.is_dirty());

        let creating = new_editor();
        assert!(creating.is_dirty(), "new profiles always show the marker");
    }

    #[test]
    fn q_quits_from_value_sections_but_types_in_text_contexts() {
        let mut editor = new_editor();
        editor.section = Section::Options;
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::Quit
        );

        let mut filtered = new_editor();
        filtered.section = Section::Steps;
        filtered.handle_key(key(KeyCode::Char('/')));
        assert_eq!(
            filtered.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::None
        );
        assert_eq!(filtered.steps_filter.text(), "q");
    }

    #[test]
    fn q_inside_select_popup_is_swallowed() {
        let mut editor = new_editor();
        editor.section = Section::Schedule;
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            EditorEvent::None
        );
        assert!(editor.row_editor.is_some());
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
            "Enter while filtering commits the filter, not a save"
        );

        editor.handle_key(key(KeyCode::Char('/')));
        editor.handle_key(key(KeyCode::Esc));
        assert_eq!(editor.steps_filter.text(), "");
    }

    #[test]
    fn enter_toggles_steps_like_space() {
        let mut editor = new_editor();
        editor.section = Section::Steps;
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.selected_steps.len(), 1);
        editor.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(editor.selected_steps.len(), 0);
    }

    #[test]
    fn esc_cancels_the_editor_when_nothing_is_engaged() {
        let mut editor = new_editor();
        assert_eq!(editor.handle_key(key(KeyCode::Esc)), EditorEvent::Cancel);
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
    fn steps_selection_scrolls_beyond_the_visible_window() {
        let mut editor = new_editor();
        editor.section = Section::Steps;
        let total = editor.filtered_steps().len();
        assert!(total > STEPS_ROWS);

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
    fn steps_window_covers_columns_times_rows() {
        let mut editor = new_editor();
        editor.set_steps_columns(3);
        let total = editor.filtered_steps().len();
        assert!(total > STEPS_ROWS * 3);
        assert_eq!(editor.steps_window(), (0, STEPS_ROWS * 3));

        for _ in 0..STEPS_ROWS {
            editor.handle_key(key(KeyCode::Down));
        }
        assert_eq!(editor.list_index, STEPS_ROWS * 3);
        assert_eq!(
            editor.steps_window(),
            (3, 3 + STEPS_ROWS * 3),
            "crossing the bottom scrolls the grid by one row"
        );
    }

    #[test]
    fn vertical_movement_steps_by_columns() {
        let mut editor = new_editor();
        editor.set_steps_columns(3);
        editor.section = Section::Steps;
        editor.handle_key(key(KeyCode::Down));
        assert_eq!(editor.list_index, 3, "down moves one grid row");
        editor.handle_key(key(KeyCode::Char('j')));
        assert_eq!(editor.list_index, 6);
        editor.handle_key(key(KeyCode::Up));
        assert_eq!(editor.list_index, 3);
        editor.handle_key(key(KeyCode::Char('k')));
        assert_eq!(editor.list_index, 0);
    }

    #[test]
    fn horizontal_movement_wraps_across_grid_rows() {
        let mut editor = new_editor();
        editor.set_steps_columns(3);
        editor.section = Section::Steps;
        editor.handle_key(key(KeyCode::Right));
        assert_eq!(editor.list_index, 1);
        editor.handle_key(key(KeyCode::Left));
        assert_eq!(editor.list_index, 0, "left at the first cell clamps");
        for _ in 0..5 {
            editor.handle_key(key(KeyCode::Right));
        }
        assert_eq!(editor.list_index, 5);
        editor.handle_key(key(KeyCode::Left));
        assert_eq!(editor.list_index, 4);
        editor.handle_key(key(KeyCode::Char('l')));
        editor.handle_key(key(KeyCode::Char('l')));
        assert_eq!(editor.list_index, 6, "l crosses onto the next grid row");
        editor.handle_key(key(KeyCode::Char('h')));
        assert_eq!(editor.list_index, 5, "h wraps back onto the previous row");
    }

    #[test]
    fn steps_columns_derive_from_width_and_widest_step() {
        let editor = EditorState::new(
            None,
            vec!["cargo".to_string(), "flatpak".to_string()],
            DEFAULT_RANDOM_DELAY_SEC,
        );
        assert_eq!(editor.steps_columns_for_width(80), 5);
        assert_eq!(editor.steps_columns_for_width(96), 6);
        assert_eq!(editor.steps_columns_for_width(24), 1);
        assert_eq!(editor.steps_columns_for_width(9), 1);
    }

    #[test]
    fn grid_window_follows_the_selection() {
        let mut scroll = 0usize;
        assert_eq!(grid_window(0, 100, 1, 10, &mut scroll), (0, 10));
        assert_eq!(grid_window(9, 100, 1, 10, &mut scroll), (0, 10));
        assert_eq!(grid_window(10, 100, 1, 10, &mut scroll), (1, 11));
        assert_eq!(grid_window(99, 100, 1, 10, &mut scroll), (90, 100));
        assert_eq!(grid_window(0, 100, 1, 10, &mut scroll), (0, 10));
        let mut tiny = 5usize;
        assert_eq!(grid_window(3, 4, 1, 10, &mut tiny), (0, 4));
        let mut zero = 0usize;
        assert_eq!(grid_window(2, 8, 1, 0, &mut zero), (0, 0));
    }

    #[test]
    fn grid_window_scrolls_by_rows_in_multi_column_mode() {
        let mut scroll = 0usize;
        assert_eq!(grid_window(29, 100, 3, 10, &mut scroll), (0, 30));
        assert_eq!(grid_window(30, 100, 3, 10, &mut scroll), (3, 33));
        let mut end = 0usize;
        assert_eq!(grid_window(99, 100, 3, 10, &mut end), (72, 100));
    }

    #[test]
    fn narrowing_the_steps_filter_clamps_instead_of_resetting() {
        let mut editor = new_editor();
        editor.section = Section::Steps;
        for _ in 0..5 {
            editor.handle_key(key(KeyCode::Down));
        }
        assert_eq!(editor.list_index, 5);
        editor.handle_key(key(KeyCode::Char('/')));
        editor.steps_filter.edit.value = "flat".to_string();
        editor.handle_key(key(KeyCode::Left));
        let len = editor.filtered_steps().len();
        assert!(
            (1..6).contains(&len),
            "the query narrows the catalog: {len}"
        );
        assert_eq!(
            editor.list_index,
            len - 1,
            "the selection clamps to the last surviving step, not back to zero"
        );
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

    #[test]
    fn editing_prefills_popup_with_current_name() {
        let mut editor = editing_editor();
        editor.section = Section::Save;
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.final_name(), "all-daily");
        assert_eq!(
            editor.handle_key(key(KeyCode::Enter)),
            EditorEvent::RequestSave,
            "the single name Enter saves immediately"
        );
        assert_eq!(
            editor.confirmed_name.as_deref(),
            Some("all-daily"),
            "the prefilled current name is confirmed"
        );
    }

    #[test]
    fn from_preset_prefills_popup_with_suggested_name() {
        let mut editor = EditorState::from_preset(
            catalog_entries(),
            vec!["flatpak".to_string()],
            "flatpak-daily",
            DEFAULT_RANDOM_DELAY_SEC,
        );
        assert!(editor.creating);
        assert!(editor.selected_steps.contains("flatpak"));
        editor.section = Section::Save;
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.final_name(), "flatpak-daily");
        let profile = editor.to_profile(&editor.final_name()).unwrap();
        assert_eq!(profile.name, "flatpak-daily");
        assert_eq!(profile.steps, vec!["flatpak"]);
    }
}
