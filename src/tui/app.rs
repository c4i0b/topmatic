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
    pub ctl: RealSystemdCtl,
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
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
        let ctl = RealSystemdCtl::new(home);
        let topmatic_bin = std::env::current_exe()?;
        let path_env = std::env::var("PATH").unwrap_or_default();
        let topgrade_bin = resolve::find_in_path("topgrade", &path_env);

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
        app.catalog = app.load_catalog();
        let report = systemd_sync::sync(&app.config, &app.topmatic_bin, &app.ctl);
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
        Ok(app)
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
                        let editor = if index < presets::PRESETS.len() {
                            let preset = &presets::PRESETS[index];
                            let steps = presets::steps_for(index, &self.catalog);
                            editor::EditorState::from_preset(
                                self.catalog.clone(),
                                steps,
                                preset.suggested_name,
                            )
                        } else {
                            editor::EditorState::new(None, self.catalog.clone())
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
                'g' => self.enable_linger(),
                's' => self.resync(),
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
            self.view = View::Editor(Box::new(editor::EditorState::new(
                Some(&profile),
                self.catalog.clone(),
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
        let report = systemd_sync::sync(&self.config, &self.topmatic_bin, &self.ctl);
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
        let _ = systemd_sync::sync(&self.config, &self.topmatic_bin, &self.ctl);
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

    fn enable_linger(&mut self) {
        match self.ctl.linger_enabled() {
            Some(true) => self.message = "lingering already enabled".to_string(),
            _ => match self.ctl.enable_linger() {
                Ok(()) => self.message = "lingering enabled".to_string(),
                Err(_) => {
                    self.message =
                        "could not enable lingering (no sudo involved); run: loginctl enable-linger"
                            .to_string()
                }
            },
        }
    }

    fn resync(&mut self) {
        let report = systemd_sync::sync(&self.config, &self.topmatic_bin, &self.ctl);
        self.message = if report.errors.is_empty() && report.is_clean() {
            "already in sync".to_string()
        } else if report.errors.is_empty() {
            "synced".to_string()
        } else {
            format!("sync errors: {}", report.errors.join("; "))
        };
        self.rebuild_rows();
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

    #[test]
    fn cancel_feedback_only_when_something_was_lost() {
        assert_eq!(cancel_message(false), "");
        assert_eq!(cancel_message(true), "editor closed — changes discarded");
    }
}
