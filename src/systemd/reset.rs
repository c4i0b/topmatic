use std::fs;

use crate::paths::Paths;

use super::SystemdCtl;
use super::units;

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
            let unique = unique_backup_path(&config_file);
            if fs::rename(&config_file, &unique).is_ok() {
                report.config_backup = Some(unique);
            }
        }
    }

    let _ = ctl.daemon_reload();
    report
}

fn unique_backup_path(config_file: &std::path::Path) -> std::path::PathBuf {
    crate::util::unique_sibling(config_file, "bak")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systemd::test_support::FakeCtl;

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
        let backup = report.config_backup.expect("backup path reported");
        assert!(fs::read_to_string(&backup).unwrap().contains("# config"));
        assert!(!paths.config_file().exists());
        assert!(!backup.starts_with(paths.config_file()));
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
