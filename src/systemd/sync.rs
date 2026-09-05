use std::collections::BTreeSet;
use std::fs;
use std::io;

#[cfg(test)]
use chrono::{DateTime, Utc};
#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::path::PathBuf;

use crate::config::AppConfig;
use crate::domain::profile::Scope;
use crate::domain::schedule::SchedulePreset;
use crate::paths::Paths;
use crate::util::write_file_if_changed;

use super::SystemdCtl;
use super::units;

#[derive(Debug, Default, PartialEq)]
pub struct SyncReport {
    pub templates_installed: bool,
    pub updated_profiles: Vec<String>,
    pub pruned_drop_ins: Vec<String>,
    pub removed_orphans: Vec<String>,
    pub reloaded: bool,
    pub errors: Vec<String>,
}

impl SyncReport {
    pub fn is_clean(&self) -> bool {
        !self.templates_installed
            && self.updated_profiles.is_empty()
            && self.pruned_drop_ins.is_empty()
            && self.removed_orphans.is_empty()
            && self.errors.is_empty()
    }
}

pub fn sync(
    config: &AppConfig,
    topmatic_bin: &std::path::Path,
    ctl: &dyn SystemdCtl,
) -> SyncReport {
    let mut report = SyncReport::default();
    let unit_dir = ctl.unit_dir();

    let service = units::service_unit(topmatic_bin, Scope::User);
    let timer = units::timer_unit();
    match write_file_if_changed(&unit_dir.join(units::SERVICE_TEMPLATE), &service).and_then(
        |changed_service| {
            write_file_if_changed(&unit_dir.join(units::TIMER_TEMPLATE), &timer)
                .map(|changed_timer| changed_service || changed_timer)
        },
    ) {
        Ok(true) => report.templates_installed = true,
        Ok(false) => {}
        Err(error) => report
            .errors
            .push(format!("failed to install unit templates: {error}")),
    }

    let known: BTreeSet<String> = sync_profiles(config, &unit_dir, ctl, &mut report);

    let mut orphans: BTreeSet<String> = BTreeSet::new();
    remove_orphan_drop_ins(&unit_dir, &known, &mut orphans);
    remove_orphan_timers(&known, ctl, &mut orphans);
    report.removed_orphans = orphans.into_iter().collect();

    let changed_anything = report.templates_installed
        || !report.updated_profiles.is_empty()
        || !report.pruned_drop_ins.is_empty()
        || !report.removed_orphans.is_empty();
    if changed_anything {
        match ctl.daemon_reload() {
            Ok(()) => report.reloaded = true,
            Err(error) => report.errors.push(format!("daemon-reload failed: {error}")),
        }
    }
    report
}

fn sync_profiles(
    config: &AppConfig,
    unit_dir: &std::path::Path,
    ctl: &dyn SystemdCtl,
    report: &mut SyncReport,
) -> BTreeSet<String> {
    let mut known = BTreeSet::new();
    for profile in &config.profiles {
        if profile.scope == Scope::System {
            report.errors.push(format!(
                "{}: system scope is not supported yet",
                profile.name
            ));
            continue;
        }
        known.insert(profile.name.clone());
        if let SchedulePreset::Custom { calendar } = &profile.schedule.preset
            && let Err(error) = super::validate_on_calendar(calendar)
        {
            let _ = ctl.disable_timer(&profile.name);
            report.errors.push(format!(
                "{}: invalid OnCalendar, timer disabled: {error}",
                profile.name
            ));
            continue;
        }
        let drop_dir = unit_dir.join(units::drop_in_dir(&profile.name));
        let drop_in_path = drop_dir.join(units::SCHEDULE_DROP_IN);
        match write_file_if_changed(&drop_in_path, &units::timer_drop_in(&profile.schedule)) {
            Ok(true) => report.updated_profiles.push(profile.name.clone()),
            Ok(false) => {}
            Err(error) => {
                report
                    .errors
                    .push(format!("{}: drop-in write failed: {error}", profile.name));
                continue;
            }
        }
        if prune_stray_drop_ins(&drop_dir, &profile.name) {
            report.pruned_drop_ins.push(profile.name.clone());
        }
        if let Err(error) = ctl.enable_timer(&profile.name) {
            report
                .errors
                .push(format!("{}: timer state failed: {error}", profile.name));
        }
    }
    known
}

