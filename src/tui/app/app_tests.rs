use super::*;

use crate::domain::profile::{NotifyPolicy, Profile};
use crate::domain::schedule::Schedule;
use crate::systemd::test_support::FakeCtl;
use crate::tui::views;
use std::sync::Arc;

struct Harness {
    _tmp: tempfile::TempDir,
    calls: Arc<std::sync::Mutex<Vec<String>>>,
    services:
        Arc<std::sync::Mutex<std::collections::HashMap<String, chrono::DateTime<chrono::Utc>>>>,
    unit_dir: PathBuf,
    activity: Arc<Mutex<ActivityLog>>,
}

impl Harness {
    fn action_texts(&self) -> Vec<String> {
        self.activity
            .lock()
            .unwrap()
            .entries()
            .iter()
            .filter(|entry| entry.kind == ActivityKind::Action)
            .map(|entry| entry.text.clone())
            .collect()
    }

    fn all_texts(&self) -> Vec<String> {
        self.activity
            .lock()
            .unwrap()
            .entries()
            .iter()
            .map(|entry| entry.text.clone())
            .collect()
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
}

fn profile(name: &str) -> Profile {
    Profile {
        name: name.to_string(),
        base: None,
        extra_steps: Vec::new(),
        excluded_steps: Vec::new(),
        steps: vec!["flatpak".to_string()],
        schedule: Schedule::default(),
        notify: NotifyPolicy::OnFailure,
    }
}

fn harness(profiles: &[Profile]) -> (App, Harness) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
    let mut config = AppConfig::default();
    for profile in profiles {
        config.upsert(profile.clone());
    }
    crate::config::save(&paths, &config).unwrap();
    let unit_dir = tmp.path().join("units");
    std::fs::create_dir_all(&unit_dir).unwrap();
    let ctl = FakeCtl::new(unit_dir.clone());
    let calls = ctl.shared_calls();
    let services = ctl.shared_services();
    let since = ctl.shared_since();
    let factory: Box<dyn Fn() -> Box<dyn SystemdCtl>> = {
        let calls = Arc::clone(&calls);
        let services = Arc::clone(&services);
        let since = Arc::clone(&since);
        let unit_dir = unit_dir.clone();
        Box::new(move || {
            Box::new(FakeCtl::from_parts(
                unit_dir.clone(),
                Arc::clone(&calls),
                Arc::clone(&services),
                Arc::clone(&since),
            ))
        })
    };
    let activity = Arc::new(Mutex::new(ActivityLog::in_memory(
        crate::activity::DEFAULT_CAPACITY,
    )));
    let mut app = App::assemble(
        crate::config::load_validated(&paths).unwrap().0,
        Vec::new(),
        paths,
        Box::new(ctl),
        factory,
        Arc::clone(&activity),
        ResolvedBins {
            topmatic_bin: PathBuf::from("/bin/topmatic"),
            topgrade_bin: Some(PathBuf::from("/nonexistent/topgrade")),
        },
    );
    app.catalog = presets::fallback_catalog();
    (
        app,
        Harness {
            _tmp: tmp,
            calls,
            services,
            unit_dir,
            activity,
        },
    )
}

fn save_editor_profile(app: &mut App, prefill: &str, name: &str) {
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    for _ in 0..prefill.chars().count() {
        app.handle_key(key(KeyCode::Backspace));
    }
    for character in name.chars() {
        app.handle_key(key(KeyCode::Char(character)));
    }
    app.handle_key(key(KeyCode::Enter));
}

fn settle(app: &mut App) {
    while !app.poll_in_flight() {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn assemble_syncs_timers_and_builds_rows() {
    let (app, harness) = harness(&[profile("all-daily")]);
    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("synced")),
        "boot records the sync in the activity log: {:?}",
        harness.action_texts()
    );
    assert_eq!(app.rows.len(), 1);
    assert_eq!(app.rows[0].name, "all-daily");
    assert!(
        harness
            .calls
            .lock()
            .unwrap()
            .contains(&"enable:all-daily".to_string()),
        "boot converges systemd state: {:?}",
        harness.calls.lock().unwrap()
    );
    assert!(
        harness
            .unit_dir
            .join("topmatic@all-daily.timer.d/10-schedule.conf")
            .is_file()
    );
}

