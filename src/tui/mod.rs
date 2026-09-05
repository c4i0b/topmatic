mod dashboard;
mod editor;
mod input;
mod logs;

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::config::{self, AppConfig};
use crate::domain::profile::NotifyPolicy;
use crate::domain::steps::{FALLBACK_STEPS, StepEntry, curated_category};
use crate::paths::Paths;
use crate::runner::{self, RunOutcome, notify::NullNotify, read_status};
use crate::systemd::{RealSystemdCtl, SystemdCtl, sync as systemd_sync};

use input::LineEdit;

pub enum View {
    Dashboard,
    Editor(Box<editor::EditorState>),
    Logs(logs::LogsState),
    Help,
    Confirm { profile: String },
}

pub struct App {
    pub config: AppConfig,
    pub paths: Paths,
    pub ctl: RealSystemdCtl,
    pub topmatic_bin: PathBuf,
    pub topgrade_bin: Option<PathBuf>,
    pub catalog: Vec<StepEntry>,
    pub view: View,
    pub selected: usize,
    pub rows: Vec<dashboard::ProfileRow>,
    pub filter: LineEdit,
    pub filter_editing: bool,
    pub list_area: Cell<Rect>,
    pub message: String,
    pub run_rx: Option<mpsc::Receiver<Result<RunOutcome, String>>>,
    pub should_quit: bool,
}