fn prune_stray_drop_ins(drop_dir: &std::path::Path, _profile: &str) -> bool {
    let entries = match fs::read_dir(drop_dir) {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    let mut pruned = false;
    for entry in entries.flatten() {
        let is_stray = entry.path().extension().is_some_and(|ext| ext == "conf")
            && entry.file_name().to_string_lossy() != units::SCHEDULE_DROP_IN;
        if is_stray && fs::remove_file(entry.path()).is_ok() {
            pruned = true;
        }
    }
    pruned
}

fn remove_orphan_drop_ins(
    unit_dir: &std::path::Path,
    known: &BTreeSet<String>,
    orphans: &mut BTreeSet<String>,
) {
    let entries = match fs::read_dir(unit_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("topmatic@") || !name.ends_with(".timer.d") {
            continue;
        }
        if let Some(instance) = units::parse_instance(&name)
            && !known.contains(instance)
            && fs::remove_dir_all(entry.path()).is_ok()
        {
            orphans.insert(instance.to_string());
        }
    }
}

fn remove_orphan_timers(
    known: &BTreeSet<String>,
    ctl: &dyn SystemdCtl,
    orphans: &mut BTreeSet<String>,
) {
    for instance in ctl.instances() {
        if known.contains(&instance) {
            continue;
        }
        let _ = ctl.disable_timer(&instance);
        orphans.insert(instance);
    }
}

pub fn purge_profile_state(paths: &Paths, profile: &str) -> io::Result<()> {
    let _ = fs::remove_dir_all(paths.logs_dir(profile));
    let _ = fs::remove_file(paths.status_file(profile));
    let _ = fs::remove_file(paths.lock_file(profile));
    Ok(())
}

#[derive(Debug, Default, PartialEq)]
pub struct ResetReport {
    pub removed_orphans: Vec<String>,
    pub removed_units: bool,
    pub purged_state: bool,
    pub config_backup: Option<std::path::PathBuf>,
}

pub fn reset(ctl: &dyn SystemdCtl, paths: &Paths, include_config: bool) -> ResetReport {
    let mut report = ResetReport::default();
    let unit_dir = ctl.unit_dir();

    for instance in ctl.instances() {
        let _ = ctl.disable_timer(&instance);
        report.removed_orphans.push(instance);
    }
    let _ = ctl.stop_all();

    let mut removed_units = false;
    let entries = fs::read_dir(&unit_dir)
        .map(|read_dir| read_dir.flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let managed = name.starts_with("topmatic@")
            && (name.ends_with(".timer.d")
                || name == units::SERVICE_TEMPLATE
                || name == units::TIMER_TEMPLATE);
        if !managed {
            continue;
        }
        let removed = match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => fs::remove_dir_all(entry.path()).is_ok(),
            Ok(_) => fs::remove_file(entry.path()).is_ok(),
            Err(_) => false,
        };
        removed_units = removed_units || removed;
    }
    report.removed_units = removed_units;

    if paths.state_dir.exists() && fs::remove_dir_all(&paths.state_dir).is_ok() {
        report.purged_state = true;
    }

    if include_config {
        let config_file = paths.config_file();
        if config_file.exists() {
            let backup = config_file.with_extension("toml.bak");
            if fs::rename(&config_file, &backup).is_ok() {
                report.config_backup = Some(backup);
            }
        }
    }

    let _ = ctl.daemon_reload();
    report
}

#[cfg(test)]
pub struct FakeCtl {
    dir: PathBuf,
    calls: RefCell<Vec<String>>,
    pub existing_instances: Vec<String>,
    pub linger: Option<bool>,
    pub fail_enable_for: Option<String>,
}

