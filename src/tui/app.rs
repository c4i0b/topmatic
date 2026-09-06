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
use super::overlay::{Overlay, OverlayAction};
use super::presets;
use super::views;

const MESSAGE_TTL_TICKS: u64 = 25;

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
    pub seen_message: String,
    pub message_expires_at_tick: u64,
    pub last_action: String,
    pub seen_last_action: String,
    pub last_action_expires_at_tick: u64,
    pub confirm: Option<(String, Overlay)>,
    pub help: Option<Overlay>,
    pub should_quit: bool,
    pub tick: u64,
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
            seen_message: String::new(),
            message_expires_at_tick: 0,
            last_action: String::new(),
            seen_last_action: String::new(),
            last_action_expires_at_tick: 0,
            confirm: None,
            help: None,
            should_quit: false,
            tick: 0,
        };
        let report = systemd_sync::sync(&app.config, &app.topmatic_bin, app.ctl.as_ref());
        app.message = if !report.errors.is_empty() {
            format!("sync errors: {}", report.errors.join("; "))
        } else {
            String::new()
        };
        app.set_last_action(format!(
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
        ));
        if !issues.is_empty() {
            app.message = format!(
                "skipped {} invalid profile(s); run topmatic doctor — {}",
                issues.len(),
                app.last_action
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

    pub fn live_snapshot(&self, name: &str) -> dashboard::LiveSnapshot {
        dashboard::LiveSnapshot {
            name: name.to_string(),
            running: self.ctl.service_active(name),
            running_since: self.ctl.service_since(name),
            status: read_status(&self.paths, name).ok().flatten(),
        }
    }

    pub fn rebuild_rows(&mut self) {
        self.rows = self
            .config
            .profiles
            .iter()
            .map(|profile| {
                let snap = self.live_snapshot(&profile.name);
                dashboard::ProfileRow {
                    name: snap.name.clone(),
                    schedule: profile.schedule.summary(),
                    next_run: self.ctl.next_run(&profile.name),
                    status: snap.status,
                    timer_active: self.ctl.timer_active(&profile.name),
                    running: snap.running,
                    running_since: snap.running_since,
                }
            })
            .collect();
        self.clamp_selection();
    }

    pub fn set_last_action(&mut self, action: impl Into<String>) {
        self.last_action = action.into();
    }

    pub fn on_tick(&mut self) {
        if self.message != self.seen_message {
            self.seen_message = self.message.clone();
            self.message_expires_at_tick = self.tick + MESSAGE_TTL_TICKS;
        } else if !self.message.is_empty() && self.tick >= self.message_expires_at_tick {
            self.message.clear();
            self.seen_message.clear();
        }
        if self.last_action != self.seen_last_action {
            self.seen_last_action = self.last_action.clone();
            self.last_action_expires_at_tick = self.tick + MESSAGE_TTL_TICKS;
        } else if !self.last_action.is_empty() && self.tick >= self.last_action_expires_at_tick {
            self.last_action.clear();
            self.seen_last_action.clear();
        }
        self.tick = self.tick.wrapping_add(1);
        let follow_open = matches!(&self.view, View::Logs(state) if state.follow);
        let any_running = self.rows.iter().any(|row| row.running);
        if follow_open || any_running || self.tick.is_multiple_of(30) {
            self.refresh_live_state();
        }
    }

    fn refresh_live_state(&mut self) {
        let previous: Vec<(String, Option<chrono::DateTime<chrono::Utc>>)> = self
            .rows
            .iter()
            .filter(|row| row.running)
            .map(|row| (row.name.clone(), row.running_since))
            .collect();
        self.rebuild_rows();
        for (name, since) in previous {
            if self.rows.iter().any(|row| row.name == name && row.running) {
                continue;
            }
            let status = match self.rows.iter().find(|row| row.name == name) {
                Some(row) => row.status.clone(),
                None => read_status(&self.paths, &name).ok().flatten(),
            };
            let status = status.as_ref();
            let finished_after_start = status
                .is_some_and(|outcome| since.is_some_and(|start| outcome.finished_at > start));
            match (status, finished_after_start) {
                (Some(outcome), true) if outcome.success => {
                    self.set_last_action(format!("{name} finished ok"));
                }
                (Some(outcome), true) => {
                    self.message =
                        format!("{name} FAILED (exit {:?})", outcome.exit_code.unwrap_or(1));
                }
                _ => self.set_last_action(format!("{name} stopped")),
            }
        }
        if let View::Logs(state) = &mut self.view
            && state.follow
        {
            let profile = state.profile.clone();
            let row = self.rows.iter().find(|row| row.name == profile);
            let running = row.is_some_and(|row| row.running);
            let status = match row.as_ref().map(|row| row.status.clone()) {
                Some(status) => status,
                None => read_status(&self.paths, &profile).ok().flatten(),
            };
            let elapsed = row
                .and_then(|row| row.running_since)
                .map(|since| dashboard::format_elapsed(chrono::Utc::now() - since))
                .unwrap_or_default();
            state.follow_header = if running {
                format!("running {profile} · {elapsed}")
            } else {
                match status.as_ref() {
                    Some(outcome) if outcome.success => format!("{profile} finished ok"),
                    Some(outcome) => format!(
                        "{profile} FAILED (exit {:?})",
                        outcome.exit_code.unwrap_or(1)
                    ),
                    None => format!("{profile} not running"),
                }
            };
            state.content = logs::tail(&self.paths, &profile, 24);
        }
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
        if let Some((profile, mut overlay)) = self.confirm.take() {
            match overlay.handle_key(key) {
                Some(OverlayAction::Selected(0)) => self.delete_profile(&profile),
                Some(OverlayAction::Selected(_)) | Some(OverlayAction::Cancelled) => {}
                None => self.confirm = Some((profile, overlay)),
            }
            return;
        }
        if let Some(mut overlay) = self.help.take() {
            match overlay.handle_key(key) {
                Some(OverlayAction::Selected(_)) | Some(OverlayAction::Cancelled) => {}
                None => self.help = Some(overlay),
            }
            return;
        }
        let quits = matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'));
        let editing_dashboard_filter = matches!(self.view, View::Dashboard) && self.filter.active;
        if quits && !editing_dashboard_filter {
            match &self.view {
                View::Dashboard | View::Logs(_) | View::PresetPicker { .. } => {
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
                    self.set_last_action(cancel_message(state.is_dirty()));
                }
                editor::EditorEvent::RequestSave => self.save_profile(*state),
                editor::EditorEvent::Quit => self.should_quit = true,
                editor::EditorEvent::Help => {
                    self.help = Some(views::help_overlay("editor help", views::editor_help()));
                    self.view = View::Editor(state);
                }
                editor::EditorEvent::None => self.view = View::Editor(state),
            },
            View::Logs(mut state) => {
                if state.follow && matches!(key.code, KeyCode::Char('x' | 'X')) {
                    let profile = state.profile.clone();
                    self.stop_run(&profile);
                    self.view = View::Logs(state);
                    return;
                }
                if state.handle_key(key) {
                    self.rebuild_rows();
                } else {
                    self.view = View::Logs(state);
                }
            }
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
                '?' => {
                    self.help = Some(views::help_overlay("help", views::dashboard_help()));
                }
                'n' => self.view = View::PresetPicker { index: 0 },
                'e' => self.open_editor_for_selected(),
                'd' => {
                    if let Some(name) = self.selected_row().map(|row| row.name.clone()) {
                        let overlay = Overlay::new(
                            "delete profile",
                            vec![
                                ratatui::text::Line::from(format!(
                                    "Delete {} and its run history?",
                                    name
                                )),
                                ratatui::text::Line::from("Timers and logs will be removed."),
                            ],
                            &["Delete", "Cancel"],
                            1,
                        );
                        self.confirm = Some((name, overlay));
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
            self.set_last_action(format!("saved {}", profile.name));
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
        self.set_last_action(format!(
            "deleted {name} — recreate with n if that was a mistake (run history purged)"
        ));
        self.view = View::Dashboard;
        self.selected = 0;
        self.rebuild_rows();
    }

    fn run_now(&mut self) {
        let Some(name) = self.selected_row().map(|row| row.name.clone()) else {
            return;
        };
        match self.ctl.start_service(&name) {
            Ok(()) => {
                self.set_last_action(format!("started {name}"));
                self.rebuild_rows();
                self.view = View::Logs(logs::LogsState::follow(&name));
            }
            Err(error) => self.message = format!("start failed: {error}"),
        }
    }

    fn stop_run(&mut self, profile: &str) {
        match self.ctl.stop_service(profile) {
            Ok(()) => self.set_last_action(format!("stopping {profile}…")),
            Err(error) => self.message = format!("stop failed: {error}"),
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
        services:
            Arc<std::sync::Mutex<std::collections::HashMap<String, chrono::DateTime<chrono::Utc>>>>,
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
        let services = ctl.shared_services();
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
                services,
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
        app.handle_key(key(KeyCode::Enter));
    }

    #[test]
    fn assemble_syncs_timers_and_builds_rows() {
        let (app, harness) = harness(&[profile("all-daily")]);
        assert!(app.last_action.contains("synced"));
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
    fn new_profile_flow_picks_preset_saves_and_converges() {
        let (mut app, harness) = harness(&[]);
        app.handle_key(key(KeyCode::Char('n')));
        assert!(matches!(app.view, View::PresetPicker { .. }));
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.view, View::Editor(_)));
        save_editor_profile(&mut app, "all-daily", "all-daily");

        assert!(app.last_action.contains("saved all-daily"));
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

        assert!(app.last_action.contains("saved new"));
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
    fn transient_feedback_expires_after_the_ttl() {
        let (mut app, harness) = harness(&[profile("all-daily")]);
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.last_action.contains("started all-daily"));
        app.on_tick();
        app.last_action_expires_at_tick = app.tick.saturating_sub(1);
        app.on_tick();
        assert!(app.last_action.is_empty(), "info feedback fades away");

        app.set_last_action("boom");
        app.message = "critical: something failed".to_string();
        app.on_tick();
        app.message_expires_at_tick = app.tick.saturating_sub(1);
        app.on_tick();
        assert!(app.message.is_empty(), "error feedback fades away too");
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
        let (mut app, _harness) = harness(&[profile("gone")]);
        std::fs::create_dir_all(app.paths.logs_dir("gone")).unwrap();

        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.confirm.is_some(), "delete opens the overlay");
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Enter));

        assert!(
            app.last_action.contains("deleted gone"),
            "delete reports as an action: {:?}",
            app.last_action
        );
        assert!(
            app.last_action.contains("recreate with n"),
            "delete points at the way back: {:?}",
            app.last_action
        );
        assert!(matches!(app.view, View::Dashboard));
        let saved = crate::config::load(&app.paths).unwrap();
        assert!(saved.profiles.is_empty());
        assert!(!app.paths.logs_dir("gone").exists());
    }

    #[test]
    fn run_now_starts_the_service_through_the_manager() {
        let (mut app, harness) = harness(&[profile("all-daily")]);
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.last_action.contains("started all-daily"));
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
        let started = chrono::Utc::now();
        let outcome = crate::runner::RunOutcome {
            profile: name.to_string(),
            dry_run: false,
            skipped: false,
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
        write_status(&app.paths, "all-daily", true, 0);

        let snap = app.live_snapshot("all-daily");
        assert!(snap.running);
        assert!(snap.running_since.is_some());
        assert!(snap.status.as_ref().is_some_and(|outcome| outcome.success));
        assert_eq!(snap.name, "all-daily");
        drop(harness);
    }

    #[test]
    fn live_view_backgrounds_with_escape_and_run_keeps_going() {
        let (mut app, _harness) = harness(&[profile("all-daily")]);
        app.handle_key(key(KeyCode::Char('r')));
        app.handle_key(key(KeyCode::Esc));
        assert!(matches!(app.view, View::Dashboard));
        assert!(app.rows[0].running, "backgrounding keeps the run alive");
    }

    #[test]
    fn live_view_x_stops_the_run_on_demand() {
        let (mut app, harness) = harness(&[profile("all-daily")]);
        app.handle_key(key(KeyCode::Char('r')));
        app.handle_key(key(KeyCode::Char('x')));
        assert!(app.last_action.contains("stopping all-daily"));
        assert!(
            harness
                .calls
                .lock()
                .unwrap()
                .contains(&"stop:all-daily".to_string())
        );
        app.on_tick();
        assert!(!app.rows[0].running);
    }

    #[test]
    fn tick_reports_when_a_followed_run_finishes_ok() {
        let (mut app, harness) = harness(&[profile("all-daily")]);
        app.handle_key(key(KeyCode::Char('r')));
        harness.services.lock().unwrap().remove("all-daily");
        write_status(&app.paths, "all-daily", true, 0);
        app.on_tick();
        assert!(app.last_action.contains("all-daily finished ok"));
        assert!(!app.rows[0].running);
        match &app.view {
            View::Logs(state) => assert!(state.follow_header.contains("finished ok")),
            _ => panic!("view should stay live"),
        }
    }

    #[test]
    fn tick_reports_a_failed_run_with_its_exit_code() {
        let (mut app, harness) = harness(&[profile("all-daily")]);
        app.handle_key(key(KeyCode::Char('r')));
        harness.services.lock().unwrap().remove("all-daily");
        write_status(&app.paths, "all-daily", false, 3);
        app.on_tick();
        assert!(app.message.contains("FAILED (exit 3)"));
    }

    #[test]
    fn tick_without_changes_does_not_resend_the_finish_message() {
        let (mut app, harness) = harness(&[profile("all-daily")]);
        app.handle_key(key(KeyCode::Char('r')));
        harness.services.lock().unwrap().remove("all-daily");
        app.on_tick();
        let first = app.last_action.clone();
        app.on_tick();
        app.on_tick();
        assert_eq!(
            app.last_action, first,
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
