use super::super::View;
use super::super::dashboard;
use super::super::logs;
use super::super::presets;
use super::App;
use super::jobs::{FOLLOW_LINGER_TICKS, JobOutcome};
use crate::activity::ActivityKind;
use crate::config;
use crate::domain::profile::{NotifyPolicy, Profile};
use crate::runner::read_status;

impl App {
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

    pub(super) fn refresh_live_state(&mut self) {
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
            let finished_fresh = !running && !stale && status.is_some();
            if running {
                state.auto_close = None;
            } else if finished_fresh && state.auto_close.is_none() && !state.linger_canceled {
                state.auto_close = Some(FOLLOW_LINGER_TICKS);
            }
            if let Some(remaining) = state.auto_close
                && let Some(left) = remaining.checked_sub(1)
            {
                state.auto_close = Some(left);
            } else if state.auto_close.is_some() {
                state.auto_close = None;
                self.view = View::Dashboard;
            }
        }
    }

    pub(super) fn activate_preset(&mut self, index: usize) {
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

    pub(super) fn run_now(&mut self) {
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
}