#[test]
fn dump_views_at_screenshot_grid() {
    let (mut app, _harness) = harness(&[profile("all-daily"), profile("dev-tools")]);
    app.catalog = presets::fallback_catalog();
    let views: [(&str, View); 4] = [
        ("dashboard", View::Dashboard),
        ("picker", View::PresetPicker { index: 0 }),
        (
            "editor",
            View::Editor(Box::new(editor::EditorState::from_preset(
                app.catalog.clone(),
                vec!["cargo".to_string(), "flatpak".to_string()],
                "all-daily",
                crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
            ))),
        ),
        (
            "logs",
            View::Logs(super::logs::LogsState::open(&app.paths, "all-daily")),
        ),
    ];
    for (name, view) in views {
        app.view = view;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(119, 27)).unwrap();
        terminal.draw(|frame| views::draw(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        println!("=== {name} ===");
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect();
            if row.trim().is_empty() {
                println!("{y:2}|");
            } else {
                println!("{y:2}|{}", row.trim_end());
            }
        }
    }
}

#[test]
fn dashboard_selection_moves_and_clamps() {
    let (mut app, _harness) = harness(&[profile("a"), profile("b")]);
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.selected, 1);
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.selected, 1, "selection clamps at the end");
    app.handle_key(key(KeyCode::Char('k')));
    assert_eq!(app.selected, 0);
}

#[test]
fn activating_an_already_active_preset_reports_and_stays() {
    let (mut app, _harness) = harness(&[profile("all-daily")]);
    app.catalog = presets::fallback_catalog();
    app.handle_key(key(KeyCode::Char('n')));
    app.handle_key(key(KeyCode::Enter));
    assert!(
        matches!(app.view, View::Dashboard),
        "no editor opens for presets"
    );
    assert!(
        app.message.contains("already active"),
        "the sticky error names the collision: {:?}",
        app.message
    );
}

#[test]
fn save_keeps_the_dashboard_responsive_while_syncing() {
    let (mut app, harness) = harness(&[]);
    app.handle_key(key(KeyCode::Char('n')));
    for _ in 0..3 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.view, View::Editor(_)));
    app.handle_key(key(KeyCode::Char(' ')));
    save_editor_profile(&mut app, "", "all-daily");

    assert!(
        app.in_flight.is_some(),
        "save defers the systemd sync behind a busy indicator"
    );
    assert!(
        app.in_flight
            .as_ref()
            .is_some_and(|job| job.label.contains("saving all-daily")),
        "the busy indicator carries the progress label"
    );
    settle(&mut app);
    assert!(app.in_flight.is_none(), "the busy indicator clears");
    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("saved all-daily")),
        "the finished save lands in the activity log: {:?}",
        harness.action_texts()
    );
    assert!(
        harness
            .calls
            .lock()
            .unwrap()
            .contains(&"enable:all-daily".to_string()),
        "the background sync still converges systemd state"
    );
}

#[test]
fn preset_activation_creates_the_overlay_and_converges() {
    let (mut app, harness) = harness(&[]);
    app.handle_key(key(KeyCode::Char('n')));
    assert!(matches!(app.view, View::PresetPicker { .. }));
    app.handle_key(key(KeyCode::Enter));
    assert!(
        matches!(app.view, View::Dashboard),
        "activation never opens the editor"
    );
    assert!(app.in_flight.is_some(), "activation spawns the sync job");
    settle(&mut app);

    let saved = crate::config::load(&app.paths).unwrap();
    let activated = saved
        .profiles
        .iter()
        .find(|p| p.name == "all-daily")
        .unwrap();
    assert_eq!(activated.base.as_deref(), Some("all"));
    assert!(
        activated.steps.is_empty(),
        "everything base stores no steps"
    );
    assert!(
        harness
            .calls
            .lock()
            .unwrap()
            .contains(&"enable:all-daily".to_string()),
        "activation converges systemd state"
    );
    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("activated preset all")),
        "activation is recorded: {:?}",
        harness.action_texts()
    );
}

#[test]
fn editing_keeps_the_same_name_and_saves_in_place() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.view, View::Editor(_)));
    save_editor_profile(&mut app, "all-daily", "all-daily");
    settle(&mut app);

    let saved = crate::config::load(&app.paths).unwrap();
    assert_eq!(
        saved.profiles.len(),
        1,
        "no duplicate profile on in-place save"
    );
    assert_eq!(saved.profiles[0].name, "all-daily");
    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("saved all-daily")),
        "the in-place save is recorded: {:?}",
        harness.action_texts()
    );
    assert!(
        matches!(app.view, View::Dashboard),
        "edit-with-same-name must return to the dashboard, not stay blocked"
    );
}

