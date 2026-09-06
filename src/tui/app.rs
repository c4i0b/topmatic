use std::cell::Cell;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use ratatui::layout::Rect;

use crate::config::{self, AppConfig};
use crate::paths::Paths;
use crate::runner::{read_status, resolve};
use crate::systemd::{RealSystemdCtl, SystemdCtl, sync as systemd_sync};

use super::View;
use super::dashboard;
use super::editor;
use super::input::{FilterState, LineEdit};
use super::logs;
use super::presets;

pub struct App {
    pub config: AppConfig,
    pub paths: Paths,
    pub ctl: Box<dyn SystemdCtl>,
    pub topmatic_bin: PathBuf,
    pub topgrade_bin: Option<PathBuf>,
    pub catalog: Vec<String>,
    pub view: View,
    pub selected: usize,
    pub list_scroll: u16,
    pub rows: Vec<dashboard::ProfileRow>,
    pub filter: FilterState,
    pub list_area: Cell<Rect>,
    pub message: String,
    pub should_quit: bool,
}

impl App {
    pub fn boot() -> anyhow::Result<Self> {
        let paths = Paths::from_env();
        let (config, issues) = config::load_validated(&paths)?;
        let _ = config::write_example_if_changed(&paths);
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
        let ctl: Box<dyn SystemdCtl> = Box::new(RealSystemdCtl::new(home));
        let topmatic_bin = std::env::current_exe()?;
        let path_env = std::env::var("PATH").unwrap_or_default();
        let topgrade_bin = resolve::find_in_path("topgrade", &path_env);

        let mut app = Self::assemble(config, issues, paths, ctl, topmatic_bin, topgrade_bin);
        app.catalog = app.load_catalog();
        Ok(app)
    }