#[cfg(test)]
impl FakeCtl {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            calls: RefCell::new(Vec::new()),
            existing_instances: Vec::new(),
            linger: Some(false),
            fail_enable_for: None,
        }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn record(&self, call: String) {
        self.calls.borrow_mut().push(call);
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
            .borrow()
            .iter()
            .any(|call| call == &format!("enable:{profile}"))
    }

    fn linger_enabled(&self) -> Option<bool> {
        self.linger
    }

    fn enable_linger(&self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::{NotifyPolicy, Profile};
    use crate::domain::schedule::{Schedule, SchedulePreset};

    fn profile(name: &str) -> Profile {
        Profile {
            name: name.to_string(),
            steps: vec!["flatpak".to_string()],
            schedule: Schedule {
                preset: SchedulePreset::Daily {
                    hour: 12,
                    minute: 0,
                },
                randomized_delay_sec: 900,
            },
            cleanup: true,
            notify: NotifyPolicy::OnFailure,
            scope: Scope::User,
        }
    }

    fn config_of(profiles: &[Profile]) -> AppConfig {
        let mut config = AppConfig::default();
        for profile in profiles {
            config.upsert(profile.clone());
        }
        config
    }

    #[test]
    fn installs_templates_drop_ins_and_ensures_timer_state() {
        let tmp = tempfile::tempdir().unwrap();
        let ctl = FakeCtl::new(tmp.path().to_path_buf());
        let config = config_of(&[profile("alpha"), profile("beta")]);

        let report = sync(&config, std::path::Path::new("/bin/topmatic"), &ctl);

        let service = fs::read_to_string(tmp.path().join("topmatic@.service")).unwrap();
        assert!(service.contains("ExecStart=/bin/topmatic run %i"));
        assert!(tmp.path().join("topmatic@.timer").exists());
        let drop_in =
            fs::read_to_string(tmp.path().join("topmatic@alpha.timer.d/10-schedule.conf")).unwrap();
        assert!(drop_in.contains("OnCalendar=*-*-* 12:00:00"));
        assert!(drop_in.contains("RandomizedDelaySec=900"));

        assert_eq!(report.updated_profiles, vec!["alpha", "beta"]);
        assert!(report.templates_installed);
        assert!(report.reloaded);
        assert!(report.errors.is_empty());
        let calls = ctl.calls();
        assert!(calls.contains(&"enable:alpha".to_string()));
        assert!(calls.contains(&"enable:beta".to_string()));
    }

    #[test]
    fn second_sync_is_clean_and_does_not_reload() {
        let tmp = tempfile::tempdir().unwrap();
        let ctl = FakeCtl::new(tmp.path().to_path_buf());
        let config = config_of(&[profile("alpha")]);

        sync(&config, std::path::Path::new("/bin/topmatic"), &ctl);
        let report = sync(&config, std::path::Path::new("/bin/topmatic"), &ctl);

        assert!(report.is_clean());
        assert!(!report.reloaded);
        assert_eq!(ctl.calls().iter().filter(|c| *c == "reload").count(), 1);
    }

    #[test]
    fn removes_orphan_drop_ins_and_timers() {
        let tmp = tempfile::tempdir().unwrap();
        let orphan_dir = tmp.path().join("topmatic@old.timer.d");
        fs::create_dir_all(&orphan_dir).unwrap();
        fs::write(orphan_dir.join("10-schedule.conf"), "[Timer]\n").unwrap();
        let mut ctl = FakeCtl::new(tmp.path().to_path_buf());
        ctl.existing_instances = vec!["old".to_string()];

        let config = config_of(&[profile("alpha")]);
        let report = sync(&config, std::path::Path::new("/bin/topmatic"), &ctl);

        assert!(!orphan_dir.exists());
        assert_eq!(report.removed_orphans, vec!["old"]);
        assert!(ctl.calls().contains(&"disable:old".to_string()));
    }

    #[test]
    fn system_scope_is_reported_as_unsupported() {
        let tmp = tempfile::tempdir().unwrap();
        let ctl = FakeCtl::new(tmp.path().to_path_buf());
        let mut system_profile = profile("sysjob");
        system_profile.scope = Scope::System;
        let config = config_of(&[system_profile]);

        let report = sync(&config, std::path::Path::new("/bin/topmatic"), &ctl);

        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("system scope is not supported yet"));
    }

    #[test]
    fn enable_failure_is_collected_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctl = FakeCtl::new(tmp.path().to_path_buf());
        ctl.fail_enable_for = Some("alpha".to_string());
        let config = config_of(&[profile("alpha")]);

        let report = sync(&config, std::path::Path::new("/bin/topmatic"), &ctl);

        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("timer state failed"))
        );
    }

    #[test]
    fn purge_removes_logs_status_and_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        fs::create_dir_all(paths.logs_dir("alpha")).unwrap();
        fs::write(paths.logs_dir("alpha").join("x.log"), "log").unwrap();
        fs::create_dir_all(paths.status_file("alpha").parent().unwrap()).unwrap();
        fs::write(paths.status_file("alpha"), "{}").unwrap();
        fs::write(paths.lock_file("alpha"), "").unwrap();

        purge_profile_state(&paths, "alpha").unwrap();

        assert!(!paths.logs_dir("alpha").exists());
        assert!(!paths.status_file("alpha").exists());
        assert!(!paths.lock_file("alpha").exists());
    }

    #[test]
    fn prunes_stray_conf_files_inside_managed_drop_in_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let drop_dir = tmp.path().join("topmatic@alpha.timer.d");
        fs::create_dir_all(&drop_dir).unwrap();
        fs::write(
            drop_dir.join("20-manual.conf"),
            "[Timer]\nOnCalendar=hourly\n",
        )
        .unwrap();
        fs::write(drop_dir.join("notes.txt"), "keep me").unwrap();
        let ctl = FakeCtl::new(tmp.path().to_path_buf());
        let config = config_of(&[profile("alpha")]);

        let report = sync(&config, std::path::Path::new("/bin/topmatic"), &ctl);

        assert!(!drop_dir.join("20-manual.conf").exists());
        assert!(drop_dir.join("notes.txt").exists());
        assert_eq!(report.pruned_drop_ins, vec!["alpha"]);
    }

    #[test]
    fn invalid_custom_calendar_disables_instead_of_enabling() {
        if which_systemd_analyze().is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let ctl = FakeCtl::new(tmp.path().to_path_buf());
        let mut broken = profile("alpha");
        broken.schedule.preset = SchedulePreset::Custom {
            calendar: "definitely not a calendar".to_string(),
        };
        let config = config_of(&[broken]);

        let report = sync(&config, std::path::Path::new("/bin/topmatic"), &ctl);

        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("invalid OnCalendar"))
        );
        assert!(!ctl.calls().contains(&"enable:alpha".to_string()));
        assert!(ctl.calls().contains(&"disable:alpha".to_string()));
    }

    fn which_systemd_analyze() -> Option<std::path::PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("systemd-analyze"))
            .find(|candidate| candidate.is_file())
    }

    #[test]
    fn reset_removes_units_state_and_backs_up_config() {
        let tmp = tempfile::tempdir().unwrap();
        let unit_dir = tmp.path().join("units");
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));

        fs::create_dir_all(unit_dir.join("topmatic@alpha.timer.d")).unwrap();
        fs::write(unit_dir.join("topmatic@.service"), "unit").unwrap();
        fs::write(unit_dir.join("topmatic@.timer"), "unit").unwrap();
        fs::write(unit_dir.join("unrelated.timer"), "keep").unwrap();
        fs::create_dir_all(paths.logs_dir("alpha")).unwrap();
        fs::create_dir_all(&paths.config_dir).unwrap();
        fs::write(paths.config_file(), "# config").unwrap();

        let mut ctl = FakeCtl::new(unit_dir.clone());
        ctl.existing_instances = vec!["alpha".to_string()];

        let report = reset(&ctl, &paths, true);

        assert!(!unit_dir.join("topmatic@.service").exists());
        assert!(!unit_dir.join("topmatic@.timer").exists());
        assert!(!unit_dir.join("topmatic@alpha.timer.d").exists());
        assert!(unit_dir.join("unrelated.timer").exists());
        assert!(!paths.state_dir.exists());
        assert!(!paths.config_file().exists());
        assert_eq!(
            report.config_backup,
            Some(paths.config_file().with_extension("toml.bak"))
        );
        assert!(
            fs::read_to_string(paths.config_file().with_extension("toml.bak"))
                .unwrap()
                .contains("# config")
        );
        assert!(ctl.calls().contains(&"stop_all".to_string()));
        assert!(ctl.calls().contains(&"disable:alpha".to_string()));
        assert!(ctl.calls().contains(&"reload".to_string()));
    }

    #[test]
    fn reset_without_config_keeps_config_file() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        fs::create_dir_all(&paths.config_dir).unwrap();
        fs::write(paths.config_file(), "# config").unwrap();
        let ctl = FakeCtl::new(tmp.path().to_path_buf());

        let report = reset(&ctl, &paths, false);

        assert!(paths.config_file().exists());
        assert_eq!(report.config_backup, None);
    }
}