#[test]
fn editing_renames_the_profile_and_purges_its_state() {
    let (mut app, harness) = harness(&[profile("old")]);
    std::fs::create_dir_all(app.paths.logs_dir("old")).unwrap();
    std::fs::write(app.paths.logs_dir("old").join("run.log"), "log").unwrap();

    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.view, View::Editor(_)));
    save_editor_profile(&mut app, "old", "new");
    settle(&mut app);

    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("saved new")),
        "the rename records the new name: {:?}",
        harness.action_texts()
    );
    let saved = crate::config::load(&app.paths).unwrap();
    assert!(saved.profile("new").is_some());
    assert!(saved.profile("old").is_none());
    assert!(
        !app.paths.logs_dir("old").exists(),
        "renaming purges the previous profile state"
    );
}

#[test]
fn saving_onto_an_existing_name_is_rejected_in_place() {
    let (mut app, _harness) = harness(&[profile("a"), profile("b")]);
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Enter));
    save_editor_profile(&mut app, "b", "a");

    assert!(app.message.contains("already exists"));
    assert!(
        matches!(app.view, View::Editor(_)),
        "the editor stays open so nothing is lost"
    );
    let saved = crate::config::load(&app.paths).unwrap();
    assert_eq!(saved.profiles.len(), 2);
}

#[test]
fn esc_closes_the_delete_overlay_without_deleting() {
    let (mut app, _harness) = harness(&[profile("kept")]);
    app.handle_key(key(KeyCode::Char('d')));
    assert!(app.confirm.is_some());
    app.handle_key(key(KeyCode::Esc));
    assert!(app.confirm.is_none(), "esc always cancels overlays");
    let saved = crate::config::load(&app.paths).unwrap();
    assert_eq!(saved.profiles.len(), 1);
}

#[test]
fn delete_overlay_cursor_starts_on_cancel() {
    let (mut app, _harness) = harness(&[profile("kept")]);
    app.handle_key(key(KeyCode::Char('d')));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.confirm.is_none());
    let saved = crate::config::load(&app.paths).unwrap();
    assert_eq!(
        saved.profiles.len(),
        1,
        "Enter on the default Cancel must not delete"
    );
}

#[test]
fn q_is_swallowed_inside_the_delete_overlay() {
    let (mut app, _harness) = harness(&[profile("kept")]);
    app.handle_key(key(KeyCode::Char('d')));
    app.handle_key(key(KeyCode::Char('q')));
    assert!(!app.should_quit, "q never quits from inside an overlay");
    assert!(app.confirm.is_some());
}

#[test]
fn message_feedback_expires_after_the_ttl_while_activity_is_kept() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("started all-daily")),
        "starting a run is recorded in the activity log: {:?}",
        harness.action_texts()
    );
    app.message = "critical: something failed".to_string();
    app.on_tick();
    app.message_expires_at_tick = app.tick.saturating_sub(1);
    app.on_tick();
    assert!(app.message.is_empty(), "transient feedback fades away");
    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("started all-daily")),
        "the activity log keeps the action after the transient fades"
    );
    drop(harness);
}

#[test]
fn esc_back_out_of_every_view() {
    let (mut app, _harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('n')));
    app.handle_key(key(KeyCode::Esc));
    assert!(matches!(app.view, View::Dashboard), "picker backs out");

    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Esc));
    assert!(matches!(app.view, View::Dashboard), "editor cancels");

    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Esc));
    assert!(matches!(app.view, View::Dashboard), "logs backs out");

    app.handle_key(key(KeyCode::Char('?')));
    assert!(app.help.is_some(), "? opens the help overlay");
    app.handle_key(key(KeyCode::Esc));
    assert!(app.help.is_none(), "esc closes the help overlay");
    assert!(
        matches!(app.view, View::Dashboard),
        "stays on the dashboard"
    );
}

#[test]
fn editor_question_mark_opens_contextual_help_without_leaving() {
    let (mut app, _harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.view, View::Editor(_)));
    app.handle_key(key(KeyCode::Char('?')));
    assert!(
        app.help.is_some(),
        "\"?\" in the editor opens the contextual help"
    );
    app.handle_key(key(KeyCode::Char('q')));
    assert!(!app.should_quit, "q is swallowed inside the help overlay");
    assert!(app.help.is_some(), "q does not close the help overlay");
    app.handle_key(key(KeyCode::Esc));
    assert!(app.help.is_none());
    assert!(
        matches!(app.view, View::Editor(_)),
        "closing the help returns to the editor, not the dashboard"
    );
}

