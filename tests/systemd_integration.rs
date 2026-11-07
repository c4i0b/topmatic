use std::process::Command;

use topmatic::systemd::parse_systemd_timestamp;

const TIMER_NAME: &str = "topmatic-itest.timer";

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl")
        .args(["--user"])
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

struct TransientTimer {}

impl TransientTimer {
    fn install() -> Option<Self> {
        Command::new("systemd-run")
            .args([
                "--user",
                "--unit=topmatic-itest",
                "--on-calendar=daily",
                "/bin/true",
            ])
            .status()
            .ok()?
            .success()
            .then_some(Self {})
    }
}

impl Drop for TransientTimer {
    fn drop(&mut self) {
        let _ = systemctl(&["stop", TIMER_NAME]);
        let _ = systemctl(&["reset-failed", "topmatic-itest.service"]);
    }
}

#[test]
#[ignore = "live: needs a real user systemd session (just test-live)"]
fn parser_accepts_real_systemctl_next_elapse_output() {
    let Some(_timer) = TransientTimer::install() else {
        panic!("user systemd is running but the transient timer could not be installed");
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
