use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::App;
use crate::activity::ActivityKind;
use crate::systemd::sync as systemd_sync;

pub(crate) const MESSAGE_TTL_TICKS: u64 = 25;
pub(crate) const FOLLOW_LINGER_TICKS: u32 = 8;
const MIN_LOADING_TIME: Duration = Duration::from_millis(120);

pub struct BackgroundJob {
    pub label: String,
    pub started: Instant,
    pub shared: Arc<Mutex<Option<JobOutcome>>>,
}

pub enum JobOutcome {
    Success(String),
    Error(String),
}

impl App {
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

    pub(super) fn spawn_sync_job(
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

    pub(super) fn set_error_message(&mut self, text: String) {
        self.message = text;
        self.message_is_error = true;
    }

    pub(super) fn clear_error_message(&mut self) {
        if self.message_is_error {
            self.message.clear();
            self.seen_message.clear();
            self.message_is_error = false;
        }
    }
}
