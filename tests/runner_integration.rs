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

const STUB_FLAKY_TOPGRADE: &str = r#"#!/bin/sh
dir="$(dirname "$0")"
count=$(( $(cat "$dir/count.txt" 2>/dev/null || echo 0) + 1 ))
echo "$count" > "$dir/count.txt"
pass_after="$(cat "$dir/pass_after.txt" 2>/dev/null || echo 1)"
if [ "$count" -ge "$pass_after" ]; then
  echo "stub topgrade recovered on attempt $count"
  exit 0
fi
echo "stub topgrade failed on attempt $count" >&2
exit 3
"#;

const STUB_NOTIFY_APPEND: &str = r#"#!/bin/sh
printf '%s|%s\n' "$1" "$2" >> "$(dirname "$0")/notify.txt"
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

    fn flaky_topgrade(&self, pass_after: u32) -> PathBuf {
        self.write_stub("topgrade", STUB_FLAKY_TOPGRADE);
        fs::write(self.bin_dir.join("pass_after.txt"), pass_after.to_string()).unwrap();
        self.bin_dir.join("topgrade")
    }

    fn notify_send_append(&self) -> PathBuf {
        self.write_stub("notify-send", STUB_NOTIFY_APPEND)
    }

    fn count(&self, name: &str) -> String {
        fs::read_to_string(self.bin_dir.join(name))
            .unwrap()
            .trim()
            .to_string()
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
            base: None,
            extra_steps: Vec::new(),
            excluded_steps: Vec::new(),
            schedule: Schedule::default(),
            notify,
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
    assert!(argv.contains("--only flatpak cargo"));
    assert!(!argv.contains("--yes"));
    assert!(!argv.contains("--cleanup"));
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
    assert!(content.contains("notify_end = \"never\""));
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

fn fast_policy(max_retries: u32) -> topmatic::runner::retry::RetryPolicy {
    topmatic::runner::retry::RetryPolicy {
        max_retries,
        base_delay: std::time::Duration::from_millis(5),
        delay_factor: 1,
        budget: std::time::Duration::from_secs(30),
        network_cap: std::time::Duration::from_millis(1),
        poll: std::time::Duration::from_millis(1),
    }
}

#[test]
fn scheduled_run_retries_until_the_stub_recovers() {
    let fixture = Fixture::new();
    let topgrade = fixture.flaky_topgrade(3);
    fixture.save_config(&[Fixture::profile("flaky", NotifyPolicy::OnFailure)]);
    let notify = fixture.notify_send_append();

    let outcome = runner::run_scheduled(
        &fixture.stored_profile("flaky"),
        &topgrade,
        &fixture.paths,
        &NotifySend::new(notify),
        false,
        &fast_policy(2),
    )
    .unwrap();

    assert!(outcome.success);
    assert_eq!(
        fixture.count("count.txt"),
        "3",
        "two failures then recovery"
    );
    assert!(
        fs::read_to_string(&outcome.log_path)
            .unwrap()
            .contains("recovered on attempt 3")
    );
    assert!(
        !fixture.bin_dir.join("notify.txt").exists(),
        "a recovered run must not notify"
    );
}

#[test]
fn scheduled_run_exhausts_retries_and_notifies_once() {
    let fixture = Fixture::new();
    let topgrade = fixture.flaky_topgrade(99);
    fixture.save_config(&[Fixture::profile("doomed", NotifyPolicy::Always)]);
    let notify = fixture.notify_send_append();

    let outcome = runner::run_scheduled(
        &fixture.stored_profile("doomed"),
        &topgrade,
        &fixture.paths,
        &NotifySend::new(notify),
        false,
        &fast_policy(3),
    )
    .unwrap();

    assert!(!outcome.success);
    assert_eq!(outcome.exit_code, Some(3));
    assert_eq!(
        fixture.count("count.txt"),
        "4",
        "initial attempt plus three retries"
    );
    let notified = fs::read_to_string(fixture.bin_dir.join("notify.txt")).unwrap();
    assert_eq!(
        notified.matches("doomed FAILED").count(),
        1,
        "notification must wait for the final attempt"
    );
    assert!(notified.contains("exit code: 3"));

    let status = runner::read_status(&fixture.paths, "doomed")
        .unwrap()
        .expect("status recorded");
    assert!(!status.success);
}

#[test]
fn plain_run_keeps_single_attempt_semantics() {
    let fixture = Fixture::new();
    let topgrade = fixture.flaky_topgrade(99);
    fixture.save_config(&[Fixture::profile("once", NotifyPolicy::Never)]);

    let outcome = runner::run(
        &fixture.stored_profile("once"),
        &topgrade,
        &fixture.paths,
        &NullNotify,
        false,
    )
    .unwrap();

    assert!(!outcome.success);
    assert_eq!(fixture.count("count.txt"), "1");
}
