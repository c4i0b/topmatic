use chrono::{DateTime, NaiveDateTime, Utc};
use std::io;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::domain::profile::Scope;

pub mod reset;
pub mod sync;
pub mod units;

#[cfg(test)]
pub(crate) mod test_support;

pub trait SystemdCtl: Send {
    fn unit_dir(&self) -> PathBuf;
    fn daemon_reload(&self) -> io::Result<()>;
    fn enable_timer(&self, profile: &str) -> io::Result<()>;
    fn disable_timer(&self, profile: &str) -> io::Result<()>;
    fn start_service(&self, profile: &str) -> io::Result<()>;
    fn stop_all(&self) -> io::Result<()>;
    fn instances(&self) -> Vec<String>;
    fn next_run(&self, profile: &str) -> Option<DateTime<Utc>>;
    fn timer_active(&self, profile: &str) -> bool;
    fn linger_enabled(&self) -> Option<bool>;
    fn service_active(&self, profile: &str) -> bool;
    fn service_since(&self, profile: &str) -> Option<DateTime<Utc>>;
    fn stop_service(&self, profile: &str) -> io::Result<()>;
}

pub struct RealSystemdCtl {
    home: PathBuf,
    sink: Arc<dyn Fn(&str) + Send + Sync>,
}

impl RealSystemdCtl {
    pub fn new(home: PathBuf) -> Self {
        Self::with_sink(home, Arc::new(|_| {}))
    }

    pub fn with_sink(home: PathBuf, sink: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        Self { home, sink }
    }

    fn systemctl(&self, args: &[&str]) -> Command {
        let mut command = Command::new("systemctl");
        command.arg("--user");
        command.args(args);
        command
    }

    fn record(&self, command: Command) -> Command {
        (self.sink)(&command_line(&command));
        command
    }
}

impl SystemdCtl for RealSystemdCtl {
    fn unit_dir(&self) -> PathBuf {
        units::scope_dirs(Scope::User, &self.home).units_dir
    }

    fn daemon_reload(&self) -> io::Result<()> {
        run_status(self.record(self.systemctl(&["daemon-reload"])))
    }

    fn enable_timer(&self, profile: &str) -> io::Result<()> {
        run_status(self.record(self.systemctl(&[
            "enable",
            "--now",
            &units::timer_instance(profile),
        ])))
    }

    fn disable_timer(&self, profile: &str) -> io::Result<()> {
        run_status(self.record(self.systemctl(&[
            "disable",
            "--now",
            &units::timer_instance(profile),
        ])))
    }

    fn start_service(&self, profile: &str) -> io::Result<()> {
        let instance = format!("topmatic@{profile}.service");
        run_status(self.record(self.systemctl(&["start", &instance])))
    }

    fn stop_all(&self) -> io::Result<()> {
        run_status(self.record(self.systemctl(&["stop", "topmatic@*.service", "topmatic@*.timer"])))
    }

    fn instances(&self) -> Vec<String> {
        crate::systemd::units::enabled_instances(&self.unit_dir())
    }

    fn next_run(&self, profile: &str) -> Option<DateTime<Utc>> {
        let output = run_output(self.systemctl(&[
            "show",
            &units::timer_instance(profile),
            "-p",
            "NextElapseUSecRealtime",
            "--value",
        ]))?;
        if !output.status.success() {
            return None;
        }
        parse_systemd_timestamp(String::from_utf8_lossy(&output.stdout).trim())
    }