    pub fn assemble(
        config: AppConfig,
        issues: Vec<String>,
        paths: Paths,
        ctl: Box<dyn SystemdCtl>,
        topmatic_bin: PathBuf,
        topgrade_bin: Option<PathBuf>,
    ) -> Self {
        let mut app = Self {
            config,
            paths,
            ctl,
            topmatic_bin,
            topgrade_bin,
            catalog: Vec::new(),
            view: View::Dashboard,
            selected: 0,
            list_scroll: 0,
            rows: Vec::new(),
            filter: FilterState::new(),
            list_area: Cell::new(Rect::default()),
            message: String::new(),
            should_quit: false,
        };
        let report = systemd_sync::sync(&app.config, &app.topmatic_bin, app.ctl.as_ref());
        app.message = if report.errors.is_empty() {
            format!(
                "synced{}{}{}{}",
                if report.templates_installed {
                    ", installed unit templates"
                } else {
                    ""
                },
                if report.pruned_drop_ins.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", pruned {} stray drop-in file(s)",
                        report.pruned_drop_ins.len()
                    )
                },
                if report.removed_orphans.is_empty() {
                    String::new()
                } else {
                    format!(", removed {} orphan(s)", report.removed_orphans.len())
                },
                if report.ignored_foreign.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", left {} foreign timer(s) untouched",
                        report.ignored_foreign.len()
                    )
                }
            )
        } else {
            format!("sync errors: {}", report.errors.join("; "))
        };
        if !issues.is_empty() {
            app.message = format!(
                "skipped {} invalid profile(s); run topmatic doctor — {}",
                issues.len(),
                app.message
            );
        }
        if app.topgrade_bin.is_none() {
            app.message =
                "warning: topgrade not found in PATH (cargo install topgrade)".to_string();
        }
        app.rebuild_rows();
        app
    }

    fn load_catalog(&self) -> Vec<String> {
        if let Some(bin) = &self.topgrade_bin
            && let Ok(output) = std::process::Command::new(bin).arg("--help").output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            return crate::domain::steps::catalog(&text);
        }
        presets::fallback_catalog()
    }

    pub fn rebuild_rows(&mut self) {
        self.rows = self
            .config
            .profiles
            .iter()
            .map(|profile| dashboard::ProfileRow {
                name: profile.name.clone(),
                schedule: profile.schedule.summary(),
                next_run: self.ctl.next_run(&profile.name),
                status: read_status(&self.paths, &profile.name).ok().flatten(),
                timer_active: self.ctl.timer_active(&profile.name),
            })
            .collect();
        self.clamp_selection();
    }

    pub fn visible_rows(&self) -> Vec<&dashboard::ProfileRow> {
        self.rows
            .iter()
            .filter(|row| self.filter.matches(&row.name))
            .collect()
    }

    pub fn filtering(&self) -> bool {
        self.filter.is_engaged()
    }

    pub fn filter_text(&self) -> &str {
        self.filter.text()
    }

    pub fn selected_profile(
        &self,
    ) -> Option<(&crate::domain::profile::Profile, &dashboard::ProfileRow)> {
        let visible = self.visible_rows();
        let row = visible.get(self.selected)?;
        let profile = self.config.profile(&row.name)?;
        Some((profile, row))
    }

    fn selected_row(&self) -> Option<&dashboard::ProfileRow> {
        self.visible_rows().into_iter().nth(self.selected)
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_rows().len();
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
        let start = self.list_scroll as usize;
        if self.selected < start {
            self.list_scroll = self.selected as u16;
        } else if start.saturating_add(super::LIST_VISIBLE) <= self.selected {
            self.list_scroll = (self.selected + 1 - super::LIST_VISIBLE) as u16;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let quits = matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'));
        let editing_dashboard_filter = matches!(self.view, View::Dashboard) && self.filter.active;
        if quits && !editing_dashboard_filter {
            match &self.view {
                View::Dashboard
                | View::Help
                | View::Confirm { .. }
                | View::Logs(_)
                | View::PresetPicker { .. } => {
                    self.should_quit = true;
                    return;
                }
                View::Editor(_) => {}
            }
        }
        match std::mem::replace(&mut self.view, View::Dashboard) {
            View::Dashboard => self.handle_dashboard_key(key),
            View::PresetPicker { index } => {
                let count = presets::PRESETS.len() + 1;
                match key.code {
                    KeyCode::Char('j' | 'J') | KeyCode::Down => {
                        self.view = View::PresetPicker {
                            index: (index + 1).min(count - 1),
                        };
                    }
                    KeyCode::Char('k' | 'K') | KeyCode::Up => {
                        self.view = View::PresetPicker {
                            index: index.saturating_sub(1),
                        };
                    }
                    KeyCode::Esc => self.view = View::Dashboard,
                    KeyCode::Enter => {
                        let jitter = self.config.defaults.resolved().random_delay.as_secs();
                        let editor = if index < presets::PRESETS.len() {
                            let preset = &presets::PRESETS[index];
                            let steps = presets::steps_for(index, &self.catalog);
                            editor::EditorState::from_preset(
                                self.catalog.clone(),
                                steps,
                                preset.suggested_name,
                                jitter,
                            )
                        } else {
                            editor::EditorState::new(None, self.catalog.clone(), jitter)
                        };
                        self.view = View::Editor(Box::new(editor));
                    }
                    _ => {}
                }
            }
            View::Editor(mut state) => match state.handle_key(key) {
                editor::EditorEvent::Cancel => {
                    self.message = cancel_message(state.is_dirty());
                }
                editor::EditorEvent::RequestSave => self.save_profile(*state),
                editor::EditorEvent::Quit => self.should_quit = true,
                editor::EditorEvent::None => self.view = View::Editor(state),
            },
            View::Logs(mut state) => {
                if state.handle_key(key) {
                    self.rebuild_rows();
                } else {
                    self.view = View::Logs(state);
                }
            }
            View::Help => {}
            View::Confirm { profile } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => self.delete_profile(&profile),
                _ => {}
            },
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) {
        if self.filter.active {
            self.filter.handle(key);
            self.clamp_selection();
            return;
        }
        match key.code {
            KeyCode::Char('j' | 'J') | KeyCode::Down => {
                if self.selected + 1 < self.visible_rows().len() {
                    self.selected += 1;
                }
                self.clamp_selection();
            }
            KeyCode::Char('k' | 'K') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.clamp_selection();
            }
            KeyCode::Enter => self.open_editor_for_selected(),
            KeyCode::Esc => {
                if self.filter.is_engaged() {
                    self.filter.edit = LineEdit::new(String::new());
                    self.selected = 0;
                    self.list_scroll = 0;
                }
            }
            KeyCode::Char(c) => match c.to_ascii_lowercase() {
                '/' => self.filter.start(),
                '?' => self.view = View::Help,
                'n' => self.view = View::PresetPicker { index: 0 },
                'e' => self.open_editor_for_selected(),
                'd' => {
                    if let Some(name) = self.selected_row().map(|row| row.name.clone()) {
                        self.view = View::Confirm { profile: name };
                    }
                }
                'r' => self.run_now(),
                'l' => self.open_logs(),
                _ => {}
            },
            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        let list = self.list_area.get();
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.selected = self.selected.saturating_sub(1);
                self.clamp_selection();
            }
            MouseEventKind::ScrollDown => {
                if self.selected + 1 < self.visible_rows().len() {
                    self.selected += 1;
                }
                self.clamp_selection();
            }
            MouseEventKind::Down(MouseButton::Left)
                if mouse.column >= list.x
                    && mouse.column < list.x + list.width
                    && mouse.row > list.y
                    && mouse.row < list.y + list.height.saturating_sub(1) =>
            {
                let index = (mouse.row - list.y - 1) as usize;
                if index < self.visible_rows().len() {
                    self.selected = index;
                }
            }
            _ => {}
        }
    }

    fn open_editor_for_selected(&mut self) {
        let profile = self
            .selected_row()
            .and_then(|row| self.config.profile(&row.name).cloned());
        if let Some(profile) = profile {
            let jitter = self.config.defaults.resolved().random_delay.as_secs();
            self.view = View::Editor(Box::new(editor::EditorState::new(
                Some(&profile),
                self.catalog.clone(),
                jitter,
            )));
        }
    }

    fn save_profile(&mut self, state: editor::EditorState) {
        let profile = match state.to_profile(&state.final_name()) {
            Ok(profile) => profile,
            Err(error) => {
                self.message = error;
                self.view = View::Editor(Box::new(state));
                return;
            }
        };
        if let Err(error) = editor::validate_draft(&profile) {
            self.message = error;
            self.view = View::Editor(Box::new(state));
            return;
        }
        let plan =
            match config::save_plan(&self.config, state.original_name.as_deref(), &profile.name) {
                Ok(plan) => plan,
                Err(error) => {
                    self.message = error;
                    self.view = View::Editor(Box::new(state));
                    return;
                }
            };
        if let config::SavePlan::RenameFrom(original) = &plan
            && self.config.remove(original)
        {
            let _ = crate::systemd::sync::purge_profile_state(&self.paths, original);
        }
        self.config.upsert(profile.clone());
        if let Err(error) = config::save(&self.paths, &self.config) {
            self.message = format!("save failed: {error}");
            self.view = View::Editor(Box::new(state));
            return;
        }
        let report = systemd_sync::sync(&self.config, &self.topmatic_bin, self.ctl.as_ref());
        if !report.errors.is_empty() {
            self.message = format!("sync errors: {}", report.errors.join("; "));
        } else {
            self.message = format!("saved {}", profile.name);
        }
        self.rebuild_rows();
    }

    fn delete_profile(&mut self, name: &str) {
        self.config.remove(name);
        if let Err(error) = config::save(&self.paths, &self.config) {
            self.message = format!("save failed: {error}");
        }
        let _ = crate::systemd::sync::purge_profile_state(&self.paths, name);
        let _ = systemd_sync::sync(&self.config, &self.topmatic_bin, self.ctl.as_ref());
        self.message = format!("deleted {name} (run history purged)");
        self.view = View::Dashboard;
        self.selected = 0;
        self.rebuild_rows();
    }

    fn run_now(&mut self) {
        let Some(name) = self.selected_row().map(|row| row.name.clone()) else {
            return;
        };
        match self.ctl.start_service(&name) {
            Ok(()) => self.message = format!("started {name} in the background"),
            Err(error) => self.message = format!("start failed: {error}"),
        }
    }

    fn open_logs(&mut self) {
        if let Some(name) = self.selected_row().map(|row| row.name.clone()) {
            self.view = View::Logs(logs::LogsState::open(&self.paths, &name));
        }
    }
}

