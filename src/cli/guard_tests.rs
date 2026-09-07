use super::guard::{effective_uid_is_root, uid_line_means_root};

#[test]
fn uid_line_means_root_uses_only_the_effective_uid() {
    assert!(uid_line_means_root("Uid:\t0\t0\t0\t0"));
    assert!(
        uid_line_means_root("Uid:\t1000\t0\t0\t0"),
        "setuid-style euid zero is detected"
    );
    assert!(
        !uid_line_means_root("Uid:\t0\t1000\t1000\t1000"),
        "real uid zero with a user euid does not block"
    );
    assert!(!uid_line_means_root("Uid:\t1000\t1000\t1000\t1000"));
    assert!(
        !uid_line_means_root("Name:\ttopmatic"),
        "unrelated lines are ignored"
    );
}

#[test]
fn guard_matches_the_running_process_effective_uid() {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let host_line = status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .unwrap();
    let proc_euid = host_line.split_whitespace().nth(1).unwrap();
    let id = std::process::Command::new("id").arg("-u").output().unwrap();
    assert!(id.status.success());
    let id_euid = String::from_utf8_lossy(&id.stdout).trim().to_string();
    assert_eq!(
        proc_euid, id_euid,
        "fixture sanity on {host_line:?} vs `id -u` {id_euid}"
    );
    assert_eq!(effective_uid_is_root(), id_euid == "0");
}