#[test]
fn help_overlay_per_key_does_not_typo_into_editor_sections() {
    let (mut app, _harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Char('?')));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.help.is_none(), "enter closes the help overlay");
    assert!(
        matches!(app.view, View::Editor(_)),
        "enter on an open help must not open the save flow"
    );
}

#[test]
fn confirmed_delete_removes_profile_and_state() {
    let (mut app, harness) = harness(&[profile("gone")]);
    std::fs::create_dir_all(app.paths.logs_dir("gone")).unwrap();

    app.handle_key(key(KeyCode::Char('d')));
    assert!(app.confirm.is_some(), "delete opens the overlay");
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Enter));
    settle(&mut app);

    let actions = harness.action_texts();
    assert!(
        actions.iter().any(|text| text.contains("deleted gone")),
        "delete reports as an action: {:?}",
        actions
    );
    assert!(
        actions.iter().any(|text| text.contains("recreate with n")),
        "delete points at the way back: {:?}",
        actions
    );
    assert!(matches!(app.view, View::Dashboard));
    let saved = crate::config::load(&app.paths).unwrap();
    assert!(saved.profiles.is_empty());
    assert!(!app.paths.logs_dir("gone").exists());
}

#[test]
fn run_now_shows_loading_then_starts_the_service_through_the_manager() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    match &app.view {
        View::Logs(state) => {
            assert!(state.follow);
            assert_eq!(state.profile, "all-daily");
        }
        _ => panic!("run now opens the live view immediately, before the start finishes"),
    }
    assert!(
        app.in_flight.is_some(),
        "run defers the start behind busy feedback"
    );
    assert!(
        app.in_flight
            .as_ref()
            .is_some_and(|job| job.label.contains("starting all-daily")),
        "the busy indicator carries the running label: {:?}",
        app.in_flight.as_ref().map(|job| job.label.clone())
    );
    settle(&mut app);
    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("started all-daily")),
        "run now is recorded in the activity log: {:?}",
        harness.action_texts()
    );
    assert!(
        harness
            .calls
            .lock()
            .unwrap()
            .contains(&"start:all-daily".to_string())
    );
    assert!(app.rows[0].running, "the row reflects the live service");
    match &app.view {
        View::Logs(state) => {
            assert!(state.follow);
            assert_eq!(state.profile, "all-daily");
        }
        _ => panic!("run now should open the live view"),
    }
}

fn write_status(paths: &crate::paths::Paths, name: &str, success: bool, exit: i32) {
    write_status_variant(paths, name, success, exit, false)
}

fn write_status_variant(
    paths: &crate::paths::Paths,
    name: &str,
    success: bool,
    exit: i32,
    skipped: bool,
) {
    let started = chrono::Utc::now();
    let outcome = crate::runner::RunOutcome {
        profile: name.to_string(),
        dry_run: false,
        skipped,
        success,
        exit_code: Some(exit),
        started_at: started,
        finished_at: chrono::Utc::now(),
        duration_secs: 2.0,
        log_path: paths.logs_dir(name).join("t.log"),
    };
    std::fs::create_dir_all(paths.status_file(name).parent().unwrap()).unwrap();
    std::fs::write(
        paths.status_file(name),
        serde_json::to_string(&outcome).unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(paths.logs_dir(name)).unwrap();
    std::fs::write(paths.logs_dir(name).join("t.log"), "done").unwrap();
}

#[test]
fn live_snapshot_gathers_running_and_status_in_one_read() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    write_status(&app.paths, "all-daily", true, 0);

    let snap = app.live_snapshot("all-daily");
    assert!(snap.running);
    assert!(snap.running_since.is_some());
    assert!(snap.status.as_ref().is_some_and(|outcome| outcome.success));
    assert_eq!(snap.name, "all-daily");
    drop(harness);
}

