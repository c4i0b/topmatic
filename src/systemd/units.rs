use std::path::Path;

use crate::domain::schedule::Schedule;

pub const SERVICE_TEMPLATE: &str = "topmatic@.service";
pub const TIMER_TEMPLATE: &str = "topmatic@.timer";
pub const SCHEDULE_DROP_IN: &str = "10-schedule.conf";

pub struct ScopeDirs {
    pub units_dir: PathBuf,
    pub systemctl_user_args: Vec<String>,
}

pub fn scope_dirs(home: &Path) -> ScopeDirs {
    ScopeDirs {
        units_dir: home.join(".config/systemd/user"),
        systemctl_user_args: vec!["--user".to_string()],
    }
}

pub fn timer_instance(profile: &str) -> String {
    format!("topmatic@{profile}.timer")
}

pub fn enabled_instances(units_dir: &Path) -> Vec<String> {
    let wants_dir = units_dir.join("timers.target.wants");
    let Ok(entries) = std::fs::read_dir(&wants_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.ends_with(".timer"))
        .filter_map(|name| parse_instance(&name).map(str::to_string))
        .collect();
    names.sort();
    names.dedup();
    names
}

pub fn drop_in_dir(profile: &str) -> String {
    format!("topmatic@{profile}.timer.d")
}

pub fn service_unit(topmatic_bin: &Path) -> String {
    let path_env = "%h/.cargo/bin:%h/.local/bin:/usr/local/bin:/usr/bin:/bin";
    format!(
        "[Unit]\n\
         Description=topmatic run for profile %i\n\
         ConditionACPower=true\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={} run %i\n\
         Environment=PATH={path_env}\n\
         Nice=19\n\
         CPUSchedulingPolicy=batch\n\
         IOSchedulingClass=idle\n\
         TimeoutStartSec=90min\n",
        topmatic_bin.display()
    )
}

pub fn timer_unit() -> String {
    "[Unit]\n\
     Description=topmatic timer for profile %i\n\
     \n\
     [Timer]\n\
     Persistent=true\n\
     \n\
     [Install]\n\
     WantedBy=timers.target\n"
        .to_string()
}

pub fn timer_drop_in(schedule: &Schedule) -> String {
    let mut body = String::from("[Timer]\n");
    for spec in schedule.preset.on_calendar_specs() {
        body.push_str(&format!("OnCalendar={spec}\n"));
    }
    body.push_str(&format!(
        "RandomizedDelaySec={}\n",
        schedule.randomized_delay_sec
    ));
    body
}

pub fn parse_instance(unit_name: &str) -> Option<&str> {
    let instance = unit_name
        .strip_prefix("topmatic@")
        .and_then(|rest| rest.strip_suffix(".timer"))
        .or_else(|| {
            unit_name
                .strip_prefix("topmatic@")
                .and_then(|rest| rest.strip_suffix(".timer.d"))
        })?;
    if instance.is_empty() {
        return None;
    }
    Some(instance)
}

use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::schedule::{SchedulePreset, Weekday};

    #[test]
    fn scope_dirs_target_user_units_without_root() {
        let dirs = scope_dirs(Path::new("/home/caio"));
        assert_eq!(
            dirs.units_dir,
            PathBuf::from("/home/caio/.config/systemd/user")
        );
        assert_eq!(dirs.systemctl_user_args, vec!["--user".to_string()]);
    }

    #[test]
    fn service_unit_references_absolute_binary_and_instance() {
        let unit = service_unit(Path::new("/home/caio/.cargo/bin/topmatic"));
        assert!(unit.contains("ExecStart=/home/caio/.cargo/bin/topmatic run %i"));
        assert!(
            unit.contains(
                "Environment=PATH=%h/.cargo/bin:%h/.local/bin:/usr/local/bin:/usr/bin:/bin"
            )
        );
        assert!(unit.contains("Type=oneshot"));
    }

    #[test]
    fn service_unit_runs_at_minimum_priority() {
        let unit = service_unit(Path::new("/bin/topmatic"));
        assert!(unit.contains("Nice=19"));
        assert!(unit.contains("CPUSchedulingPolicy=batch"));
        assert!(unit.contains("IOSchedulingClass=idle"));
        assert!(unit.contains("TimeoutStartSec=90min"));
        assert!(unit.contains("ConditionACPower=true"));
    }

    #[test]
    fn timer_unit_is_persistent_and_installed() {
        let unit = timer_unit();
        assert!(unit.contains("Persistent=true"));
        assert!(unit.contains("WantedBy=timers.target"));
    }

    #[test]
    fn drop_in_writes_one_oncalendar_line_per_spec() {
        let schedule = Schedule {
            preset: SchedulePreset::Biweekly,
            randomized_delay_sec: 300,
        };
        let drop_in = timer_drop_in(&schedule);
        assert_eq!(
            drop_in,
            "[Timer]\nOnCalendar=Mon *-*-1..7 00:00:00\nOnCalendar=Mon *-*-15..21 00:00:00\nRandomizedDelaySec=300\n"
        );
    }

    #[test]
    fn drop_in_carries_schedule_and_delay() {
        let schedule = Schedule {
            preset: SchedulePreset::Weekly {
                weekday: Weekday::Mon,
                hour: 9,
                minute: 30,
            },
            randomized_delay_sec: 300,
        };
        let drop_in = timer_drop_in(&schedule);
        assert!(drop_in.contains("OnCalendar=Mon *-*-* 09:30:00"));
        assert!(drop_in.contains("RandomizedDelaySec=300"));
    }

    #[test]
    fn enables_instances_from_wants_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let wants_dir = dir.path().join("timers.target.wants");
        std::fs::create_dir_all(&wants_dir).unwrap();
        for unit in ["topmatic@all-daily.timer", "topmatic@dev-daily.timer"] {
            std::os::unix::fs::symlink("../topmatic@.timer", wants_dir.join(unit)).unwrap();
        }
        std::fs::write(wants_dir.join("topmatic@stale-daily.timer.d"), b"ignored").unwrap();
        std::fs::write(dir.path().join("topmatic@inactive.timer"), b"not enabled").unwrap();
        assert_eq!(
            enabled_instances(dir.path()),
            vec!["all-daily", "dev-daily"]
        );
    }

    #[test]
    fn parses_instance_names_from_units_and_drop_in_dirs() {
        assert_eq!(
            parse_instance("topmatic@flatpak-daily.timer"),
            Some("flatpak-daily")
        );
        assert_eq!(
            parse_instance("topmatic@flatpak-daily.timer.d"),
            Some("flatpak-daily")
        );
        assert_eq!(
            parse_instance("topmatic@.timer"),
            None,
            "template is not an instance"
        );
        assert_eq!(parse_instance("other@x.timer"), None);
    }
}
