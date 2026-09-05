use std::path::Path;

use crate::domain::profile::Scope;
use crate::domain::schedule::Schedule;

pub const SERVICE_TEMPLATE: &str = "topmatic@.service";
pub const TIMER_TEMPLATE: &str = "topmatic@.timer";

pub struct ScopeDirs {
    pub units_dir: PathBuf,
    pub systemctl_user_args: Vec<String>,
}

pub fn scope_dirs(scope: Scope, home: &Path) -> ScopeDirs {
    match scope {
        Scope::User => ScopeDirs {
            units_dir: home.join(".config/systemd/user"),
            systemctl_user_args: vec!["--user".to_string()],
        },
        Scope::System => ScopeDirs {
            units_dir: PathBuf::from("/etc/systemd/system"),
            systemctl_user_args: Vec::new(),
        },
    }
}

pub fn timer_instance(profile: &str) -> String {
    format!("topmatic@{profile}.timer")
}

pub fn drop_in_dir(profile: &str) -> String {
    format!("topmatic@{profile}.timer.d")
}

pub fn service_unit(topmatic_bin: &Path, scope: Scope) -> String {
    let path_env = match scope {
        Scope::User => "%h/.cargo/bin:/usr/local/bin:/usr/bin:/bin",
        Scope::System => "/usr/local/bin:/usr/bin:/bin",
    };
    format!(
        "[Unit]\n\
         Description=topmatic run for profile %i\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={} run %i\n\
         Environment=PATH={path_env}\n\
         Nice=19\n\
         CPUSchedulingPolicy=batch\n\
         IOSchedulingClass=idle\n",
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
    format!(
        "[Timer]\n\
         OnCalendar={}\n\
         RandomizedDelaySec={}\n",
        schedule.preset.on_calendar(),
        schedule.randomized_delay_sec
    )
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
        let dirs = scope_dirs(Scope::User, Path::new("/home/caio"));
        assert_eq!(
            dirs.units_dir,
            PathBuf::from("/home/caio/.config/systemd/user")
        );
        assert_eq!(dirs.systemctl_user_args, vec!["--user".to_string()]);
    }

    #[test]
    fn scope_dirs_target_system_units_for_future_support() {
        let dirs = scope_dirs(Scope::System, Path::new("/home/caio"));
        assert_eq!(dirs.units_dir, PathBuf::from("/etc/systemd/system"));
        assert!(dirs.systemctl_user_args.is_empty());
    }

    #[test]
    fn service_unit_references_absolute_binary_and_instance() {
        let unit = service_unit(Path::new("/home/caio/.cargo/bin/topmatic"), Scope::User);
        assert!(unit.contains("ExecStart=/home/caio/.cargo/bin/topmatic run %i"));
        assert!(unit.contains("Environment=PATH=%h/.cargo/bin:/usr/local/bin:/usr/bin:/bin"));
        assert!(unit.contains("Type=oneshot"));
    }

    #[test]
    fn service_unit_runs_at_minimum_priority() {
        let unit = service_unit(Path::new("/bin/topmatic"), Scope::User);
        assert!(unit.contains("Nice=19"));
        assert!(unit.contains("CPUSchedulingPolicy=batch"));
        assert!(unit.contains("IOSchedulingClass=idle"));
    }

    #[test]
    fn timer_unit_is_persistent_and_installed() {
        let unit = timer_unit();
        assert!(unit.contains("Persistent=true"));
        assert!(unit.contains("WantedBy=timers.target"));
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