pub fn run() -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    let result = app_loop(&mut terminal);
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn app_loop(terminal: &mut ratatui::DefaultTerminal) -> anyhow::Result<()> {
    let mut app = App::boot()?;
    loop {
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key);
                    }
                }
                Event::Mouse(mouse) => app.handle_mouse(mouse),
                _ => {}
            }
        }
        app.poll_background_run();
        if app.should_quit {
            break;
        }
    }
    Ok(())
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
        let topgrade_bin = runner::resolve::find_in_path("topgrade", &path_env);

        let mut app = Self {
            config,
            paths,
            ctl,
            topmatic_bin,
            topgrade_bin,
            catalog: Vec::new(),
            view: View::Dashboard,
            selected: 0,
            rows: Vec::new(),
            filter: LineEdit::new(String::new()),
            filter_editing: false,
            list_area: Cell::new(Rect::default()),
            message: String::new(),
            run_rx: None,
            should_quit: false,
        };
        app.catalog = app.load_catalog();
        let report = systemd_sync::sync(&app.config, &app.topmatic_bin, &app.ctl);
        app.message = if report.errors.is_empty() {
            format!(
                "synced{}{}{}",
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

    fn load_catalog(&self) -> Vec<StepEntry> {
        if let Some(bin) = &self.topgrade_bin
            && let Ok(output) = std::process::Command::new(bin).arg("--help").output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            return crate::domain::steps::catalog(&text);
        }
        FALLBACK_STEPS
            .iter()
            .map(|id| StepEntry {
                id: id.to_string(),
                category: curated_category(id),
            })
            .collect()
    }

    pub fn rebuild_rows(&mut self) {
        self.rows = self
            .config
            .profiles
            .iter()
            .map(|profile| dashboard::ProfileRow {
                name: profile.name.clone(),
                enabled: profile.enabled,
                schedule: profile.schedule.summary(),
                next_run: if profile.enabled {
                    self.ctl.next_run(&profile.name)
                } else {
                    None
                },
                status: read_status(&self.paths, &profile.name).ok().flatten(),
                timer_active: profile.enabled && self.ctl.timer_active(&profile.name),
            })
            .collect();
        self.clamp_selection();
    }

    pub fn visible_rows(&self) -> Vec<&dashboard::ProfileRow> {
        let needle = self.filter.value.to_lowercase();
        self.rows
            .iter()
            .filter(|row| needle.is_empty() || row.name.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn filtering(&self) -> bool {
        !self.filter.value.is_empty() || self.filter_editing
    }

    pub fn filter_text(&self) -> &str {
        &self.filter.value
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
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let quits = matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'));
        if quits {
            match &self.view {
                View::Dashboard | View::Help | View::Confirm { .. } => {
                    self.should_quit = true;
                    return;
                }
                View::Logs(_) => {
                    self.should_quit = true;
                    return;
                }
                View::Editor(_) => {}
            }
        }
        match std::mem::replace(&mut self.view, View::Dashboard) {
            View::Dashboard => self.handle_dashboard_key(key),
            View::Editor(mut state) => match state.handle_key(key) {
                editor::EditorEvent::Cancel => {
                    self.message.clear();
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
        if self.filter_editing {
            self.handle_filter_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('j' | 'J') | KeyCode::Down => {
                if self.selected + 1 < self.visible_rows().len() {
                    self.selected += 1;
                }
            }
            KeyCode::Char('k' | 'K') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Enter => self.open_editor_for_selected(),
            KeyCode::Esc => {
                if self.filtering() {
                    self.filter = LineEdit::new(String::new());
                    self.selected = 0;
                }
            }
            KeyCode::Char(c) => match c.to_ascii_lowercase() {
                '/' => self.filter_editing = true,
                '?' => self.view = View::Help,
                'n' => {
                    self.view = View::Editor(Box::new(editor::EditorState::new(
                        None,
                        self.catalog.clone(),
                    )));
                }
                'e' => self.open_editor_for_selected(),
                ' ' => self.toggle_enabled(),
                'd' => {
                    if let Some(name) = self.selected_row().map(|row| row.name.clone()) {
                        self.view = View::Confirm { profile: name };
                    }
                }
                'r' => self.run_now(),
                't' => self.dry_run_selected(),
                'l' => self.open_logs(),
                'g' => self.enable_linger(),
                's' => self.resync(),
                _ => {}
            },
            _ => {}
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.filter_editing = false,
            KeyCode::Esc => {
                self.filter_editing = false;
                self.filter = LineEdit::new(String::new());
                self.selected = 0;
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                if self.selected + 1 < self.visible_rows().len() {
                    self.selected += 1;
                }
            }
            _ => {
                self.filter.handle_key(key);
                self.clamp_selection();
            }
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        let list = self.list_area.get();
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.selected = self.selected.saturating_sub(1);
            }
            MouseEventKind::ScrollDown => {
                if self.selected + 1 < self.visible_rows().len() {
                    self.selected += 1;
                }
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
        let profile = match state.to_profile() {
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
        if state.creating && self.config.profile(&profile.name).is_some() {
            self.message = format!("profile {} already exists", profile.name);
            self.view = View::Editor(Box::new(state));
            return;
        }
        let mut profile = profile;
        if !profile.repos.is_empty()
            && let Some(topgrade) = self.topgrade_bin.clone()
            && let Err(error) = runner::verify(&profile, &topgrade, &self.paths, &NullNotify)
        {
            profile.enabled = false;
            self.message = format!("saved paused — {error}");
            self.config.upsert(profile);
            let _ = config::save(&self.paths, &self.config);
            let _ = systemd_sync::sync(&self.config, &self.topmatic_bin, &self.ctl);
            self.rebuild_rows();
            return;
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

    fn toggle_enabled(&mut self) {
        let Some(name) = self.selected_row().map(|row| row.name.clone()) else {
            return;
        };
        let Some(profile) = self.config.profile_mut(&name) else {
            return;
        };
        profile.enabled = !profile.enabled;
        let enabled = profile.enabled;
        if let Err(error) = config::save(&self.paths, &self.config) {
            self.message = format!("save failed: {error}");
            return;
        }
        let _ = systemd_sync::sync(&self.config, &self.topmatic_bin, &self.ctl);
        self.message = format!("{name} {}", if enabled { "resumed" } else { "paused" });
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

    fn dry_run_selected(&mut self) {
        let profile = self
            .selected_row()
            .and_then(|row| self.config.profile(&row.name).cloned());
        let Some(profile) = profile else {
            return;
        };
        let Some(topgrade) = self.topgrade_bin.clone() else {
            self.message = "topgrade not found in PATH".to_string();
            return;
        };
        let paths = self.paths.clone();
        let profile_name = profile.name.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = runner::run(&profile, &topgrade, &paths, &NullNotify, true)
                .map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
        self.run_rx = Some(rx);
        self.message = format!("dry-run of {profile_name} started…");
    }

    fn poll_background_run(&mut self) {
        let Some(rx) = &self.run_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(outcome)) => {
                self.message = if outcome.success {
                    format!("dry-run of {} finished ok", outcome.profile)
                } else {
                    format!(
                        "dry-run of {} failed (exit {:?}), see logs",
                        outcome.profile, outcome.exit_code
                    )
                };
                self.run_rx = None;
                self.rebuild_rows();
            }
            Ok(Err(error)) => {
                self.message = format!("dry-run error: {error}");
                self.run_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.run_rx = None,
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

    fn draw(&self, frame: &mut Frame) {
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .areas(frame.area());

        self.draw_header(frame, header);
        match &self.view {
            View::Dashboard => {
                let areas = dashboard::render(self, frame, body);
                self.list_area.set(areas.list);
            }
            View::Editor(state) => self.draw_editor(state, frame, body),
            View::Logs(state) => logs::render(state, frame, body),
            View::Help => self.draw_help(frame, body),
            View::Confirm { profile } => self.draw_confirm(profile, frame, body),
        }
        self.draw_footer(frame, footer);
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let linger = match self.ctl.linger_enabled() {
            Some(true) => Span::styled("linger: on", Style::new().fg(Color::Green)),
            Some(false) => {
                Span::styled("linger: off (L to enable)", Style::new().fg(Color::Yellow))
            }
            None => Span::raw(""),
        };
        let line = Line::from(vec![
            Span::styled(" topmatic ", Style::new().add_modifier(Modifier::BOLD)),
            linger,
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let hints = if self.filter_editing {
            "filter: type…  Enter accept  Esc clear  ↑↓ move"
        } else {
            match &self.view {
                View::Dashboard => {
                    "/ filter  n new  e edit  space pause  d delete  r run  t test  l logs  g linger  s resync  ? help  q quit"
                }
                View::Editor(_) => {
                    "tab section  space toggle  ←→ adjust  type numbers  esc cancel  q quit"
                }
                View::Logs(_) => "enter open  h back  r refresh  esc back  q quit",
                View::Help => "any key closes  q quit",
                View::Confirm { .. } => "y confirm delete  esc cancels  q quit",
            }
        };
        let prefix = if self.filter_editing {
            format!(" /{}", self.filter.value)
        } else if self.message.is_empty() {
            String::new()
        } else {
            format!(" {} ", self.message)
        };
        let footer = Line::from(vec![
            Span::styled(prefix, Style::new().fg(Color::Yellow)),
            Span::styled(format!(" {hints}"), Style::new().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(footer), area);
    }

    fn draw_editor(&self, state: &editor::EditorState, frame: &mut Frame, area: Rect) {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(area);

        let name_line = Line::from(vec![
            focus_marker(state.section == editor::Section::Name),
            Span::raw(" name: "),
            Span::styled(
                if state.creating {
                    state.name.value.clone()
                } else {
                    state.name.value.clone() + " (fixed)"
                },
                Style::new().fg(Color::Cyan),
            ),
        ]);

        let mut steps_lines = vec![Line::from(vec![
            focus_marker(state.section == editor::Section::Steps),
            Span::raw(format!(
                " steps ({}/{} shown, selected: {}): filter: ",
                state.filtered_steps().len(),
                state.catalog.len(),
                state.selected_steps.len()
            )),
            Span::styled(state.filter.value.clone(), Style::new().fg(Color::Cyan)),
        ])];
        for (index, entry) in state.filtered_steps().iter().take(10).enumerate() {
            let marker = if state.selected_steps.contains(&entry.id) {
                "[x]"
            } else {
                "[ ]"
            };
            let line = Line::from(vec![
                Span::raw(if index == state.list_index {
                    "▶ "
                } else {
                    "  "
                }),
                Span::raw(format!("{marker} ")),
                Span::styled(
                    entry.id.clone(),
                    if entry.category.is_some() {
                        Style::new().fg(Color::Green)
                    } else {
                        Style::new()
                    },
                ),
                Span::styled(
                    entry
                        .category
                        .map(|c| format!("  ({c})"))
                        .unwrap_or_default(),
                    Style::new().fg(Color::DarkGray),
                ),
            ]);
            steps_lines.push(line);
        }

        let focus_repos = state.section == editor::Section::Repos;
        steps_lines.push(Line::from(vec![
            focus_marker(focus_repos),
            Span::raw(format!(" repos ({}): path: ", state.repos.len())),
            Span::styled(state.repo_path.value.clone(), Style::new().fg(Color::Cyan)),
        ]));
        steps_lines.push(Line::from(vec![
            Span::raw("  apply (optional): "),
            Span::styled(state.repo_apply.value.clone(), Style::new().fg(Color::Cyan)),
        ]));
        steps_lines.push(Line::from(vec![
            Span::raw("  scan dir + Enter: "),
            Span::styled(state.repo_scan.value.clone(), Style::new().fg(Color::Cyan)),
        ]));
        for (index, repo) in state.repos.iter().take(6).enumerate() {
            let row = 3 + index;
            steps_lines.push(Line::from(vec![
                Span::raw(if focus_repos && state.repo_index == row {
                    "▶ "
                } else {
                    "  "
                }),
                Span::raw(repo.path.clone()),
                Span::styled(
                    repo.apply
                        .as_ref()
                        .map(|apply| format!("  -> {apply}"))
                        .unwrap_or_default(),
                    Style::new().fg(Color::Yellow),
                ),
            ]));
        }

        let left_block = Paragraph::new(
            std::iter::once(name_line)
                .chain(steps_lines)
                .collect::<Vec<_>>(),
        )
        .block(Block::bordered().title("profile"));
        frame.render_widget(left_block, left);

        let mut right_lines: Vec<Line> = Vec::new();
        let focus = state.section == editor::Section::Schedule;
        right_lines.push(Line::from(vec![
            focus_marker(focus),
            Span::raw(" schedule"),
        ]));
        for (index, text) in self.schedule_lines(state).iter().enumerate() {
            right_lines.push(Line::from(vec![
                Span::raw(if focus && index == state.schedule_index {
                    "▶ "
                } else {
                    "  "
                }),
                Span::raw(text.clone()),
            ]));
        }
        right_lines.push(Line::from(""));
        let focus = state.section == editor::Section::Options;
        right_lines.push(Line::from(vec![focus_marker(focus), Span::raw(" options")]));
        let options = [
            format!("  cleanup (auto-clean): {}", on_off(state.cleanup)),
            format!("  notify: {}", notify_label(state.notify)),
            format!("  enabled: {}", on_off(state.enabled)),
        ];
        for (index, text) in options.iter().enumerate() {
            right_lines.push(Line::from(vec![
                Span::raw(if focus && index == state.option_index {
                    "▶ "
                } else {
                    "  "
                }),
                Span::raw(text.clone()),
            ]));
        }
        right_lines.push(Line::from(""));
        let focus = state.section == editor::Section::Actions;
        right_lines.push(Line::from(vec![
            focus_marker(focus),
            Span::raw(if state.action_index == 0 {
                "  <Save>   Cancel"
            } else {
                "   Save  <Cancel>"
            }),
        ]));
        let right_block = Paragraph::new(right_lines).block(Block::bordered().title("settings"));
        frame.render_widget(right_block, right);
    }

    fn schedule_lines(&self, state: &editor::EditorState) -> Vec<String> {
        use crate::domain::schedule::SchedulePreset;
        let mut lines = vec![match &state.schedule.preset {
            SchedulePreset::Hourly => "preset: hourly".to_string(),
            SchedulePreset::EveryNHours { hours } => format!("preset: every N hours ({hours}h)"),
            SchedulePreset::Daily { .. } => "preset: daily at fixed time".to_string(),
            SchedulePreset::Weekly { .. } => "preset: weekly at fixed time".to_string(),
            SchedulePreset::Spread { .. } => "preset: spread over the period".to_string(),
            SchedulePreset::Custom { .. } => "preset: custom OnCalendar".to_string(),
        }];
        match &state.schedule.preset {
            SchedulePreset::Hourly => {}
            SchedulePreset::EveryNHours { hours } => lines.push(format!("every: {hours} hours")),
            SchedulePreset::Daily { hour, minute } => {
                lines.push(format!("at: {hour:02}:{minute:02}{}", state.typed_hint()));
            }
            SchedulePreset::Weekly {
                weekday,
                hour,
                minute,
            } => {
                lines.push(format!("on: {}", weekday.as_systemd()));
                lines.push(format!("at: {hour:02}:{minute:02}{}", state.typed_hint()));
            }
            SchedulePreset::Spread { period } => lines.push(format!(
                "window: {}",
                match period {
                    crate::domain::schedule::SpreadPeriod::Daily => "daily",
                    crate::domain::schedule::SpreadPeriod::Weekly => "weekly",
                }
            )),
            SchedulePreset::Custom { .. } => {
                lines.push(format!("expr: {}", state.custom.value));
            }
        }
        lines.push(format!(
            "max random delay: {}",
            crate::domain::schedule::format_delay(state.schedule.randomized_delay_sec)
        ));
        lines
    }

    fn draw_help(&self, frame: &mut Frame, area: Rect) {
        let text = vec![
            Line::from("topmatic help"),
            Line::from(""),
            Line::from("n  new profile      e  edit profile      space  pause/resume"),
            Line::from("d  delete profile (also purges run history)"),
            Line::from("r  run now via systemd      t  dry-run test in foreground thread"),
            Line::from("l  browse run logs      L  enable lingering      R  resync units"),
            Line::from(""),
            Line::from(
                "Config: ~/.config/topmatic/config.toml (source of truth, editable by hand)",
            ),
            Line::from("topgrade runs fully isolated with its own config; no sudo anywhere."),
        ];
        frame.render_widget(
            Paragraph::new(text).block(Block::bordered().title("help (? or any key closes)")),
            area,
        );
    }

    fn draw_confirm(&self, profile: &str, frame: &mut Frame, area: Rect) {
        let text = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("Delete profile "),
                Span::styled(profile.to_string(), Style::new().fg(Color::Red)),
                Span::raw("?"),
            ]),
            Line::from("Run history (logs, status) will also be deleted."),
            Line::from(""),
            Line::from("y to confirm, anything else cancels"),
        ])
        .block(Block::bordered().title("confirm deletion"));
        let popup = centered_rect(area, 50, 9);
        frame.render_widget(Clear, popup);
        frame.render_widget(text, popup);
    }
}

fn focus_marker(focused: bool) -> Span<'static> {
    if focused {
        Span::styled(
            "▸",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw(" ")
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn notify_label(policy: NotifyPolicy) -> &'static str {
    match policy {
        NotifyPolicy::Always => "always",
        NotifyPolicy::OnFailure => "on failure",
        NotifyPolicy::Never => "never",
    }
}

fn centered_rect(area: Rect, percent_x: u16, height: u16) -> Rect {
    let width = area.width.saturating_sub(2);
    let popup_width = (width * percent_x / 100).max(30);
    let popup_height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    }
}
