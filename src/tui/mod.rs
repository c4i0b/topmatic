mod dashboard;
mod editor;
mod input;
mod logs;
mod presets;

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

const LIST_VISIBLE: usize = 14;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, ListItem, Paragraph};

use crate::config::{self, AppConfig};
use crate::paths::Paths;
use crate::runner::{self, RunOutcome, notify::NullNotify, read_status};
use crate::systemd::{RealSystemdCtl, SystemdCtl, sync as systemd_sync};

use input::{FilterState, LineEdit};

pub enum View {
    Dashboard,
    PresetPicker { index: usize },
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
    pub catalog: Vec<String>,
    pub view: View,
    pub selected: usize,
    pub list_scroll: u16,
    pub rows: Vec<dashboard::ProfileRow>,
    pub filter: FilterState,
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
            list_scroll: 0,
            rows: Vec::new(),
            filter: FilterState::new(),
            list_area: Cell::new(Rect::default()),
            message: String::new(),
            run_rx: None,
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
        } else if start.saturating_add(LIST_VISIBLE) <= self.selected {
            self.list_scroll = (self.selected + 1 - LIST_VISIBLE) as u16;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let quits = matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'));
        let editing_dashboard_filter = matches!(self.view, View::Dashboard) && self.filter.active;
        if quits && !editing_dashboard_filter {
            match &self.view {
                View::Dashboard | View::Help | View::Confirm { .. } => {
                    self.should_quit = true;
                    return;
                }
                View::Logs(_) | View::PresetPicker { .. } => {
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
                't' => self.dry_run_selected(),
                'l' => self.open_logs(),
                'g' => self.enable_linger(),
                's' => self.resync(),
                _ => {}
            },
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
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
            View::PresetPicker { index } => {
                let mut items: Vec<ListItem> = presets::PRESETS
                    .iter()
                    .enumerate()
                    .map(|(i, preset)| {
                        if i == *index {
                            ListItem::new(Line::styled(
                                format!("▶ {} — {}", preset.label, preset.description),
                                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                            ))
                        } else {
                            ListItem::new(Line::from(format!(
                                "  {} — {}",
                                preset.label, preset.description
                            )))
                        }
                    })
                    .collect();
                let scratch_index = presets::PRESETS.len();
                items.push(ListItem::new(if *index == scratch_index {
                    Line::styled(
                        "▶ Start from scratch",
                        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    )
                } else {
                    Line::from("  Start from scratch")
                }));
                let list = ratatui::widgets::List::new(items)
                    .block(Block::bordered().title("new profile — pick a preset"));
                frame.render_widget(list, body);
            }
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
                Span::styled("linger: off (g to enable)", Style::new().fg(Color::Yellow))
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
        let hints = if self.filter.active {
            "filter: type…  Enter accept  Esc clear  ↑↓ move"
        } else {
            match &self.view {
                View::Dashboard => {
                    "/ filter  n new  e edit  d delete  r run  t test  l logs  g linger  s resync  ? help  q quit"
                }
                View::Editor(_) => "tab section  enter save  / filter steps  esc cancel  q quit",
                View::PresetPicker { .. } => "enter choose  esc back  q quit",
                View::Logs(_) => "enter open  h back  r refresh  esc back  q quit",
                View::Help => "any key closes  q quit",
                View::Confirm { .. } => "y confirm delete  esc cancels  q quit",
            }
        };
        let prefix = if self.filter.active {
            format!(" /{}", self.filter.text())
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
        let title = match &state.original_name {
            Some(name) => format!("edit profile — {name}"),
            None => "new profile".to_string(),
        };
        let inner_height = area.height.saturating_sub(2) as usize;

        let mut lines: Vec<Line> = Vec::new();
        let focus_steps = state.section == editor::Section::Steps;
        let steps_total = state.filtered_steps().len();
        let filter_label = if state.steps_filter.active {
            format!("filter: {}▏", state.steps_filter.text())
        } else {
            "/ filter".to_string()
        };
        lines.push(Line::from(vec![
            focus_marker(focus_steps),
            Span::raw(format!(
                " steps (selected: {}) [{}-{}/{}] {}",
                state.selected_steps.len(),
                if steps_total == 0 {
                    0
                } else {
                    state.steps_window().0 + 1
                },
                state.steps_window().1,
                steps_total,
                filter_label
            )),
        ]));
        let (steps_start, steps_end) = state.steps_window();
        for (offset, id) in state.filtered_steps()[steps_start..steps_end]
            .iter()
            .enumerate()
        {
            let index = steps_start + offset;
            let marker = if state.selected_steps.contains(*id) {
                "[x]"
            } else {
                "[ ]"
            };
            lines.push(Line::from(format!(
                "{}{marker} {}",
                if focus_steps && index == state.list_index {
                    "▶ "
                } else {
                    "  "
                },
                id
            )));
        }

        lines.push(Line::from(""));
        let focus_schedule = state.section == editor::Section::Schedule;
        lines.push(Line::from(vec![
            focus_marker(focus_schedule),
            Span::styled(
                format!("schedule: {}", state.schedule.summary()),
                Style::new().fg(Color::Cyan),
            ),
        ]));
        let choices = crate::domain::schedule::quick_choices();
        for (index, (label, _)) in choices.iter().enumerate() {
            let selected =
                crate::domain::schedule::matches_quick_choice(&state.schedule) == Some(index);
            lines.push(Line::from(format!(
                "{}{} {}{}",
                if focus_schedule && state.schedule_index == index {
                    "▶ "
                } else {
                    "  "
                },
                if selected { "[" } else { " " },
                label,
                if selected { "]" } else { " " },
            )));
        }
        let custom_row = choices.len();
        lines.push(Line::from(format!(
            "{}custom OnCalendar: {}",
            if focus_schedule && state.schedule_index == custom_row {
                "▶ "
            } else {
                "  "
            },
            state.custom.value
        )));
        for (row, text) in schedule_context_lines(state).iter().enumerate() {
            let index = choices.len() + 1 + row;
            lines.push(Line::from(format!(
                "{}{}",
                if focus_schedule && state.schedule_index == index {
                    "▶ "
                } else {
                    "  "
                },
                text
            )));
        }
        let jitter_row = state_rows_count(state) - 1;
        lines.push(Line::from(format!(
            "{}jitter: {}",
            if focus_schedule && state.schedule_index == jitter_row {
                "▶ "
            } else {
                "  "
            },
            crate::domain::schedule::format_delay(state.schedule.randomized_delay_sec)
        )));

        lines.push(Line::from(""));
        let focus_options = state.section == editor::Section::Options;
        lines.push(Line::from(vec![
            focus_marker(focus_options),
            Span::styled("options", Style::new().fg(Color::Cyan)),
        ]));
        lines.push(Line::from(format!(
            "{}cleanup:  {}yes{}  {}no{}",
            if focus_options && state.option_index == 0 {
                "▶ "
            } else {
                "  "
            },
            if state.cleanup { "[" } else { " " },
            if state.cleanup { "]" } else { " " },
            if !state.cleanup { "[" } else { " " },
            if !state.cleanup { "]" } else { " " },
        )));
        let notify = state.notify;
        lines.push(Line::from(format!(
            "{}notify:   {}always{}  {}on failure{}  {}never{}",
            if focus_options && state.option_index == 1 {
                "▶ "
            } else {
                "  "
            },
            if notify == crate::domain::profile::NotifyPolicy::Always {
                "["
            } else {
                " "
            },
            if notify == crate::domain::profile::NotifyPolicy::Always {
                "]"
            } else {
                " "
            },
            if notify == crate::domain::profile::NotifyPolicy::OnFailure {
                "["
            } else {
                " "
            },
            if notify == crate::domain::profile::NotifyPolicy::OnFailure {
                "]"
            } else {
                " "
            },
            if notify == crate::domain::profile::NotifyPolicy::Never {
                "["
            } else {
                " "
            },
            if notify == crate::domain::profile::NotifyPolicy::Never {
                "]"
            } else {
                " "
            },
        )));

        let _ = inner_height;
        let paragraph = Paragraph::new(lines).block(Block::bordered().title(title));
        frame.render_widget(paragraph, area);

        if let Some(popup) = &state.name_popup {
            let popup_area = centered_rect(area, 60, 5);
            let text = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw(" name: "),
                    Span::styled(popup.value.clone(), Style::new().fg(Color::Cyan)),
                    Span::raw("▏"),
                ]),
                Line::from(Span::styled(
                    " enter saves · esc back",
                    Style::new().fg(Color::DarkGray),
                )),
            ])
            .block(Block::bordered().title("save profile"));
            frame.render_widget(Clear, popup_area);
            frame.render_widget(text, popup_area);
        }
    }

    fn draw_help(&self, frame: &mut Frame, area: Rect) {
        let text = vec![
            Line::from("topmatic help"),
            Line::from(""),
            Line::from("n  new profile (presets)      e  edit profile      d  delete profile"),
            Line::from("r  run now via systemd        t  dry-run test"),
            Line::from("l  browse run logs            g  enable lingering"),
            Line::from("s  resync units               /  filter profiles"),
            Line::from(""),
            Line::from("editor: tab switches section, enter asks the name and saves,"),
            Line::from("space or arrows pick choices, / filters steps, esc cancels, q quits"),
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

fn schedule_context_lines(state: &editor::EditorState) -> Vec<String> {
    use crate::domain::schedule::SchedulePreset;
    match &state.schedule.preset {
        SchedulePreset::EveryNHours { hours } => vec![format!("every: {hours}h")],
        SchedulePreset::Daily { hour, minute } => {
            vec![
                format!("hour: {hour:02} (←→)"),
                format!("minute: {minute:02}"),
            ]
        }
        SchedulePreset::Weekly {
            weekday,
            hour,
            minute,
        } => vec![
            format!("weekday: {}", weekday.as_systemd()),
            format!("hour: {hour:02}"),
            format!("minute: {minute:02}"),
        ],
        _ => Vec::new(),
    }
}

fn state_rows_count(state: &editor::EditorState) -> usize {
    let choices = crate::domain::schedule::quick_choices().len();
    choices + 1 + schedule_context_lines(state).len() + 1
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
