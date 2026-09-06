use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use super::SystemdCtl;

#[cfg(test)]
pub struct FakeCtl {
    dir: PathBuf,
    calls: Arc<Mutex<Vec<String>>>,
    pub existing_instances: Vec<String>,
    pub linger: Option<bool>,
    pub fail_enable_for: Option<String>,
}

#[cfg(test)]
impl FakeCtl {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            calls: Arc::new(Mutex::new(Vec::new())),
            existing_instances: Vec::new(),
            linger: Some(false),
            fail_enable_for: None,
        }
    }

    pub fn shared_calls(&self) -> Arc<Mutex<Vec<String>>> {
        Arc::clone(&self.calls)
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}

#[cfg(test)]
impl SystemdCtl for FakeCtl {
    fn unit_dir(&self) -> PathBuf {
        self.dir.clone()
    }

    fn daemon_reload(&self) -> io::Result<()> {
        self.record("reload".to_string());
        Ok(())
    }

    fn enable_timer(&self, profile: &str) -> io::Result<()> {
        self.record(format!("enable:{profile}"));
        if self.fail_enable_for.as_deref() == Some(profile) {
            return Err(io::Error::other("boom"));
        }
        Ok(())
    }

    fn disable_timer(&self, profile: &str) -> io::Result<()> {
        self.record(format!("disable:{profile}"));
        Ok(())
    }

    fn start_service(&self, profile: &str) -> io::Result<()> {
        self.record(format!("start:{profile}"));
        Ok(())
    }

    fn stop_all(&self) -> io::Result<()> {
        self.record("stop_all".to_string());
        Ok(())
    }

    fn instances(&self) -> Vec<String> {
        self.existing_instances.clone()
    }

    fn next_run(&self, _profile: &str) -> Option<DateTime<Utc>> {
        None
    }

    fn timer_active(&self, profile: &str) -> bool {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call == &format!("enable:{profile}"))
    }

    fn linger_enabled(&self) -> Option<bool> {
        self.linger
    }
}
