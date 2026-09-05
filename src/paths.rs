use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let config_base = xdg_dir("XDG_CONFIG_HOME", &home, ".config");
        let state_base = xdg_dir("XDG_STATE_HOME", &home, ".local/state");
        Self::with_bases(config_base, state_base)
    }

    pub fn with_bases(config_base: PathBuf, state_base: PathBuf) -> Self {
        Self {
            config_dir: config_base.join("topmatic"),
            state_dir: state_base.join("topmatic"),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn topgrade_config_file(&self) -> PathBuf {
        self.config_dir.join("topgrade.toml")
    }

    pub fn topgrade_config_file_for(&self, profile: &crate::domain::profile::Profile) -> PathBuf {
        if profile.repos.is_empty() {
            self.topgrade_config_file()
        } else {
            self.config_dir
                .join(format!("topgrade-{}.toml", profile.name))
        }
    }

    pub fn logs_dir(&self, profile: &str) -> PathBuf {
        self.state_dir.join("logs").join(profile)
    }

    pub fn status_file(&self, profile: &str) -> PathBuf {
        self.state_dir
            .join("status")
            .join(format!("{profile}.json"))
    }

    pub fn lock_file(&self, profile: &str) -> PathBuf {
        self.state_dir.join(format!("{profile}.lock"))
    }
}

fn xdg_dir(var: &str, home: &Path, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_topmatic_under_given_bases() {
        let paths = Paths::with_bases(PathBuf::from("/xdg-config"), PathBuf::from("/xdg-state"));
        assert_eq!(paths.config_dir, PathBuf::from("/xdg-config/topmatic"));
        assert_eq!(paths.state_dir, PathBuf::from("/xdg-state/topmatic"));
    }

    #[test]
    fn derives_profile_specific_paths() {
        let paths = Paths {
            config_dir: PathBuf::from("/cfg"),
            state_dir: PathBuf::from("/state"),
        };
        assert_eq!(paths.config_file(), PathBuf::from("/cfg/config.toml"));
        assert_eq!(
            paths.topgrade_config_file(),
            PathBuf::from("/cfg/topgrade.toml")
        );
        assert_eq!(
            paths.logs_dir("flatpak-daily"),
            PathBuf::from("/state/logs/flatpak-daily")
        );
        assert_eq!(
            paths.status_file("flatpak-daily"),
            PathBuf::from("/state/status/flatpak-daily.json")
        );
        assert_eq!(
            paths.lock_file("flatpak-daily"),
            PathBuf::from("/state/flatpak-daily.lock")
        );
    }
}