    fn timer_active(&self, profile: &str) -> bool {
        run_output(self.systemctl(&["is-active", &units::timer_instance(profile)]))
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn service_active(&self, profile: &str) -> bool {
        let unit = units::timer_instance(profile).replace(".timer", ".service");
        run_output(self.systemctl(&["is-active", &unit]))
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn service_since(&self, profile: &str) -> Option<DateTime<Utc>> {
        let unit = units::timer_instance(profile).replace(".timer", ".service");
        let output =
            run_output(self.systemctl(&["show", &unit, "-p", "ActiveEnterTimestamp", "--value"]))?;
        parse_systemd_timestamp(String::from_utf8_lossy(&output.stdout).trim())
    }

    fn stop_service(&self, profile: &str) -> io::Result<()> {
        let unit = units::timer_instance(profile).replace(".timer", ".service");
        run_status(self.record(self.systemctl(&["stop", &unit])))
    }

    fn linger_enabled(&self) -> Option<bool> {
        let user = std::env::var("USER").ok()?;
        let mut command = Command::new("loginctl");
        command.args(["show-user", &user, "-p", "Linger", "--value"]);
        let output = run_output(command)?;
        if !output.status.success() {
            return None;
        }
        match String::from_utf8_lossy(&output.stdout).trim() {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        }
    }
}

const SYSTEMD_TIMEOUT: Duration = Duration::from_secs(10);

fn command_line(command: &Command) -> String {
    use std::ffi::OsStr;
    let program = command.get_program();
    let args: Vec<&OsStr> = command.get_args().collect();
    let mut parts: Vec<String> = vec![program.to_string_lossy().into_owned()];
    parts.extend(
        args.into_iter()
            .map(|arg| arg.to_string_lossy().into_owned()),
    );
    parts.join(" ")
}

fn run_status(mut command: Command) -> io::Result<()> {
    run_status_with_timeout(&mut command, SYSTEMD_TIMEOUT)
}

fn run_status_with_timeout(command: &mut Command, timeout: Duration) -> io::Result<()> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    match wait_timeout(&mut child, timeout)? {
        Some(status) if status.success() => Ok(()),
        Some(status) => Err(io::Error::other(format!("command failed with {status}"))),
        None => Err(io::Error::other(format!(
            "timed out after {}s",
            timeout.as_secs()
        ))),
    }
}

fn run_output(command: Command) -> Option<Output> {
    run_output_with_timeout(command, SYSTEMD_TIMEOUT)
}

fn run_output_with_timeout(mut command: Command, timeout: Duration) -> Option<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    match wait_timeout(&mut child, timeout) {
        Ok(Some(_)) => {}
        Ok(None) | Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    }
    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_end(&mut stdout);
    }
    let status = child.wait().ok()?;
    Some(Output {
        status,
        stdout,
        stderr: Vec::new(),
    })
}

