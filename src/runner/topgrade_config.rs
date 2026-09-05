use crate::domain::profile::Profile;

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_string()).to_string()
}

pub fn render() -> String {
    [
        "# Managed by topmatic. This file isolates topmatic from your own topgrade",
        "# configuration; manual edits will be overwritten.",
        "skip_notify = true",
        "no_retry = true",
        "no_self_update = true",
    ]
    .join("\n")
        + "\n"
}

pub fn render_for(profile: &Profile) -> String {
    let mut content = render();
    if !profile.repos.is_empty() {
        content.push_str("\n[git]\nrepos = [\n");
        for repo in &profile.repos {
            content.push_str(&format!("    {},\n", toml_string(&repo.path)));
        }
        content.push_str("]\n");
        let with_apply: Vec<&crate::domain::repos::RepoEntry> = profile
            .repos
            .iter()
            .filter(|repo| repo.apply.is_some())
            .collect();
        if !with_apply.is_empty() {
            content.push_str("\n[commands]\n");
            for repo in with_apply {
                let name = std::path::Path::new(&repo.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "repo".to_string());
                let command = format!(
                    "cd {} && {}",
                    repo.path,
                    repo.apply.clone().unwrap_or_default()
                );
                content.push_str(&format!(
                    "{} = {}\n",
                    toml_string(&format!("apply-{name}")),
                    toml_string(&command)
                ));
            }
        }
    }
    content
}

pub fn write_if_changed(path: &std::path::Path) -> std::io::Result<()> {
    crate::util::write_file_if_changed(path, &render()).map(|_| ())
}

pub fn write_for(profile: &Profile, path: &std::path::Path) -> std::io::Result<()> {
    crate::util::write_file_if_changed(path, &render_for(profile)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unattended_defaults() {
        let text = render();
        assert!(text.contains("skip_notify = true"));
        assert!(text.contains("no_retry = true"));
        assert!(text.contains("no_self_update = true"));
    }

    #[test]
    fn write_if_changed_creates_and_preserves() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("topgrade.toml");
        write_if_changed(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), render());
        let first_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        write_if_changed(&path).unwrap();
        let second_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(first_mtime, second_mtime);
    }

    fn profile_with_repos() -> crate::domain::profile::Profile {
        let mut profile = crate::domain::profile::Profile {
            name: "dev-daily".to_string(),
            steps: vec!["git_repos".to_string()],
            repos: Vec::new(),
            schedule: crate::domain::schedule::Schedule::default(),
            cleanup: true,
            notify: Default::default(),
            enabled: true,
            scope: Default::default(),
        };
        profile
            .repos
            .push(crate::domain::repos::RepoEntry::pull_only("~/dev/tool"));
        profile
            .repos
            .push(crate::domain::repos::RepoEntry::with_apply(
                "~/dot",
                "stow bash",
            ));
        profile
    }

    #[test]
    fn renders_git_repos_and_apply_commands_per_profile() {
        let content = render_for(&profile_with_repos());
        let parsed: toml::Value = toml::from_str(&content).unwrap_or_else(|error| {
            panic!("rendered config is not valid TOML: {error}\n{content}")
        });
        let repos = parsed
            .get("git")
            .and_then(|git| git.get("repos"))
            .and_then(|repos| repos.as_array())
            .expect("[git] repos array");
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].as_str(), Some("~/dev/tool"));
        assert_eq!(repos[1].as_str(), Some("~/dot"));
        let commands = parsed
            .get("commands")
            .and_then(|commands| commands.as_table())
            .expect("[commands] table");
        assert_eq!(
            commands.get("apply-dot").and_then(|cmd| cmd.as_str()),
            Some("cd ~/dot && stow bash")
        );
        assert_eq!(commands.len(), 1);
    }

    #[test]
    fn render_without_repos_matches_base() {
        let mut profile = profile_with_repos();
        profile.repos.clear();
        assert_eq!(render_for(&profile), render());
        toml::from_str::<toml::Value>(&render_for(&profile)).unwrap();
    }

    #[test]
    fn rendered_base_config_is_valid_toml() {
        let parsed: toml::Value =
            toml::from_str(&render()).expect("base config must be valid TOML");
        assert_eq!(
            parsed.get("skip_notify").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(parsed.get("no_retry").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            parsed.get("no_self_update").and_then(|v| v.as_bool()),
            Some(true)
        );
    }
}
