use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use topmatic::config;
use topmatic::domain::profile::{NotifyPolicy, Profile};
use topmatic::domain::schedule::Schedule;
use topmatic::paths::Paths;
use topmatic::runner;
use topmatic::runner::notify::{NotifySend, NullNotify};

const STUB_TOPGRADE: &str = r#"#!/bin/sh
printf '%s\n' "$*" > "$(dirname "$0")/argv.txt"
echo "stub topgrade stdout"
echo "stub topgrade stderr" >&2
exit "$(cat "$(dirname "$0")/exit.txt" 2>/dev/null || echo 0)"
"#;

const STUB_NOTIFY: &str = r#"#!/bin/sh
printf '%s|%s\n' "$1" "$2" > "$(dirname "$0")/notify.txt"
"#;

struct Fixture {
    _tmp: TempDir,
    paths: Paths,
    bin_dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        Self {
            _tmp: tmp,
            paths,
            bin_dir,
        }
    }

    fn write_stub(&self, name: &str, body: &str) -> PathBuf {
        let path = self.bin_dir.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn topgrade(&self) -> PathBuf {
        self.write_stub("topgrade", STUB_TOPGRADE)
    }

    fn notify_send(&self) -> PathBuf {
        self.write_stub("notify-send", STUB_NOTIFY)
    }

    fn set_exit_code(&self, code: &str) {
        fs::write(self.bin_dir.join("exit.txt"), code).unwrap();
    }

    fn argv(&self) -> String {
        fs::read_to_string(self.bin_dir.join("argv.txt")).unwrap()
    }

    fn profile(name: &str, notify: NotifyPolicy) -> Profile {
        Profile {
            name: name.to_string(),
            steps: vec!["flatpak".to_string(), "cargo".to_string()],
            schedule: Schedule::default(),
            cleanup: true,
            notify,
            enabled: true,
            scope: Default::default(),
        }
    }

    fn save_config(&self, profiles: &[Profile]) {
        let mut app = config::AppConfig::default();
        for profile in profiles {
            app.upsert(profile.clone());
        }
        config::save(&self.paths, &app).unwrap();
    }

    fn stored_profile(&self, name: &str) -> Profile {
        config::load(&self.paths)
            .unwrap()
            .profile(name)
            .unwrap()
            .clone()
    }
}

fn assert_log_contains(path: &Path, needle: &str) {
    let content = fs::read_to_string(path).unwrap();
    assert!(
        content.contains(needle),
        "log missing {needle:?}:\n{content}"
    );
}

#[test]
fn successful_run_records_argv_logs_and_status() {
    let fixture = Fixture::new();
    fixture.save_config(&[Fixture::profile("flatpak-daily", NotifyPolicy::Never)]);
    let topgrade = fixture.topgrade();

    let outcome = runner::run(
        &fixture.stored_profile("flatpak-daily"),
        &topgrade,
        &fixture.paths,
        &NullNotify,
        false,
    )
    .unwrap();

    assert!(outcome.success);
    assert_eq!(outcome.exit_code, Some(0));
    assert!(!outcome.skipped);

    let argv = fixture.argv();
    assert!(argv.contains("--no-ask-retry"));
    assert!(argv.contains("--cleanup"));
    assert!(argv.contains("--only flatpak cargo"));
    assert!(argv.contains("--yes"));
    assert!(argv.contains(&format!(
        "--config {}",
        fixture.paths.topgrade_config_file().display()
    )));

    assert_log_contains(&outcome.log_path, "stub topgrade stdout");
    assert_log_contains(&outcome.log_path, "stub topgrade stderr");

    let status = runner::read_status(&fixture.paths, "flatpak-daily")
        .unwrap()
        .expect("status written");
    assert_eq!(status.profile, "flatpak-daily");
    assert!(status.success);
}

#[test]
fn topgrade_config_file_is_written_and_isolated() {
    let fixture = Fixture::new();
    fixture.save_config(&[Fixture::profile("flatpak-daily", NotifyPolicy::Never)]);
    let topgrade = fixture.topgrade();

    runner::run(
        &fixture.stored_profile("flatpak-daily"),
        &topgrade,
        &fixture.paths,
        &NullNotify,
        false,
    )
    .unwrap();

    let content = fs::read_to_string(fixture.paths.topgrade_config_file()).unwrap();
    assert!(content.contains("skip_notify = true"));
    assert!(content.contains("no_self_update = true"));
}

#[test]
fn dry_run_flag_is_forwarded() {
    let fixture = Fixture::new();
    fixture.save_config(&[Fixture::profile("flatpak-daily", NotifyPolicy::Never)]);
    let topgrade = fixture.topgrade();

    runner::run(
        &fixture.stored_profile("flatpak-daily"),
        &topgrade,
        &fixture.paths,
        &NullNotify,
        true,
    )
    .unwrap();

    assert!(fixture.argv().contains("--dry-run"));
}

#[test]
fn failing_run_notifies_according_to_policy() {
    let fixture = Fixture::new();
    fixture.save_config(&[
        Fixture::profile("failing", NotifyPolicy::OnFailure),
        Fixture::profile("silent", NotifyPolicy::Never),
    ]);
    let topgrade = fixture.topgrade();
    let notify = NotifySend::new(fixture.notify_send());
    fixture.set_exit_code("3");

    let outcome = runner::run(
        &fixture.stored_profile("failing"),
        &topgrade,
        &fixture.paths,
        &notify,
        false,
    )
    .unwrap();
    assert!(!outcome.success);
    assert_eq!(outcome.exit_code, Some(3));

    let record = fs::read_to_string(fixture.bin_dir.join("notify.txt")).unwrap();
    assert!(record.starts_with("topmatic: failing FAILED"));

    runner::run(
        &fixture.stored_profile("silent"),
        &topgrade,
        &fixture.paths,
        &notify,
        false,
    )
    .unwrap();
    assert!(
        !fixture.bin_dir.join("notify.txt").exists() || {
            let record = fs::read_to_string(fixture.bin_dir.join("notify.txt")).unwrap();
            !record.contains("silent")
        }
    );
}

#[test]
fn run_is_skipped_when_lock_is_already_held() {
    let fixture = Fixture::new();
    fixture.save_config(&[Fixture::profile("flatpak-daily", NotifyPolicy::Never)]);
    let topgrade = fixture.topgrade();

    let lock_path = fixture.paths.lock_file("flatpak-daily");
    fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .unwrap();
    let mut lock = fd_lock::RwLock::new(file);
    let _guard = lock.write().unwrap();

    let outcome = runner::run(
        &fixture.stored_profile("flatpak-daily"),
        &topgrade,
        &fixture.paths,
        &NullNotify,
        false,
    )
    .unwrap();

    assert!(outcome.skipped);
    assert!(!fixture.bin_dir.join("argv.txt").exists());
    let status = runner::read_status(&fixture.paths, "flatpak-daily")
        .unwrap()
        .expect("skip is recorded");
    assert!(status.skipped);
}
