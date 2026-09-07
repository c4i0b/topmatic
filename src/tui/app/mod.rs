use std::cell::Cell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use ratatui::layout::Rect;

use crate::activity::{ActivityKind, ActivityLog};
use crate::config::{self, AppConfig};
use crate::paths::Paths;
use crate::runner::resolve;
use crate::systemd::{RealSystemdCtl, SystemdCtl, sync as systemd_sync};

use super::View;
use super::dashboard;
use super::editor;
use super::input::{self, FilterState};
use super::logs;
use super::overlay::{Overlay, OverlayAction};
use super::presets;
use super::views;
#[cfg(test)]
use jobs::FOLLOW_LINGER_TICKS;
use jobs::MESSAGE_TTL_TICKS;

mod jobs;
mod runs;

pub use jobs::{BackgroundJob, JobOutcome};

pub struct App {
    pub config: AppConfig,
    pub paths: Paths,
    pub ctl: Box<dyn SystemdCtl>,
    controller_factory: Box<dyn Fn() -> Box<dyn SystemdCtl>>,
    pub topmatic_bin: PathBuf,
    pub topgrade_bin: Option<PathBuf>,
    pub catalog: Vec<String>,
    pub in_flight: Option<BackgroundJob>,
    pub view: View,
    pub selected: usize,
    pub list_scroll: u16,
    pub rows: Vec<dashboard::ProfileRow>,
    pub filter: FilterState,
    pub list_area: Cell<Rect>,
    pub message: String,
    pub seen_message: String,
    pub message_expires_at_tick: u64,
    pub message_is_error: bool,
    pub activity: Arc<Mutex<ActivityLog>>,
    pub activity_panel: bool,
    pub activity_scroll: usize,
    pub activity_area: Cell<Rect>,
    pub confirm: Option<(String, Overlay)>,
    pub help: Option<Overlay>,
    pub should_quit: bool,
    pub tick: u64,
}

enum Transition {
    Stay(View),
    Leave,
}

pub struct ResolvedBins {
    topmatic_bin: PathBuf,
    topgrade_bin: Option<PathBuf>,
}

impl ResolvedBins {
    pub fn new(topmatic_bin: PathBuf, topgrade_bin: Option<PathBuf>) -> Self {
        Self {
            topmatic_bin,
            topgrade_bin,
        }
    }
}

impl App {
    pub fn boot() -> anyhow::Result<Self> {
        let paths = Paths::from_env();
        let (config, issues) = config::load_validated(&paths)?;
        let _ = config::write_example_if_changed(&paths);
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
        let activity = Arc::new(Mutex::new(ActivityLog::with_file(
            crate::activity::DEFAULT_CAPACITY,
            paths.activity_file(),
        )));
        let ctl: Box<dyn SystemdCtl> = Box::new(RealSystemdCtl::with_sink(
            home.clone(),
            command_sink(&activity),
        ));
        let controller_factory: Box<dyn Fn() -> Box<dyn SystemdCtl>> = {
            let activity = Arc::clone(&activity);
            Box::new(move || {
                Box::new(RealSystemdCtl::with_sink(
                    home.clone(),
                    command_sink(&activity),
                ))
            })
        };
        let topmatic_bin = std::env::current_exe()?;
        let path_env = std::env::var("PATH").unwrap_or_default();
        let topgrade_bin = resolve::find_in_path("topgrade", &path_env);

        let mut app = Self::assemble(
            config,
            issues,
            paths,
            ctl,
            controller_factory,
            activity,
            ResolvedBins {
                topmatic_bin,
                topgrade_bin,
            },
        );
        app.catalog = app.load_catalog();
        Ok(app)
    }

