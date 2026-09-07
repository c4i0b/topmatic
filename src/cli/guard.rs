use anyhow::Result;

pub(super) fn uid_line_means_root(line: &str) -> bool {
    line.strip_prefix("Uid:")
        .and_then(|fields| fields.split_whitespace().nth(1))
        .is_some_and(|euid| euid == "0")
}

pub(super) fn effective_uid_is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .is_some_and(|status| status.lines().any(uid_line_means_root))
}

pub fn block_root() -> Result<()> {
    if effective_uid_is_root() {
        anyhow::bail!(
            "topmatic must not be run as root. It manages your user's systemd session, so running it with sudo would target root's units instead. Re-run as your regular user."
        );
    }
    Ok(())
}