pub(crate) fn cancel_message(dirty: bool) -> String {
    if dirty {
        "editor closed — changes discarded".to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::domain::profile::{NotifyPolicy, Profile, Scope};
    use crate::domain::schedule::Schedule;
    use crate::systemd::test_support::FakeCtl;
    use crate::tui::views;
    use std::sync::Arc;

    struct Harness {
        _tmp: tempfile::TempDir,
        calls: Arc<std::sync::Mutex<Vec<String>>>,
        unit_dir: PathBuf,
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
    }

    fn profile(name: &str) -> Profile {
        Profile {
            name: name.to_string(),
            steps: vec!["flatpak".to_string()],
            schedule: Schedule::default(),
            notify: NotifyPolicy::OnFailure,
            scope: Scope::User,
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
        let mut app = App::assemble(
            crate::config::load_validated(&paths).unwrap().0,
            Vec::new(),
            paths,
            Box::new(ctl),
            PathBuf::from("/bin/topmatic"),
            Some(PathBuf::from("/nonexistent/topgrade")),
        );
        app.catalog = presets::fallback_catalog();
        (
            app,
            Harness {
                _tmp: tmp,
                calls,
                unit_dir,
            },
        )
    }

    fn save_editor_profile(app: &mut App, prefill: &str, name: &str) {
        for _ in 0..3 {
            app.handle_key(key(KeyCode::Tab));
        }
        app.handle_key(key(KeyCode::Enter));
        for _ in 0..prefill.chars().count() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for character in name.chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));
    }

    #[test]
    fn assemble_syncs_timers_and_builds_rows() {
        let (app, harness) = harness(&[profile("all-daily")]);
        assert!(app.message.contains("synced"));
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
        let views: [(&str, View); 6] = [
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
            ("help", View::Help),
            (
                "confirm",
                View::Confirm {
                    profile: "all-daily".to_string(),
                },
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
    fn new_profile_flow_picks_preset_saves_and_converges() {
        let (mut app, harness) = harness(&[]);
        app.handle_key(key(KeyCode::Char('n')));
        assert!(matches!(app.view, View::PresetPicker { .. }));
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.view, View::Editor(_)));
        save_editor_profile(&mut app, "all-daily", "all-daily");

        assert!(app.message.contains("saved all-daily"));
        assert!(matches!(app.view, View::Dashboard));
        let saved = crate::config::load(&app.paths).unwrap();
        assert_eq!(saved.profiles.len(), 1);
        assert_eq!(saved.profiles[0].name, "all-daily");
        assert!(!saved.profiles[0].steps.is_empty());
        assert!(
            harness
                .calls
                .lock()
                .unwrap()
                .contains(&"enable:all-daily".to_string()),
            "saving converges the new timer"
        );
    }

    #[test]
    fn editing_renames_the_profile_and_purges_its_state() {
        let (mut app, _harness) = harness(&[profile("old")]);
        std::fs::create_dir_all(app.paths.logs_dir("old")).unwrap();
        std::fs::write(app.paths.logs_dir("old").join("run.log"), "log").unwrap();

        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.view, View::Editor(_)));
        save_editor_profile(&mut app, "old", "new");

        assert!(app.message.contains("saved new"));
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
    fn confirmed_delete_removes_profile_and_state() {
        let (mut app, _harness) = harness(&[profile("gone")]);
        std::fs::create_dir_all(app.paths.logs_dir("gone")).unwrap();

        app.handle_key(key(KeyCode::Char('d')));
        assert!(matches!(app.view, View::Confirm { .. }));
        app.handle_key(key(KeyCode::Char('y')));

        assert!(app.message.contains("deleted gone"));
        assert!(matches!(app.view, View::Dashboard));
        let saved = crate::config::load(&app.paths).unwrap();
        assert!(saved.profiles.is_empty());
        assert!(!app.paths.logs_dir("gone").exists());
    }

    #[test]
    fn run_now_starts_the_service_through_the_manager() {
        let (mut app, harness) = harness(&[profile("all-daily")]);
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.message.contains("started all-daily"));
        assert!(
            harness
                .calls
                .lock()
                .unwrap()
                .contains(&"start:all-daily".to_string())
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
    fn cancel_feedback_only_when_something_was_lost() {
        assert_eq!(cancel_message(false), "");
        assert_eq!(cancel_message(true), "editor closed — changes discarded");
    }
}