#[test]
fn live_view_escape_returns_and_run_keeps_going() {
    let (mut app, _harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    app.handle_key(key(KeyCode::Esc));
    assert!(matches!(app.view, View::Dashboard));
    assert!(
        app.rows[0].running,
        "leaving the live view keeps the run alive"
    );
}

#[test]
fn live_view_follow_ignores_the_stop_key() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    app.handle_key(key(KeyCode::Char('x')));
    assert!(
        matches!(app.view, View::Logs(_)),
        "x keeps the live view open"
    );
    assert!(app.rows[0].running, "the run was not stopped");
    assert!(
        !harness
            .calls
            .lock()
            .unwrap()
            .contains(&"stop:all-daily".to_string()),
        "x no longer reaches systemd: {:?}",
        harness.calls.lock().unwrap()
    );
    drop(harness);
}

#[test]
fn tick_reports_when_a_followed_run_finishes_ok() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    harness.services.lock().unwrap().remove("all-daily");
    write_status(&app.paths, "all-daily", true, 0);
    app.on_tick();
    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("all-daily finished ok")),
        "a finished run records as an action: {:?}",
        harness.action_texts()
    );
    assert!(!app.rows[0].running);
    match &app.view {
        View::Logs(state) => assert!(state.follow_header.contains("finished ok")),
        _ => panic!("view should stay live"),
    }
}

#[test]
fn run_now_on_a_running_profile_joins_the_live_view_without_a_second_start() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    app.handle_key(key(KeyCode::Esc));
    app.handle_key(key(KeyCode::Char('r')));
    let starts = harness
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|call| call.starts_with("start:"))
        .count();
    assert_eq!(starts, 1, "an already-running profile is not started twice");
    assert!(
        harness
            .action_texts()
            .iter()
            .any(|text| text.contains("already running")),
        "the absorbed run says so in the activity log: {:?}",
        harness.action_texts()
    );
    assert!(
        matches!(&app.view, View::Logs(state) if state.follow),
        "pressing r on a running row opens the live view"
    );
}

#[test]
fn skipped_outcomes_render_as_skipped_not_failed() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    harness.services.lock().unwrap().remove("all-daily");
    write_status_variant(&app.paths, "all-daily", false, 1, true);
    app.on_tick();
    match &app.view {
        View::Logs(state) => {
            assert!(
                state.follow_header.contains("skipped"),
                "a skipped outcome must not read as a failure: {}",
                state.follow_header
            );
            assert!(!state.follow_header.contains("FAILED"));
        }
        _ => panic!("view should stay live"),
    }
}

#[test]
fn picker_key_transitions_are_pure_decisions() {
    let (mut app, _harness) = harness(&[]);
    for (index, expect_stay) in [(0usize, false), (1, false), (2, false), (3, true)] {
        let t = app.picker_key(index, key(KeyCode::Enter));
        assert!(
            matches!(t, Transition::Stay(view) if matches!(view, View::Editor(_))) == expect_stay,
            "custom entry opens the editor, presets activate and leave"
        );
    }
    assert!(matches!(
        app.picker_key(0, key(KeyCode::Esc)),
        Transition::Leave
    ));
    assert!(matches!(
        app.picker_key(1, key(KeyCode::Down)),
        Transition::Stay(View::PresetPicker { index: 2 })
    ));
}

#[test]
fn live_view_auto_returns_after_the_run_finishes() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    assert!(matches!(&app.view, View::Logs(state) if state.follow));

    harness.services.lock().unwrap().remove("all-daily");
    write_status(&app.paths, "all-daily", true, 0);
    app.on_tick();
    assert!(
        matches!(&app.view, View::Logs(state) if state.auto_close.is_some()),
        "a fresh finish arms the linger instead of closing instantly"
    );

    for _ in 0..FOLLOW_LINGER_TICKS + 2 {
        app.on_tick();
    }
    assert!(
        matches!(app.view, View::Dashboard),
        "the live view returns to the dashboard once the linger expires"
    );
}

#[test]
fn pressing_a_key_while_lingering_cancels_the_auto_return() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    harness.services.lock().unwrap().remove("all-daily");
    write_status(&app.paths, "all-daily", false, 1);
    app.on_tick();
    app.handle_key(key(KeyCode::Up));
    for _ in 0..FOLLOW_LINGER_TICKS + 2 {
        app.on_tick();
    }
    assert!(
        matches!(&app.view, View::Logs(state) if state.follow),
        "a key press cancels the auto close so the user can keep reading"
    );
}

