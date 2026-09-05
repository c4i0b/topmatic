use chrono::{DateTime, NaiveDateTime, Utc};
use std::io;
use std::path::PathBuf;
use std::process::Command;

use crate::domain::profile::Scope;

pub mod sync;
pub mod units;

pub trait SystemdCtl {
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
    fn enable_linger(&self) -> io::Result<()>;
}

pub struct RealSystemdCtl {
    home: PathBuf,
}

impl RealSystemdCtl {
    pub fn new(home: PathBuf) -> Self {
        Self { home }
    }

    fn systemctl(&self, args: &[&str]) -> Command {
        let mut command = Command::new("systemctl");
        command.arg("--user");
        command.args(args);
        command
    }
}

impl SystemdCtl for RealSystemdCtl {
    fn unit_dir(&self) -> PathBuf {
        units::scope_dirs(Scope::User, &self.home).units_dir
    }

    fn daemon_reload(&self) -> io::Result<()> {
        run_status(self.systemctl(&["daemon-reload"]))
    }

    fn enable_timer(&self, profile: &str) -> io::Result<()> {
        run_status(self.systemctl(&["enable", "--now", &units::timer_instance(profile)]))
    }

    fn disable_timer(&self, profile: &str) -> io::Result<()> {
        run_status(self.systemctl(&["disable", "--now", &units::timer_instance(profile)]))
    }

    fn start_service(&self, profile: &str) -> io::Result<()> {
        let instance = format!("topmatic@{profile}.service");
        run_status(self.systemctl(&["start", &instance]))
    }

    fn stop_all(&self) -> io::Result<()> {
        run_status(self.systemctl(&["stop", "topmatic@*.service", "topmatic@*.timer"]))
    }

    fn instances(&self) -> Vec<String> {
        let output = self
            .systemctl(&[
                "list-unit-files",
                "topmatic@*.timer",
                "--no-legend",
                "--no-pager",
                "--plain",
            ])
            .output();
        let Ok(output) = output else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .filter_map(units::parse_instance)
            .map(str::to_string)
            .collect()
    }

    fn next_run(&self, profile: &str) -> Option<DateTime<Utc>> {
        let output = self
            .systemctl(&[
                "show",
                &units::timer_instance(profile),
                "-p",
                "NextElapseUSecRealtime",
                "--value",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_systemd_timestamp(String::from_utf8_lossy(&output.stdout).trim())
    }

    fn timer_active(&self, profile: &str) -> bool {
        self.systemctl(&["is-active", &units::timer_instance(profile)])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn linger_enabled(&self) -> Option<bool> {
        let user = std::env::var("USER").ok()?;
        let output = Command::new("loginctl")
            .args(["show-user", &user, "-p", "Linger", "--value"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        match String::from_utf8_lossy(&output.stdout).trim() {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        }
    }

    fn enable_linger(&self) -> io::Result<()> {
        let user = std::env::var("USER")
            .map_err(|_| io::Error::other("USER environment variable is not set"))?;
        let mut command = Command::new("loginctl");
        command.arg("enable-linger").arg(&user);
        run_status(command)
    }
}

fn run_status(mut command: Command) -> io::Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("command failed with {status}")))
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
}
