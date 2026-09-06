use crate::domain::profile::NotifyPolicy;

pub trait NotifyBackend {
    fn send(&self, summary: &str, body: &str) -> std::io::Result<()>;
}

pub struct NotifySend {
    bin: std::path::PathBuf,
}

impl NotifySend {
    pub fn new(bin: std::path::PathBuf) -> Self {
        Self { bin }
    }
}

impl NotifyBackend for NotifySend {
    fn send(&self, summary: &str, body: &str) -> std::io::Result<()> {
        let status = std::process::Command::new(&self.bin)
            .arg(summary)
            .arg(body)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "notify-send exited with {status}"
            )))
        }
    }
}

pub struct NullNotify;

impl NotifyBackend for NullNotify {
    fn send(&self, _summary: &str, _body: &str) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn should_notify(policy: NotifyPolicy, success: bool) -> bool {
    match policy {
        NotifyPolicy::Always => true,
        NotifyPolicy::OnFailure => !success,
        NotifyPolicy::Never => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_gates_notifications_by_outcome() {
        assert!(should_notify(NotifyPolicy::Always, true));
        assert!(should_notify(NotifyPolicy::Always, false));
        assert!(should_notify(NotifyPolicy::OnFailure, false));
        assert!(!should_notify(NotifyPolicy::OnFailure, true));
        assert!(!should_notify(NotifyPolicy::Never, false));
    }
}
