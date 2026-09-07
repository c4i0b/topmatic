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
        base: None,
        extra_steps: Vec::new(),
        excluded_steps: Vec::new(),
        steps: vec!["flatpak".to_string()],
        schedule: Schedule::default(),
        notify: NotifyPolicy::OnFailure,
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

    choose(&mut editor, 1, 0);
    assert_eq!(editor.suggested_name.as_deref(), Some("all-weekly"));
    choose(&mut editor, 1, 0);
    assert_eq!(editor.suggested_name.as_deref(), Some("all-biweekly"));
    choose(&mut editor, 1, 0);
    assert_eq!(editor.suggested_name.as_deref(), Some("all-monthly"));
    choose(&mut editor, 0, 3);
    assert_eq!(editor.suggested_name.as_deref(), Some("all-daily"));
    choose(&mut editor, 0, 1);
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

fn ctrl_s() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
}

#[test]
fn save_flow_confirms_name_on_the_single_enter() {
    let mut editor = editing_editor();
    editor.handle_key(ctrl_s());
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
    editor.handle_key(ctrl_s());
    assert_eq!(editor.handle_key(key(KeyCode::Esc)), EditorEvent::None);
    assert!(editor.name_popup.is_none(), "Esc closes the name popup");

    editor.handle_key(ctrl_s());
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
    editor.handle_key(key(KeyCode::Enter));
    editor.handle_key(ctrl_s());
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
    assert_eq!(matches_quick_choice(&editor.schedule), Some(2));
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

    editor.handle_key(ctrl_s());
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
    assert_eq!(popup.index, 2, "daily preselects at its new position");
    assert_eq!(
        popup.options,
        vec![
            "every 6 hours".to_string(),
            "every 12 hours".to_string(),
            "daily".to_string(),
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
    assert_eq!(matches_quick_choice(&editor.schedule), Some(2));
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
fn tab_cycles_the_three_sections_and_wraps() {
    let mut editor = new_editor();
    assert_eq!(editor.section, Section::Steps);
    editor.handle_key(key(KeyCode::Tab));
    assert_eq!(editor.section, Section::Schedule);
    editor.handle_key(key(KeyCode::Tab));
    assert_eq!(editor.section, Section::Options);
    editor.handle_key(key(KeyCode::Tab));
    assert_eq!(editor.section, Section::Steps);
    editor.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(editor.section, Section::Options);
}

#[test]
fn popup_rejects_invalid_names_by_staying_open() {
    let mut editor = editing_editor();
    editor.handle_key(ctrl_s());
    editor.name_popup = Some(LineEdit::new(""));
    editor.handle_key(key(KeyCode::Enter));
    assert_eq!(editor.handle_key(key(KeyCode::Enter)), EditorEvent::None);
    assert!(editor.name_popup.is_some());
}

#[test]
fn esc_on_dirty_editor_offers_save_discard_and_cancel() {
    let mut editor = editing_editor();
    editor.notify = NotifyPolicy::Never;
    assert_eq!(editor.handle_key(key(KeyCode::Esc)), EditorEvent::None);
    assert!(
        editor.unsaved.is_some(),
        "a dirty editor asks before leaving"
    );

    assert_eq!(editor.handle_key(key(KeyCode::Esc)), EditorEvent::None);
    assert!(
        editor.unsaved.is_none(),
        "esc dismisses the question and stays in the editor"
    );

    editor.handle_key(key(KeyCode::Esc));
    editor.handle_key(key(KeyCode::Enter));
    assert!(
        editor.name_popup.is_some(),
        "the Save option opens the name popup"
    );
    assert_eq!(
        editor.handle_key(key(KeyCode::Enter)),
        EditorEvent::RequestSave
    );
}

#[test]
fn esc_on_dirty_editor_discard_leaves_with_cancel_event() {
    let mut editor = editing_editor();
    editor.notify = NotifyPolicy::Never;
    editor.handle_key(key(KeyCode::Esc));
    editor.handle_key(key(KeyCode::Down));
    assert_eq!(
        editor.handle_key(key(KeyCode::Enter)),
        EditorEvent::Cancel,
        "Discard leaves like the old cancel"
    );
}

#[test]
fn overlay_edit_saves_the_delta_without_the_name_popup() {
    let mut profile = editing_editor().original.clone().unwrap();
    profile.base = Some("dev-tools".to_string());
    profile.steps = Vec::new();
    profile.extra_steps = vec!["flatpak".to_string()];
    let mut editor = EditorState::new(Some(&profile), catalog_entries(), DEFAULT_RANDOM_DELAY_SEC);

    let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert!(editor.name_popup.is_none());
    assert_eq!(
        editor.handle_key(ctrl_s),
        EditorEvent::RequestSave,
        "ctrl+s on a preset-based profile saves directly — no popup"
    );
    let saved = editor.to_profile("").unwrap();
    assert_eq!(saved.base.as_deref(), Some("dev-tools"));
    assert_eq!(
        saved.extra_steps,
        vec!["flatpak".to_string()],
        "the untouched selection round-trips as the same delta"
    );
    assert!(saved.excluded_steps.is_empty());
}

#[test]
fn overlay_exclusion_round_trip_removes_the_field_when_reverted() {
    let mut profile = editing_editor().original.clone().unwrap();
    profile.base = Some("all".to_string());
    profile.steps = Vec::new();
    profile.excluded_steps = vec!["flatpak".to_string()];
    let mut editor = EditorState::new(Some(&profile), catalog_entries(), DEFAULT_RANDOM_DELAY_SEC);
    assert!(
        !editor.selected_steps.contains("flatpak"),
        "the excluded step starts unchecked"
    );

    editor.steps_filter.edit = LineEdit::new("flatpak");
    editor.handle_key(key(KeyCode::Char(' ')));
    assert!(
        editor.selected_steps.contains("flatpak"),
        "toggling re-includes it"
    );
    let saved = editor.to_profile("").unwrap();
    assert!(
        saved.excluded_steps.is_empty(),
        "re-including removes the exclusion — the field omits from the config"
    );
}

#[test]
fn everything_base_editor_starts_with_the_whole_catalog_checked() {
    let mut profile = editing_editor().original.clone().unwrap();
    profile.base = Some("all".to_string());
    profile.steps = Vec::new();
    let editor = EditorState::new(Some(&profile), catalog_entries(), DEFAULT_RANDOM_DELAY_SEC);
    assert_eq!(
        editor.selected_steps.len(),
        editor.catalog.len(),
        "everything base resolves to the full catalog minus exclusions"
    );
}

#[test]
fn ctrl_a_marks_and_ctrl_d_clears_the_filtered_steps() {
    let mut editor = editing_editor();
    editor.section = Section::Steps;
    let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
    let ctrl_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);

    editor.handle_key(ctrl_a);
    let total = editor.filtered_steps().len();
    assert_eq!(
        editor.selected_steps.len(),
        total,
        "ctrl+a marks every step in the grid"
    );

    editor.steps_filter.edit = LineEdit::new("flat");
    editor.handle_key(ctrl_d);
    let filtered: Vec<String> = editor
        .filtered_steps()
        .iter()
        .map(|step| step.to_string())
        .collect();
    for step in &filtered {
        assert!(
            !editor.selected_steps.contains(step),
            "ctrl+d clears only the filtered set"
        );
    }
    assert!(
        !filtered.is_empty(),
        "the filter matched something for the assertion to mean anything"
    );
}

#[test]
fn bulk_keys_do_nothing_outside_the_steps_section() {
    let mut editor = editing_editor();
    editor.section = Section::Schedule;
    let before = editor.selected_steps.clone();
    editor.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    editor.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(
        editor.selected_steps, before,
        "the bulk keys are steps-only"
    );
}

#[test]
fn preset_create_asks_on_esc_and_clearing_marks_returns_silent() {
    let mut editor = EditorState::from_preset(
        catalog_entries(),
        vec!["flatpak".to_string()],
        "flatpak-daily",
        DEFAULT_RANDOM_DELAY_SEC,
    );
    assert!(
        editor.is_dirty(),
        "a preset pre-marks steps: there is something to save, esc asks"
    );
    assert_eq!(editor.handle_key(key(KeyCode::Esc)), EditorEvent::None);
    assert!(editor.unsaved.is_some());

    assert_eq!(editor.handle_key(key(KeyCode::Esc)), EditorEvent::None);
    editor.steps_filter.edit = LineEdit::new("flatpak");
    editor.handle_key(key(KeyCode::Char(' ')));
    assert!(
        !editor.is_dirty(),
        "unchecking every marked box leaves nothing worth saving"
    );
    editor.handle_key(key(KeyCode::Esc));
    assert_eq!(
        editor.handle_key(key(KeyCode::Esc)),
        EditorEvent::Cancel,
        "esc leaves silently once the filter is cleared and nothing is marked"
    );
}

#[test]
fn untouched_new_editor_leaves_silently_on_esc() {
    let mut editor = new_editor();
    assert!(!editor.is_dirty());
    assert_eq!(editor.handle_key(key(KeyCode::Esc)), EditorEvent::Cancel);
    assert!(
        editor.unsaved.is_none(),
        "nothing was changed, nothing to ask"
    );
}

#[test]
fn dirty_marker_starts_clean_and_marks_edits() {
    let mut editor = editing_editor();
    assert!(!editor.is_dirty(), "freshly opened profile is clean");
    editor.notify = NotifyPolicy::Never;
    assert!(editor.is_dirty());

    let creating = new_editor();
    assert!(
        !creating.is_dirty(),
        "a blank new profile with no marks is clean — esc leaves silently"
    );
    let mut touched = new_editor();
    touched.handle_key(key(KeyCode::Char(' ')));
    assert!(
        touched.is_dirty(),
        "toggling one step makes a new profile dirty"
    );
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
    assert_eq!(
        editor.steps_window(),
        (0, total),
        "every step is in view, no paging"
    );

    for _ in 0..total + 5 {
        editor.handle_key(key(KeyCode::Down));
    }
    assert_eq!(editor.list_index, total - 1);

    for _ in 0..total + 5 {
        editor.handle_key(key(KeyCode::Up));
    }
    assert_eq!(editor.list_index, 0);
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
    assert_eq!(editor.steps_columns_for(80, 10), 5);
    assert_eq!(editor.steps_columns_for(96, 10), 6);
    assert_eq!(editor.steps_columns_for(24, 10), 1);
    assert_eq!(editor.steps_columns_for(9, 10), 1);
    let wide = EditorState::new(
        None,
        (0..30).map(|i| format!("step-{i}")).collect(),
        DEFAULT_RANDOM_DELAY_SEC,
    );
    assert_eq!(
        wide.steps_columns_for(80, 10),
        5,
        "width rules when height is plenty"
    );
    assert_eq!(
        wide.steps_columns_for(80, 5),
        6,
        "five rows for thirty steps force a sixth column"
    );
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
    editor.handle_key(ctrl_s());
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
    editor.handle_key(ctrl_s());
    assert_eq!(editor.final_name(), "flatpak-daily");
    let profile = editor.to_profile(&editor.final_name()).unwrap();
    assert_eq!(profile.name, "flatpak-daily");
    assert_eq!(profile.steps, vec!["flatpak"]);
}
