use crate::paths::Paths;

pub fn run() -> anyhow::Result<()> {
    let paths = Paths::from_env();
    if !paths.config_file().exists() {
        std::fs::create_dir_all(&paths.config_dir)?;
        std::fs::write(paths.config_file(), crate::config::template())?;
    }
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let mut split = editor.split_whitespace();
    let program = split.next().unwrap_or("vi");
    let mut command = std::process::Command::new(program);
    command.args(split).arg(paths.config_file());
    let status = command.status()?;
    if !status.success() {
        anyhow::bail!("editor {editor:?} exited with {status}");
    }
    super::sync::run()
}
