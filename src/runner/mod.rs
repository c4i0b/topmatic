use std::fs::{self, OpenOptions};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::argv::topgrade_argv;
use crate::domain::profile::Profile;
use crate::paths::Paths;

pub mod notify;
pub mod resolve;
pub mod topgrade_config;

use notify::NotifyBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutcome {
    pub profile: String,
    pub dry_run: bool,
    pub skipped: bool,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_secs: f64,
    pub log_path: PathBuf,
}

pub fn run(
    profile: &Profile,
    topgrade_bin: &Path,
    paths: &Paths,
    notify: &dyn NotifyBackend,
    dry_run: bool,
) -> anyhow::Result<RunOutcome> {
    fs::create_dir_all(&paths.config_dir)?;
    fs::create_dir_all(paths.state_dir.join("status"))?;
    fs::create_dir_all(paths.logs_dir(&profile.name))?;
    let topgrade_config_path = paths.topgrade_config_file_for(profile);
    topgrade_config::write_for(profile, &topgrade_config_path)?;

    let started = Utc::now();
    let log_path = paths
        .logs_dir(&profile.name)
        .join(format!("{}.log", started.format("%Y%m%d-%H%M%S")));

    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.lock_file(&profile.name))?;
    let mut lock = fd_lock::RwLock::new(lock_file);

    let outcome = match lock.try_write() {
        Ok(_guard) => execute(
            profile,
            topgrade_bin,
            &topgrade_config_path,
            dry_run,
            started,
            &log_path,
        )?,
        Err(_) => RunOutcome {
            profile: profile.name.clone(),
            dry_run,
            skipped: true,
            success: false,
            exit_code: None,
            started_at: started,
            finished_at: Utc::now(),
            duration_secs: 0.0,
            log_path: log_path.clone(),
        },
    };

    write_status(paths, &profile.name, &outcome)?;

    if !outcome.skipped && notify::should_notify(profile.notify, outcome.success) {
        let summary = if outcome.success {
            format!("topmatic: {} updated", profile.name)
        } else {
            format!("topmatic: {} FAILED", profile.name)
        };
        let body = format!(
            "exit code: {}\nlog: {}",
            outcome
                .exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            outcome.log_path.display()
        );
        let _ = notify.send(&summary, &body);
    }

    Ok(outcome)
}

fn execute(
    profile: &Profile,
    topgrade_bin: &Path,
    topgrade_config_path: &Path,
    dry_run: bool,
    started: DateTime<Utc>,
    log_path: &Path,
) -> anyhow::Result<RunOutcome> {
    let argv = topgrade_argv(profile, topgrade_config_path, dry_run);
    let mut command = Command::new(topgrade_bin);
    command.args(&argv[1..]);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;

    let log_out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let log_err = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let out_handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut writer = Tee(BufWriter::new(log_out), io::stdout().lock());
        let _ = io::copy(&mut reader, &mut writer);
        let _ = writer.flush();
    });
    let err_handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut writer = Tee(BufWriter::new(log_err), io::stderr().lock());
        let _ = io::copy(&mut reader, &mut writer);
        let _ = writer.flush();
    });

    let status = child.wait()?;
    let _ = out_handle.join();
    let _ = err_handle.join();

    let finished = Utc::now();
    Ok(RunOutcome {
        profile: profile.name.clone(),
        dry_run,
        skipped: false,
        success: status.success(),
        exit_code: status.code(),
        started_at: started,
        finished_at: finished,
        duration_secs: (finished - started).num_milliseconds() as f64 / 1000.0,
        log_path: log_path.to_path_buf(),
    })
}

fn write_status(paths: &Paths, profile: &str, outcome: &RunOutcome) -> anyhow::Result<()> {
    let path = paths.status_file(profile);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(outcome)?)?;
    Ok(())
}

pub fn read_status(paths: &Paths, profile: &str) -> anyhow::Result<Option<RunOutcome>> {
    let path = paths.status_file(profile);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

pub fn verify(
    profile: &Profile,
    topgrade_bin: &Path,
    paths: &Paths,
    notify: &dyn NotifyBackend,
) -> Result<(), String> {
    let outcome =
        run(profile, topgrade_bin, paths, notify, true).map_err(|error| error.to_string())?;
    if outcome.skipped {
        return Err("another run is already in progress".to_string());
    }
    if outcome.success {
        Ok(())
    } else {
        Err(format!(
            "dry-run failed (exit {:?}), log: {}",
            outcome.exit_code,
            outcome.log_path.display()
        ))
    }
}

struct Tee<A: Write, B: Write>(A, B);

impl<A: Write, B: Write> Write for Tee<A, B> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write_all(buf)?;
        self.1.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()?;
        self.1.flush()
    }
}
