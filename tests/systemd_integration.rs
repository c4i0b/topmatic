use std::path::PathBuf;
use std::process::Command;

use topmatic::systemd::parse_systemd_timestamp;

const TIMER_NAME: &str = "topmatic-itest.timer";

fn systemd_user_available() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-system-running"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl")
        .args(["--user"])
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

struct TransientTimer {
    path: PathBuf,
}

impl TransientTimer {
    fn install() -> Option<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        let unit_dir = home.join(".config/systemd/user");
        std::fs::create_dir_all(&unit_dir).ok()?;
        let path = unit_dir.join(TIMER_NAME);
        std::fs::write(
            &path,
            "[Unit]\nDescription=topmatic integration test\n\n[Timer]\nOnCalendar=daily\n\n[Install]\nWantedBy=timers.target\n",
        )
        .ok()?;
        if !systemctl(&["daemon-reload"]) || !systemctl(&["start", TIMER_NAME]) {
            let _ = std::fs::remove_file(&path);
            let _ = systemctl(&["daemon-reload"]);
            return None;
        }
        Some(Self { path })
    }
}

impl Drop for TransientTimer {
    fn drop(&mut self) {
        let _ = systemctl(&["stop", TIMER_NAME]);
        let _ = std::fs::remove_file(&self.path);
        let _ = systemctl(&["daemon-reload"]);
    }
}

#[test]
fn parser_accepts_real_systemctl_next_elapse_output() {
    if !systemd_user_available() {
        return;
    }
    let Some(_timer) = TransientTimer::install() else {
        return;
    };

    let output = Command::new("systemctl")
        .args([
            "--user",
            "show",
            TIMER_NAME,
            "-p",
            "NextElapseUSecRealtime",
            "--value",
        ])
        .output()
        .expect("systemctl show");
    assert!(output.status.success());

    let raw = String::from_utf8_lossy(&output.stdout);
    assert!(
        raw.contains("20"),
        "unexpected NextElapseUSecRealtime: {raw:?}"
    );

    let parsed = parse_systemd_timestamp(&raw)
        .unwrap_or_else(|| panic!("real systemctl output must parse: {raw:?}"));
    assert!(parsed.timestamp() > 0);
}