#[test]
fn stale_status_from_before_the_run_does_not_pose_as_its_result() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    write_status(&app.paths, "all-daily", false, 9);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    harness.services.lock().unwrap().remove("all-daily");
    app.on_tick();
    match &app.view {
        View::Logs(state) => {
            assert!(
                state.follow_header.contains("not running"),
                "a status older than the run start is not its result: {}",
                state.follow_header
            );
            assert!(!state.follow_header.contains("FAILED"));
        }
        _ => panic!("view should stay live"),
    }
}

#[test]
fn tick_reports_a_failed_run_with_its_exit_code() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    harness.services.lock().unwrap().remove("all-daily");
    write_status(&app.paths, "all-daily", false, 3);
    app.on_tick();
    assert!(app.message.contains("FAILED (exit 3)"));
    assert!(
        harness
            .all_texts()
            .iter()
            .any(|text| text.contains("FAILED (exit 3)")),
        "a failed run is recorded in the activity log too: {:?}",
        harness.all_texts()
    );
}

#[test]
fn tick_without_changes_does_not_resend_the_finish_message() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    harness.services.lock().unwrap().remove("all-daily");
    app.on_tick();
    let count = |all: &[String]| all.iter().filter(|t| t.contains("finished ok")).count();
    let first = count(&harness.action_texts());
    app.on_tick();
    app.on_tick();
    assert_eq!(
        count(&harness.action_texts()),
        first,
        "the transition fires once, not on every tick"
    );
}

#[test]
fn logs_key_opens_the_logs_view_for_the_selection() {
    let (mut app, _harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('l')));
    assert!(matches!(app.view, View::Logs(_)));
    app.handle_key(key(KeyCode::Esc));
    assert!(matches!(app.view, View::Dashboard));
}

#[test]
fn error_messages_stick_until_success_replaces_them() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    app.set_error_message("start failed: boom".to_string());
    for _ in 0..MESSAGE_TTL_TICKS + 10 {
        app.on_tick();
    }
    assert!(
        !app.message.is_empty() && app.message.contains("start failed"),
        "errors do not expire like info: {}",
        app.message
    );

    app.handle_key(key(KeyCode::Char('r')));
    settle(&mut app);
    assert!(
        app.message.is_empty(),
        "a successful job clears the sticky error: {:?}",
        app.message
    );
    drop(harness);
}

#[test]
fn quitting_is_blocked_while_the_dashboard_filter_is_active() {
    let (mut app, _harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(key(KeyCode::Char('q')));
    assert!(!app.should_quit, "q must type into the filter, not quit");
    app.handle_key(key(KeyCode::Esc));
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn loading_stays_visible_for_the_minimum_duration_before_clearing() {
    let (mut app, _harness) = harness(&[]);
    let shared = std::sync::Arc::new(std::sync::Mutex::new(Some(JobOutcome::Success(
        "done".into(),
    ))));
    app.in_flight = Some(BackgroundJob {
        label: "saving x".to_string(),
        started: std::time::Instant::now(),
        shared,
    });
    assert!(
        !app.poll_in_flight(),
        "a too-quick job is not finalizable yet"
    );
    assert!(
        app.in_flight.is_some(),
        "the spinner stays up for the minimum perceivable duration"
    );
    app.in_flight.as_mut().unwrap().started =
        std::time::Instant::now() - std::time::Duration::from_secs(1);
    assert!(
        app.poll_in_flight(),
        "the spinner clears after the minimum duration"
    );
    assert!(app.in_flight.is_none());
}

fn mouse(
    kind: crossterm::event::MouseEventKind,
    column: u16,
    row: u16,
) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

#[test]
fn dashboard_l_toggles_the_activity_panel() {
    let (mut app, _harness) = harness(&[]);
    assert!(!app.activity_panel);
    app.handle_key(key(KeyCode::Char('L')));
    assert!(app.activity_panel, "L toggles the panel on");
    app.handle_key(key(KeyCode::Char('L')));
    assert!(!app.activity_panel, "L toggles the panel off again");
}

#[test]
fn toggle_activity_panel_resets_scroll() {
    let (mut app, _harness) = harness(&[]);
    app.activity_scroll = 5;
    app.handle_key(key(KeyCode::Char('L')));
    assert_eq!(
        app.activity_scroll, 0,
        "opening the panel resets scroll to newest"
    );
}

#[test]
fn ctrl_c_quits_from_anywhere_including_active_filters() {
    let (mut app, _harness) = harness(&[profile("all-daily")]);

    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(
        app.should_quit,
        "ctrl+c kills even while the filter swallows plain letters"
    );

    let (mut app, _harness) = harness(&[profile("all-daily")]);
    app.view = View::Editor(Box::new(crate::tui::editor::EditorState::new(
        None,
        Vec::new(),
        crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
    )));
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.should_quit, "ctrl+c kills from the editor");
}

#[test]
fn plain_c_without_control_types_normally() {
    let (mut app, _harness) = harness(&[profile("all-daily")]);
    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(key(KeyCode::Char('c')));
    assert!(!app.should_quit);
    assert_eq!(app.filter.text(), "c", "plain c is just a letter");
}

#[test]
fn dashboard_arrows_move_the_selection_while_typing_a_filter() {
    let (mut app, _harness) = harness(&[profile("all-daily"), profile("dev-tools")]);
    app.handle_key(key(KeyCode::Char('/')));
    assert!(app.filter.active);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.selected, 1, "down moves the selection mid-filter");
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.selected, 0, "up moves it back");
    app.handle_key(key(KeyCode::Char('a')));
    assert_eq!(app.filter.text(), "a", "regular keys still type");
}

