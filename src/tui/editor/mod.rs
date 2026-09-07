use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::domain::profile::{NotifyPolicy, Profile, sanitize_name};
use crate::domain::schedule::{Schedule, SchedulePreset, Weekday, quick_choices};

use crate::systemd::validate_on_calendar;

use super::input::{FilterState, LineEdit};
use super::overlay::{Overlay, OverlayAction};
use popup::{RowEditor, RowEditorEvent, SelectTarget};

pub mod popup;
pub(crate) mod render;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Steps,
    Schedule,
    Options,
}

const SECTIONS: [Section; 3] = [Section::Steps, Section::Schedule, Section::Options];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScheduleRow {
    Preset,
    Weekday,
}

pub struct EditorState {
    pub creating: bool,
    pub base: Option<String>,
    pub original: Option<Profile>,
    pub original_name: Option<String>,
    pub suggested_name: Option<String>,
    pub name_popup: Option<LineEdit>,
    pub confirmed_name: Option<String>,
    pub row_editor: Option<RowEditor>,
    pub unsaved: Option<Overlay>,
    pub steps_filter: FilterState,
    pub steps_columns: usize,
    pub catalog: Vec<String>,
    pub selected_steps: BTreeSet<String>,
    pub schedule: Schedule,
    pub notify: NotifyPolicy,
    pub section: Section,
    baseline: (BTreeSet<String>, Schedule, NotifyPolicy),
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
        let base_schedule = crate::domain::schedule::daily_choice(jitter_secs);
        match original {
            Some(profile) => {
                let mut schedule = profile.schedule.clone();
                schedule.preset = schedule.preset.clone().normalized();
                let mut original = profile.clone();
                original.schedule = schedule.clone();
                let base = profile.base.clone();
                let resolved: BTreeSet<String> = {
                    use std::collections::BTreeSet as Set;
                    match crate::domain::overlay::resolved_steps(
                        profile,
                        catalog
                            .iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                            .as_slice(),
                    ) {
                        crate::domain::overlay::ResolvedSteps::Everything { excluded } => {
                            let ex: Set<String> = excluded.into_iter().collect();
                            catalog
                                .iter()
                                .filter(|step| !ex.contains(step.as_str()))
                                .cloned()
                                .collect()
                        }
                        crate::domain::overlay::ResolvedSteps::Explicit(steps) => {
                            steps.into_iter().collect()
                        }
                    }
                };
                Self {
                    creating: false,
                    base,
                    original: Some(original),
                    original_name: Some(profile.name.clone()),
                    suggested_name: None,
                    name_popup: None,
                    confirmed_name: None,
                    row_editor: None,
                    unsaved: None,
                    steps_filter: FilterState::new(),
                    steps_columns: 1,
                    catalog,
                    selected_steps: resolved.clone(),
                    schedule: schedule.clone(),
                    notify: profile.notify,
                    section: Section::Steps,
                    baseline: (resolved, schedule, profile.notify),
                    list_index: 0,
                    schedule_index: 0,
                    jitter_secs,
                }
            }
            None => Self {
                creating: true,
                base: None,
                original: None,
                original_name: None,
                suggested_name: None,
                name_popup: None,
                confirmed_name: None,
                row_editor: None,
                unsaved: None,
                steps_filter: FilterState::new(),
                steps_columns: 1,
                catalog,
                selected_steps: BTreeSet::new(),
                schedule: base_schedule.clone(),
                notify: NotifyPolicy::OnFailure,
                section: Section::Steps,
                baseline: (
                    BTreeSet::new(),
                    base_schedule.clone(),
                    NotifyPolicy::OnFailure,
                ),
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
        let len = self.filtered_steps().len();
        (0, len)
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

    pub fn steps_columns_for(&self, width: u16, rows: usize) -> usize {
        let cell = self.widest_step() + 9;
        let by_width = ((width as usize) / cell).clamp(1, 6);
        let to_fit = self.filtered_steps().len().div_ceil(rows.max(1));
        by_width.max(to_fit)
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
        if self.creating {
            return !self.selected_steps.is_empty();
        }
        let (steps, schedule, notify) = &self.baseline;
        *schedule != self.schedule || *notify != self.notify || *steps != self.selected_steps
    }

    pub fn to_profile(&self, name: &str) -> Result<Profile, String> {
        if let Some(base) = self.base.as_deref() {
            let name = self
                .original_name
                .clone()
                .unwrap_or_else(|| name.to_string());
            let delta =
                crate::domain::overlay::step_delta(Some(base), &self.selected_steps, &self.catalog);
            return Ok(Profile {
                name,
                base: Some(base.to_string()),
                extra_steps: delta.extra_steps,
                excluded_steps: delta.excluded_steps,
                steps: Vec::new(),
                schedule: Schedule {
                    preset: self.schedule.preset.clone(),
                    randomized_delay_sec: self.schedule.randomized_delay_sec,
                },
                notify: self.notify,
            });
        }
        let name = sanitize_name(name).map_err(|error| error.to_string())?;
        if self.selected_steps.is_empty() {
            return Err("select at least one step".to_string());
        }
        let preset = self.schedule.preset.clone();
        let steps: Vec<String> = self.selected_steps.iter().cloned().collect();
        Ok(Profile {
            name,
            base: None,
            extra_steps: Vec::new(),
            excluded_steps: Vec::new(),
            steps,
            schedule: Schedule {
                preset,
                randomized_delay_sec: self.schedule.randomized_delay_sec,
            },
            notify: self.notify,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> EditorEvent {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('s' | 'S'))
        {
            if self.name_popup.is_some() {
                return self.handle_popup_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            }
            if self.base.is_some() {
                return EditorEvent::RequestSave;
            }
            self.open_name_popup();
            return EditorEvent::None;
        }
        if let Some(mut overlay) = self.unsaved.take() {
            match overlay.handle_key(key) {
                Some(OverlayAction::Selected(0)) => {
                    self.open_name_popup();
                    return EditorEvent::None;
                }
                Some(OverlayAction::Selected(_)) => return EditorEvent::Cancel,
                Some(OverlayAction::Cancelled) => {}
                None => self.unsaved = Some(overlay),
            }
            return EditorEvent::None;
        }
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
            if self.is_dirty() {
                self.unsaved = Some(Self::unsaved_overlay());
            } else {
                return EditorEvent::Cancel;
            }
            return EditorEvent::None;
        }
        if key.code == KeyCode::Esc && self.steps_filter.is_engaged() {
            self.steps_filter.clear_query();
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
            Section::Schedule | Section::Options => false,
        }
    }

    fn handle_steps_key(&mut self, key: KeyEvent) -> EditorEvent {
        if self.steps_filter.active {
            let len = self.filtered_steps().len();
            super::input::handle_filter_typing(
                &mut self.steps_filter,
                key,
                &mut self.list_index,
                len,
                self.steps_columns,
            );
            self.clamp_steps_selection();
            return EditorEvent::None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('a' | 'A') => {
                    self.mark_filtered_steps(true);
                    return EditorEvent::None;
                }
                KeyCode::Char('d' | 'D') => {
                    self.mark_filtered_steps(false);
                    return EditorEvent::None;
                }
                _ => {}
            }
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

    fn mark_filtered_steps(&mut self, marked: bool) {
        let filtered: Vec<String> = self
            .filtered_steps()
            .iter()
            .map(|step| step.to_string())
            .collect();
        for id in filtered {
            if marked {
                self.selected_steps.insert(id);
            } else {
                self.selected_steps.remove(&id);
            }
        }
    }

    fn move_steps_selection(&mut self, delta: i64) {
        let len = self.filtered_steps().len();
        if len == 0 {
            return;
        }
        let columns = self.steps_columns.max(1) as i64;
        let next = (self.list_index as i64 + delta * columns).clamp(0, len as i64 - 1) as usize;
        self.list_index = next;
    }

    fn move_steps_horizontally(&mut self, delta: i64) {
        let len = self.filtered_steps().len();
        if len == 0 {
            return;
        }
        let next = (self.list_index as i64 + delta).clamp(0, len as i64 - 1) as usize;
        self.list_index = next;
    }

    fn clamp_steps_selection(&mut self) {
        let len = self.filtered_steps().len();
        self.list_index = if len == 0 {
            0
        } else {
            self.list_index.min(len - 1)
        };
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

    fn unsaved_overlay() -> Overlay {
        Overlay::new(
            "unsaved changes",
            vec![ratatui::text::Line::from("save changes before leaving?")],
            &["Save", "Discard", "Cancel"],
            0,
        )
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
mod tests;
