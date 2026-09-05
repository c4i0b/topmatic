use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use crate::paths::Paths;

pub struct LogsState {
    pub profile: String,
    pub entries: Vec<PathBuf>,
    pub selected: usize,
    pub content: Option<String>,
    pub scroll: u16,
}

impl LogsState {
    pub fn open(paths: &Paths, profile: &str) -> Self {
        let mut state = Self {
            profile: profile.to_string(),
            entries: Vec::new(),
            selected: 0,
            content: None,
            scroll: 0,
        };
        state.reload(paths);
        state.load_selected();
        state
    }

    pub fn reload(&mut self, paths: &Paths) {
        self.entries = list_runs(paths, &self.profile);
        self.selected = 0;
        self.load_selected();
    }

    pub fn load_selected(&mut self) {
        self.content = self
            .entries
            .get(self.selected)
            .and_then(|path| std::fs::read_to_string(path).ok());
        self.scroll = 0;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.content.is_some() {
                    self.scroll = self.scroll.saturating_sub(1);
                } else {
                    self.selected = self.selected.saturating_sub(1);
                    self.load_selected();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.content.is_some() {
                    self.scroll = self.scroll.saturating_add(1);
                } else if self.selected + 1 < self.entries.len() {
                    self.selected += 1;
                    self.load_selected();
                }
            }
            KeyCode::Enter => {
                if self.content.is_none() && !self.entries.is_empty() {
                    self.load_selected();
                }
            }
            KeyCode::Backspace | KeyCode::Char('h') => {
                self.content = None;
            }
            KeyCode::Char('r') => {
                let paths = Paths::from_env();
                self.reload(&paths);
            }
            _ => {}
        }
        false
    }
}

pub fn list_runs(paths: &Paths, profile: &str) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(paths.logs_dir(profile)) {
        Ok(read_dir) => read_dir.flatten().map(|entry| entry.path()).collect(),
        Err(_) => Vec::new(),
    };
    entries.retain(|path| path.extension().is_some_and(|ext| ext == "log"));
    entries.sort();
    entries.reverse();
    entries
}

pub fn tail(paths: &Paths, profile: &str, lines: usize) -> Option<String> {
    let newest = list_runs(paths, profile).into_iter().next()?;
    let content = std::fs::read_to_string(newest).ok()?;
    let mut tail: Vec<&str> = content.lines().collect();
    if tail.len() > lines {
        tail = tail.split_off(tail.len() - lines);
    }
    Some(tail.join("\n"))
}

pub fn render(state: &LogsState, frame: &mut Frame, area: Rect) {
    if let Some(content) = &state.content {
        let paragraph = Paragraph::new(content.clone())
            .scroll((state.scroll, 0))
            .block(
                Block::bordered()
                    .title(format!(
                        "{} — {} (Esc/back to list, j/k scroll)",
                        state.profile,
                        state
                            .entries
                            .get(state.selected)
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ))
                    .style(Style::new().fg(Color::Cyan)),
            );
        frame.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = state
        .entries
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let name = path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let line = if index == state.selected {
                Line::styled(
                    format!("▶ {name}"),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )
            } else {
                Line::from(format!("  {name}"))
            };
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::bordered().title(format!(
            "runs of {} (Enter: open, r: refresh, Esc: back)",
            state.profile
        )))
        .style(Style::new().fg(Color::Cyan));
    frame.render_widget(list, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn lists_runs_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        let dir = paths.logs_dir("alpha");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("20260101-010101.log"), "old").unwrap();
        std::fs::write(dir.join("20260202-020202.log"), "new").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignore").unwrap();

        let runs = list_runs(&paths, "alpha");
        assert_eq!(runs.len(), 2);
        assert_eq!(
            runs[0].file_name().unwrap().to_string_lossy(),
            "20260202-020202.log"
        );
    }

    #[test]
    fn missing_logs_dir_yields_empty_list() {
        let paths = Paths::with_bases(PathBuf::from("/nope/c"), PathBuf::from("/nope/s"));
        assert!(list_runs(&paths, "ghost").is_empty());
    }

    #[test]
    fn tail_returns_last_lines_of_newest_run() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        let dir = paths.logs_dir("alpha");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("20260101-010101.log"), "old1\nold2\n").unwrap();
        let newest = "l1\nl2\nl3\nl4\n".to_string();
        std::fs::write(dir.join("20260202-020202.log"), &newest).unwrap();

        assert_eq!(tail(&paths, "alpha", 2).unwrap(), "l3\nl4");
        assert_eq!(tail(&paths, "alpha", 10).unwrap(), "l1\nl2\nl3\nl4");
        assert!(tail(&paths, "ghost", 5).is_none());
    }
}
