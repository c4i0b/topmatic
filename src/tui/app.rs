use std::cell::Cell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use ratatui::layout::Rect;

use crate::activity::{ActivityKind, ActivityLog};
use crate::config::{self, AppConfig};
use crate::paths::Paths;
use crate::runner::{read_status, resolve};
use crate::systemd::{RealSystemdCtl, SystemdCtl, sync as systemd_sync};

use super::View;
use super::dashboard;
use super::editor;
use super::input::{self, FilterState};
use super::logs;
use super::overlay::{Overlay, OverlayAction};
use super::presets;
use super::views;
use crate::domain::profile::{NotifyPolicy, Profile};

const MESSAGE_TTL_TICKS: u64 = 25;
const MIN_LOADING_TIME: Duration = Duration::from_millis(120);

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

pub struct BackgroundJob {
    pub label: String,
    pub started: Instant,
    pub shared: Arc<Mutex<Option<JobOutcome>>>,
}

pub enum JobOutcome {
    Success(String),
    Error(String),
}

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

    pub fn log(&mut self, kind: ActivityKind, text: impl Into<String>) {
        self.activity.lock().unwrap().log(kind, text.into());
    }

    fn set_error_message(&mut self, text: String) {
        self.message = text;
        self.message_is_error = true;
    }

    fn clear_error_message(&mut self) {
        if self.message_is_error {
            self.message.clear();
            self.seen_message.clear();
            self.message_is_error = false;
        }
    }

    pub fn spawn_background(
        &mut self,
        label: impl Into<String>,
        work: impl FnOnce() -> JobOutcome + Send + 'static,
    ) {
        let shared = Arc::new(Mutex::new(None));
        let thread_shared = Arc::clone(&shared);
        std::thread::spawn(move || *thread_shared.lock().unwrap() = Some(work()));
        self.in_flight = Some(BackgroundJob {
            label: label.into(),
            started: Instant::now(),
            shared,
        });
    }

    fn spawn_sync_job(
        &mut self,
        label: String,
        on_report: impl FnOnce(&crate::systemd::sync::SyncReport) -> JobOutcome + Send + 'static,
    ) {
        let ctl = (self.controller_factory)();
        let config = self.config.clone();
        let bin = self.topmatic_bin.clone();
        self.spawn_background(label, move || {
            let report = systemd_sync::sync(&config, &bin, ctl.as_ref());
            on_report(&report)
        });
    }

    pub fn poll_in_flight(&mut self) -> bool {
        let Some(job) = self.in_flight.take() else {
            return false;
        };
        let Some(outcome) = job.shared.lock().unwrap().take() else {
            self.in_flight = Some(job);
            return false;
        };
        if job.started.elapsed() < MIN_LOADING_TIME {
            job.shared.lock().unwrap().replace(outcome);
            self.in_flight = Some(job);
            return false;
        }
        match outcome {
            JobOutcome::Success(action) => {
                self.log(ActivityKind::Action, action);
                self.clear_error_message();
            }
            JobOutcome::Error(message) => {
                self.log(ActivityKind::Error, message.clone());
                self.set_error_message(message);
            }
        }
        self.rebuild_rows();
        true
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
                    self.log(ActivityKind::Action, format!("{name} finished ok"));
                }
                (Some(outcome), true) => {
                    let detail =
                        format!("{name} FAILED (exit {:?})", outcome.exit_code.unwrap_or(1));
                    self.set_error_message(detail.clone());
                    self.log(ActivityKind::Error, detail);
                }
                _ => self.log(ActivityKind::Action, format!("{name} stopped")),
            }
        }
        if let View::Logs(state) = &mut self.view
            && state.follow
        {
            let profile = state.profile.clone();
            let row = self.rows.iter().find(|row| row.name == profile);
            let running = row.is_some_and(|row| row.running);
            let since = row.and_then(|row| row.running_since);
            let status = match row.as_ref().map(|row| row.status.clone()) {
                Some(status) => status,
                None => read_status(&self.paths, &profile).ok().flatten(),
            };
            let stale = since.is_some_and(|start| {
                status
                    .as_ref()
                    .is_some_and(|outcome| outcome.finished_at <= start)
            });
            let elapsed = since
                .map(|since| dashboard::format_elapsed(chrono::Utc::now() - since))
                .unwrap_or_default();
            state.follow_header = if running {
                format!("running {profile} · {elapsed}")
            } else if stale {
                format!("{profile} not running")
            } else {
                match status.as_ref() {
                    Some(outcome) if outcome.skipped => {
                        format!("{profile} skipped — a run was already active")
                    }
                    Some(outcome) if outcome.success => format!("{profile} finished ok"),
                    Some(outcome) => format!(
                        "{profile} FAILED (exit {:?})",
                        outcome.exit_code.unwrap_or(1)
                    ),
                    None => format!("{profile} not running"),
                }
            };
            state.set_content(logs::tail(&self.paths, &profile, 200));
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
                        if index < presets::PRESETS.len() {
                            self.activate_preset(index);
                        } else {
                            let jitter = self.config.defaults.resolved().random_delay.as_secs();
                            self.view = View::Editor(Box::new(editor::EditorState::new(
                                None,
                                self.catalog.clone(),
                                jitter,
                            )));
                        }
                    }
                    _ => {}
                }
            }
            View::Editor(mut state) => match state.handle_key(key) {
                editor::EditorEvent::Cancel => {
                    let discarded = cancel_message(state.is_dirty());
                    if !discarded.is_empty() {
                        self.log(ActivityKind::Action, discarded);
                    }
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

    fn activate_preset(&mut self, index: usize) {
        let preset = &presets::PRESETS[index];
        let base = crate::domain::presets::PRESET_IDS[index];
        let name = preset.suggested_name.to_string();
        if self.config.profiles.iter().any(|p| p.name == name) {
            self.set_error_message(format!("{name} is already active"));
            return;
        }
        let profile = Profile {
            name,
            steps: Vec::new(),
            base: Some(base.to_string()),
            extra_steps: Vec::new(),
            excluded_steps: Vec::new(),
            schedule: crate::domain::schedule::daily_choice(
                self.config.defaults.resolved().random_delay.as_secs(),
            ),
            notify: NotifyPolicy::default(),
        };
        self.log(ActivityKind::Action, format!("activated preset {base}"));
        self.config.upsert(profile);
        if let Err(error) = config::save(&self.paths, &self.config) {
            self.set_error_message(format!("save failed: {error}"));
            return;
        }
        self.spawn_sync_job(format!("activating {base}"), move |report| {
            if report.errors.is_empty() {
                JobOutcome::Success(format!("activated {base}"))
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

    fn run_now(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let name = row.name.clone();
        if row.running {
            self.log(ActivityKind::Action, format!("{name} already running"));
            self.view = View::Logs(logs::LogsState::follow(&name));
            return;
        }
        self.log(ActivityKind::Action, format!("starting {name}"));
        let ctl = (self.controller_factory)();
        let name_for_job = name.clone();
        self.spawn_background(format!("starting {name}"), move || {
            match ctl.start_service(&name_for_job) {
                Ok(()) => JobOutcome::Success(format!("started {name_for_job}")),
                Err(error) => JobOutcome::Error(format!("start failed: {error}")),
            }
        });
        self.view = View::Logs(logs::LogsState::follow(&name));
    }

    fn stop_run(&mut self, profile: &str) {
        self.log(ActivityKind::Action, format!("stopping {profile}"));
        let ctl = (self.controller_factory)();
        let name = profile.to_string();
        self.spawn_background(format!("stopping {name}"), move || {
            match ctl.stop_service(&name) {
                Ok(()) => JobOutcome::Success(format!("stopped {name}")),
                Err(error) => JobOutcome::Error(format!("stop failed: {error}")),
            }
        });
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