    pub fn assemble(
        config: AppConfig,
        issues: Vec<String>,
        paths: Paths,
        ctl: Box<dyn SystemdCtl>,
        controller_factory: Box<dyn Fn() -> Box<dyn SystemdCtl>>,
        activity: Arc<Mutex<ActivityLog>>,
        bins: ResolvedBins,
    ) -> Self {
        let topmatic_bin = bins.topmatic_bin;
        let topgrade_bin = bins.topgrade_bin;
        let mut app = Self {
            config,
            paths,
            ctl,
            controller_factory,
            topmatic_bin,
            topgrade_bin,
            catalog: Vec::new(),
            in_flight: None,
            view: View::Dashboard,
            selected: 0,
            list_scroll: 0,
            rows: Vec::new(),
            filter: FilterState::new(),
            list_area: Cell::new(Rect::default()),
            message: String::new(),
            seen_message: String::new(),
            message_expires_at_tick: 0,
            message_is_error: false,
            activity,
            activity_panel: false,
            activity_scroll: 0,
            activity_area: Cell::new(Rect::default()),
            confirm: None,
            help: None,
            should_quit: false,
            tick: 0,
        };
        let report = systemd_sync::sync(&app.config, &app.topmatic_bin, app.ctl.as_ref());
        if !report.errors.is_empty() {
            let detail = format!("sync errors: {}", report.errors.join("; "));
            app.set_error_message(detail.clone());
            app.log(ActivityKind::Error, detail);
        }
        let synced_text = format!(
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
        );
        app.log(ActivityKind::Action, synced_text.clone());
        if !issues.is_empty() {
            app.message = format!(
                "skipped {} invalid profile(s); run topmatic doctor — {}",
                issues.len(),
                synced_text
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

    pub fn log(&mut self, kind: ActivityKind, text: impl Into<String>) {
        self.activity.lock().unwrap().log(kind, text.into());
    }

    pub fn set_editor_layout(&mut self, width: u16, height: u16) {
        if let View::Editor(state) = &mut self.view {
            let rows = (height as usize).saturating_sub(13).max(1);
            state.set_steps_columns(state.steps_columns_for(width, rows));
        }
    }

    pub fn on_tick(&mut self) {
        self.poll_in_flight();
        if self.message != self.seen_message {
            self.seen_message = self.message.clone();
            self.message_expires_at_tick = self.tick + MESSAGE_TTL_TICKS;
        } else if !self.message.is_empty()
            && !self.message_is_error
            && self.tick >= self.message_expires_at_tick
        {
            self.message.clear();
            self.seen_message.clear();
        }
        self.tick = self.tick.wrapping_add(1);
        let follow_open = matches!(&self.view, View::Logs(state) if state.follow);
        let any_running = self.rows.iter().any(|row| row.running);
        if follow_open || any_running || self.tick.is_multiple_of(30) {
            self.refresh_live_state();
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
        if matches!(key.code, KeyCode::Char('c' | 'C'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.should_quit = true;
            return;
        }
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
        let taken = std::mem::replace(&mut self.view, View::Dashboard);
        self.view = match self.route(taken, key) {
            Transition::Stay(view) => view,
            Transition::Leave => View::Dashboard,
        };
    }

    fn route(&mut self, view: View, key: KeyEvent) -> Transition {
        match view {
            View::Dashboard => {
                self.handle_dashboard_key(key);
                let next = std::mem::replace(&mut self.view, View::Dashboard);
                Transition::Stay(next)
            }
            View::PresetPicker { index } => self.picker_key(index, key),
            View::Editor(state) => self.editor_key(state, key),
            View::Logs(state) => self.logs_key(state, key),
        }
    }

    fn picker_key(&mut self, index: usize, key: KeyEvent) -> Transition {
        let count = presets::PRESETS.len() + 1;
        match key.code {
            KeyCode::Char('j' | 'J') | KeyCode::Down => Transition::Stay(View::PresetPicker {
                index: (index + 1).min(count - 1),
            }),
            KeyCode::Char('k' | 'K') | KeyCode::Up => Transition::Stay(View::PresetPicker {
                index: index.saturating_sub(1),
            }),
            KeyCode::Esc => Transition::Leave,
            KeyCode::Enter => {
                if index < presets::PRESETS.len() {
                    self.activate_preset(index);
                    Transition::Leave
                } else {
                    let jitter = self.config.defaults.resolved().random_delay.as_secs();
                    Transition::Stay(View::Editor(Box::new(editor::EditorState::new(
                        None,
                        self.catalog.clone(),
                        jitter,
                    ))))
                }
            }
            _ => Transition::Stay(View::PresetPicker { index }),
        }
    }

    fn editor_key(&mut self, mut state: Box<editor::EditorState>, key: KeyEvent) -> Transition {
        match state.handle_key(key) {
            editor::EditorEvent::Cancel => {
                let discarded = cancel_message(state.is_dirty());
                if !discarded.is_empty() {
                    self.log(ActivityKind::Action, discarded);
                }
                Transition::Leave
            }
            editor::EditorEvent::RequestSave => {
                self.save_profile(*state);
                let next = std::mem::replace(&mut self.view, View::Dashboard);
                Transition::Stay(next)
            }
            editor::EditorEvent::Quit => {
                self.should_quit = true;
                Transition::Leave
            }
            editor::EditorEvent::Help => {
                self.help = Some(views::help_overlay("editor help", views::editor_help()));
                Transition::Stay(View::Editor(state))
            }
            editor::EditorEvent::None => Transition::Stay(View::Editor(state)),
        }
    }

    fn logs_key(&mut self, mut state: logs::LogsState, key: KeyEvent) -> Transition {
        if state.handle_key(key) {
            self.rebuild_rows();
            Transition::Leave
        } else {
            Transition::Stay(View::Logs(state))
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) {
        if self.filter.active {
            let len = self.visible_rows().len();
            input::handle_filter_typing(&mut self.filter, key, &mut self.selected, len, 1);
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
                if self.activity_panel {
                    self.activity_panel = false;
                } else if self.filter.is_engaged() {
                    self.filter.clear_query();
                    self.clamp_selection();
                }
            }
            KeyCode::Char('L') => self.toggle_activity_panel(),
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

    pub fn toggle_activity_panel(&mut self) {
        if self.activity_panel {
            self.activity_panel = false;
        } else {
            self.activity_panel = true;
            self.activity_scroll = 0;
        }
    }

    fn scroll_activity(&mut self, delta: usize) {
        let total = self.activity.lock().unwrap().len();
        let max = total.saturating_sub(1);
        self.activity_scroll = (self.activity_scroll + delta).min(max);
    }

    fn scroll_activity_back(&mut self, delta: usize) {
        self.activity_scroll = self.activity_scroll.saturating_sub(delta);
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.activity_panel {
            let area = self.activity_area.get();
            let over_panel = area.height > 0
                && mouse.column >= area.x
                && mouse.column < area.x + area.width
                && mouse.row >= area.y
                && mouse.row < area.y + area.height;
            if over_panel {
                match mouse.kind {
                    MouseEventKind::ScrollUp => self.scroll_activity_back(1),
                    MouseEventKind::ScrollDown => self.scroll_activity(1),
                    _ => {}
                }
                return;
            }
        }
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
                self.set_error_message(error);
                self.view = View::Editor(Box::new(state));
                return;
            }
        };
        if let Err(error) = editor::validate_draft(&profile) {
            self.set_error_message(error);
            self.view = View::Editor(Box::new(state));
            return;
        }
        let plan =
            match config::save_plan(&self.config, state.original_name.as_deref(), &profile.name) {
                Ok(plan) => plan,
                Err(error) => {
                    self.set_error_message(error);
                    let mut state = state;
                    state.reopen_name_popup();
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
            self.set_error_message(format!("save failed: {error}"));
            self.view = View::Editor(Box::new(state));
            return;
        }
        self.clear_error_message();
        let name = profile.name.clone();
        self.spawn_sync_job(format!("saving {name}"), move |report| {
            if report.errors.is_empty() {
                JobOutcome::Success(format!("saved {name}"))
            } else {
                JobOutcome::Error(format!("sync errors: {}", report.errors.join("; ")))
            }
        });
    }

    fn delete_profile(&mut self, name: &str) {
        self.config.remove(name);
        if let Err(error) = config::save(&self.paths, &self.config) {
            self.set_error_message(format!("save failed: {error}"));
        }
        let _ = crate::systemd::sync::purge_profile_state(&self.paths, name);
        let name = name.to_string();
        self.spawn_sync_job(format!("deleting {name}"), move |report| {
            if report.errors.is_empty() {
                JobOutcome::Success(format!(
                    "deleted {name} — recreate with n if that was a mistake (run history purged)"
                ))
            } else {
                JobOutcome::Error(format!("sync errors: {}", report.errors.join("; ")))
            }
        });
        self.view = View::Dashboard;
        self.selected = 0;
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

fn command_sink(activity: &Arc<Mutex<ActivityLog>>) -> Arc<dyn Fn(&str) + Send + Sync> {
    let activity = Arc::clone(activity);
    Arc::new(move |line| {
        activity
            .lock()
            .unwrap()
            .log(ActivityKind::Command, line.to_string())
    })
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod app_tests;
