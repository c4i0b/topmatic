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
    quiet: bool,
) -> anyhow::Result<RunOutcome> {
    fs::create_dir_all(&paths.config_dir)?;
    fs::create_dir_all(paths.state_dir.join("status"))?;
    fs::create_dir_all(paths.logs_dir(&profile.name))?;
    topgrade_config::write_if_changed(&paths.topgrade_config_file())?;

    let started = Utc::now();
    let log_path = paths
        .logs_dir(&profile.name)
        .join(format!("{}.log", started.format("%Y%m%d-%H%M%S%.3f")));

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
            paths,
            dry_run,
            started,
            &log_path,
            quiet,
        ),
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

    if !outcome.skipped && !dry_run && notify::should_notify(profile.notify, outcome.success) {
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
    paths: &Paths,
    dry_run: bool,
    started: DateTime<Utc>,
    log_path: &Path,
    quiet: bool,
) -> RunOutcome {
    let argv = topgrade_argv(profile, &paths.topgrade_config_file(), dry_run);
    let mut command = Command::new(topgrade_bin);
    command.args(&argv[1..]);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .and_then(|mut f| writeln!(f, "failed to spawn topgrade: {error}"));
            return failed_outcome(profile, dry_run, started, log_path);
        }
    };

    let log_out = match OpenOptions::new().create(true).append(true).open(log_path) {
        Ok(handle) => handle,
        Err(_) => return failed_outcome(profile, dry_run, started, log_path),
    };
    let log_err = match OpenOptions::new().create(true).append(true).open(log_path) {
        Ok(handle) => handle,
        Err(_) => return failed_outcome(profile, dry_run, started, log_path),
    };
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let stdout_terminal = io::stdout();
    let stderr_terminal = io::stdout();
    let out_handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut writer = Writer::new(BufWriter::new(log_out), stdout_terminal, !quiet);
        let _ = io::copy(&mut reader, &mut writer);
        let _ = writer.flush();
    });
    let err_handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut writer = Writer::new(BufWriter::new(log_err), stderr_terminal, !quiet);
        let _ = io::copy(&mut reader, &mut writer);
        let _ = writer.flush();
    });

    let status = match child.wait() {
        Ok(status) => status,
        Err(error) => {
            if let Ok(mut err_log) = OpenOptions::new().create(true).append(true).open(log_path) {
                let _ = writeln!(err_log, "wait failed: {error}");
            }
            return failed_outcome(profile, dry_run, started, log_path);
        }
    };
    let _ = out_handle.join();
    let _ = err_handle.join();

    let finished = Utc::now();
    RunOutcome {
        profile: profile.name.clone(),
        dry_run,
        skipped: false,
        success: status.success(),
        exit_code: status.code(),
        started_at: started,
        finished_at: finished,
        duration_secs: (finished - started).num_milliseconds() as f64 / 1000.0,
        log_path: log_path.to_path_buf(),
    }
}

fn failed_outcome(
    profile: &Profile,
    dry_run: bool,
    started: DateTime<Utc>,
    log_path: &Path,
) -> RunOutcome {
    RunOutcome {
        profile: profile.name.clone(),
        dry_run,
        skipped: false,
        success: false,
        exit_code: None,
        started_at: started,
        finished_at: Utc::now(),
        duration_secs: 0.0,
        log_path: log_path.to_path_buf(),
    }
}

fn write_status(paths: &Paths, profile: &str, outcome: &RunOutcome) -> anyhow::Result<()> {
    let path = paths.status_file(profile);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("status.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(outcome)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn read_status(paths: &Paths, profile: &str) -> anyhow::Result<Option<RunOutcome>> {
    let path = paths.status_file(profile);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

struct Writer<A: Write, B: Write> {
    out: A,
    terminal: B,
    tee: bool,
}

impl<A: Write, B: Write> Writer<A, B> {
    fn new(out: A, terminal: B, tee: bool) -> Self {
        Self { out, terminal, tee }
    }
}

impl<A: Write, B: Write> Write for Writer<A, B> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.out.write_all(buf)?;
        if self.tee {
            self.terminal.write_all(buf)?;
            self.terminal.flush()?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writer(tee: bool) -> (Writer<BufWriter<std::fs::File>, Vec<u8>>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("capture.log");
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log)
            .unwrap();
        (Writer::new(BufWriter::new(file), Vec::new(), tee), dir)
    }

    #[test]
    fn quiet_writer_logs_but_never_reaches_the_terminal() {
        let (mut writer, dir) = writer(false);
        writer.write_all(b"topgrade output\n").unwrap();
        writer.flush().unwrap();
        drop(writer);

        let log = dir.path().join("capture.log");
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "topgrade output\n",
            "quiet mode must still persist the full log"
        );
    }

    #[test]
    fn quiet_writer_keeps_the_terminal_buffer_empty() {
        let (mut writer, _dir) = writer(false);
        writer.write_all(b"topgrade output\n").unwrap();
        writer.flush().unwrap();
        assert!(
            writer.terminal.is_empty(),
            "quiet mode must not push a single byte toward the TUI's stdout"
        );
    }

    #[test]
    fn loud_writer_tees_log_and_terminal() {
        let (mut writer, dir) = writer(true);
        writer.write_all(b"topgrade output\n").unwrap();
        writer.flush().unwrap();

        assert_eq!(writer.terminal, b"topgrade output\n".to_vec());
        drop(writer);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("capture.log")).unwrap(),
            "topgrade output\n"
        );
    }
}