fn wait_timeout(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    child.wait()?;
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn parse_systemd_timestamp(value: &str) -> Option<DateTime<Utc>> {
    let value = strip_weekday_prefix(value.trim());
    let (datetime, timezone) = value.rsplit_once(' ')?;
    let naive = NaiveDateTime::parse_from_str(datetime, "%Y-%m-%d %H:%M:%S").ok()?;
    match timezone {
        "UTC" | "GMT" => Some(naive.and_utc()),
        offset if parse_offset_minutes(offset).is_some() => {
            let offset = parse_offset_minutes(offset).unwrap();
            Some((naive - chrono::Duration::minutes(offset)).and_utc())
        }
        _ => {
            use chrono::TimeZone;
            chrono::Local
                .from_local_datetime(&naive)
                .single()
                .map(|local| local.with_timezone(&Utc))
        }
    }
}

fn strip_weekday_prefix(value: &str) -> &str {
    const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    for weekday in WEEKDAYS {
        if let Some(rest) = value.strip_prefix(weekday) {
            return rest.trim_start();
        }
    }
    value
}

fn parse_offset_minutes(timezone: &str) -> Option<i64> {
    let (sign, digits) = timezone.split_at(1);
    let sign = match sign {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let (hours, minutes) = match digits.len() {
        2 => (digits.parse::<i64>().ok()?, 0),
        4 => (
            digits[..2].parse::<i64>().ok()?,
            digits[2..].parse::<i64>().ok()?,
        ),
        _ => return None,
    };
    Some(sign * (hours * 60 + minutes))
}

pub fn validate_on_calendar(calendar: &str) -> Result<(), String> {
    let Ok(output) = Command::new("systemd-analyze")
        .args(["calendar", "--"])
        .arg(calendar)
        .output()
    else {
        return Ok(());
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("invalid calendar expression {calendar:?}")
        } else {
            stderr
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn parses_next_elapse_timestamps() {
        assert_eq!(
            parse_systemd_timestamp("2026-09-06 12:34:56 UTC"),
            Some(
                NaiveDateTime::parse_from_str("2026-09-06 12:34:56", "%Y-%m-%d %H:%M:%S")
                    .unwrap()
                    .and_utc()
            )
        );
        assert_eq!(
            parse_systemd_timestamp("Sun 2026-09-06 11:56:39 -03"),
            Some(
                NaiveDateTime::parse_from_str("2026-09-06 14:56:39", "%Y-%m-%d %H:%M:%S")
                    .unwrap()
                    .and_utc()
            )
        );
        assert_eq!(
            parse_systemd_timestamp("2026-09-06 06:26:39 +0530"),
            Some(
                NaiveDateTime::parse_from_str("2026-09-06 00:56:39", "%Y-%m-%d %H:%M:%S")
                    .unwrap()
                    .and_utc()
            )
        );
        assert!(
            parse_systemd_timestamp("Wed 2026-01-14 09:00:00 CET").is_some(),
            "abbreviation timezones fall back to local interpretation"
        );
        assert_eq!(parse_systemd_timestamp("n/a"), None);
        assert_eq!(parse_systemd_timestamp(""), None);
    }

    #[test]
    fn validates_calendar_expressions_when_tool_exists() {
        if which_systemd_analyze().is_none() {
            return;
        }
        assert!(validate_on_calendar("*-*-* 12:00:00").is_ok());
        assert!(validate_on_calendar("definitely not a calendar").is_err());
    }

    fn which_systemd_analyze() -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("systemd-analyze"))
            .find(|candidate| candidate.is_file())
    }

    #[test]
    fn run_status_reports_success_and_failure() {
        let mut ok = Command::new("/bin/true");
        assert!(run_status_with_timeout(&mut ok, Duration::from_secs(5)).is_ok());
        let mut fail = Command::new("/bin/false");
        let error = run_status_with_timeout(&mut fail, Duration::from_secs(5))
            .unwrap_err()
            .to_string();
        assert!(error.contains("command failed"), "got: {error}");
    }

    #[test]
    fn run_status_kills_a_hanging_command() {
        let mut command = Command::new("sleep");
        command.arg("60");
        let error = run_status_with_timeout(&mut command, Duration::from_millis(150))
            .unwrap_err()
            .to_string();
        assert!(error.contains("timed out"), "got: {error}");
    }

    #[test]
    fn command_line_formats_program_and_args_in_order() {
        let ctl = RealSystemdCtl::new(PathBuf::from("/home/user"));
        for (args, expected) in [
            (
                &["enable", "--now", "topmatic@all-daily.timer"][..],
                "systemctl --user enable --now topmatic@all-daily.timer".to_string(),
            ),
            (
                &["daemon-reload"][..],
                "systemctl --user daemon-reload".to_string(),
            ),
            (
                &["stop", "topmatic@*.service", "topmatic@*.timer"][..],
                "systemctl --user stop topmatic@*.service topmatic@*.timer".to_string(),
            ),
        ] {
            let line = command_line(&ctl.systemctl(args));
            assert_eq!(line, expected, "for args {args:?}");
        }
    }

    #[test]
    fn record_forwards_the_command_unchanged_after_sinking_it() {
        let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink: Arc<dyn Fn(&str) + Send + Sync> = {
            let calls = Arc::clone(&calls);
            Arc::new(move |line| calls.lock().unwrap().push(line.to_string()))
        };
        let ctl = RealSystemdCtl::with_sink(PathBuf::from("/home/user"), sink);
        let original = ctl.systemctl(&["enable", "--now", "topmatic@all-daily.timer"]);
        let forwarded = ctl.record(original);
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &["systemctl --user enable --now topmatic@all-daily.timer".to_string()],
            "the sink receives the command line before execution"
        );
        assert_eq!(forwarded.get_program(), "systemctl");
        let args: Vec<_> = forwarded.get_args().map(|a| a.to_string_lossy()).collect();
        assert_eq!(
            args,
            vec!["--user", "enable", "--now", "topmatic@all-daily.timer"]
        );
    }

    #[test]
    fn run_status_silences_child_stdout_and_stderr() {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("printf 'created symlink noise\\n'; echo noise >&2; exit 3");
        let error = run_status_with_timeout(&mut command, Duration::from_secs(5))
            .unwrap_err()
            .to_string();
        assert!(error.contains("command failed"), "got: {error}");
    }

    #[test]
    fn run_output_captures_stdout_and_kills_a_hanging_command() {
        let mut ok = Command::new("/bin/echo");
        ok.arg("hello");
        let output = run_output_with_timeout(ok, Duration::from_secs(5)).unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");

        let mut hang = Command::new("sleep");
        hang.arg("60");
        assert!(run_output_with_timeout(hang, Duration::from_millis(150)).is_none());
    }
}