#[test]
fn dashboard_esc_closes_the_activity_panel_before_engaging_filter() {
    let (mut app, _harness) = harness(&[]);
    app.activity_panel = true;
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.activity_panel, "esc closes the activity panel");
    app.handle_key(key(KeyCode::Char('/')));
    assert!(app.filter.active, "filter is open after esc-closed panel");
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.filter.active);
}

#[test]
fn mouse_wheel_inside_the_activity_panel_scrolls_it() {
    let (mut app, harness) = harness(&[]);
    app.activity_panel = true;
    app.activity_area
        .set(ratatui::layout::Rect::new(0, 10, 80, 5));
    harness
        .activity
        .lock()
        .unwrap()
        .log(ActivityKind::Action, "one");
    harness
        .activity
        .lock()
        .unwrap()
        .log(ActivityKind::Action, "two");
    assert_eq!(
        app.activity_scroll, 0,
        "panel starts showing the newest entries"
    );
    app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollDown, 5, 12));
    assert_eq!(app.activity_scroll, 1, "wheel down moves one entry back");
    app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollUp, 5, 12));
    assert_eq!(app.activity_scroll, 0, "wheel up scrolls forward again");
}

#[test]
fn activity_scroll_clamps_to_entries_minus_one() {
    let (mut app, harness) = harness(&[]);
    let baseline = harness.activity.lock().unwrap().len();
    app.activity_panel = true;
    app.activity_area
        .set(ratatui::layout::Rect::new(0, 10, 80, 5));
    harness
        .activity
        .lock()
        .unwrap()
        .log(ActivityKind::Action, "only");
    assert_eq!(harness.activity.lock().unwrap().len(), baseline + 1);
    app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollDown, 5, 12));
    assert_eq!(
        app.activity_scroll,
        app.activity.lock().unwrap().len().saturating_sub(1),
        "clamp prevents scrolling past the earliest entry"
    );
}

#[test]
fn cancel_feedback_only_when_something_was_lost() {
    assert_eq!(cancel_message(false), "");
    assert_eq!(cancel_message(true), "editor closed — changes discarded");
}

#[test]
fn console_cancel_records_a_log_entry_when_edits_were_dropped() {
    let (mut app, harness) = harness(&[profile("all-daily")]);
    let entries_before = harness.all_texts().len();
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(
        harness.all_texts().len(),
        entries_before,
        "backing out of a clean editor logs nothing"
    );
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Char(' ')));
    app.handle_key(key(KeyCode::Esc));
    assert!(
        matches!(&app.view, View::Editor(state) if state.unsaved.is_some()),
        "esc on a dirty editor asks instead of leaving"
    );
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert!(
        harness
            .all_texts()
            .iter()
            .any(|text| text.contains("changes discarded")),
        "choosing Discard leaves and lands in the activity log"
    );
}

#[test]
fn command_sink_forwards_lines_into_the_activity_log() {
    let activity = Arc::new(Mutex::new(ActivityLog::in_memory(16)));
    let sink = command_sink(&activity);
    sink("systemctl --user enable --now topmatic@all-daily.timer");
    let log = activity.lock().unwrap();
    let entry = log.tail().expect("the sink records the command");
    assert_eq!(entry.kind, ActivityKind::Command);
    assert_eq!(
        entry.text,
        "systemctl --user enable --now topmatic@all-daily.timer"
    );
}
