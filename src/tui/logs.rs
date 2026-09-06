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
    pub content_lines: usize,
    pub viewport: std::cell::Cell<usize>,
    pub scroll: u16,
    pub follow_offset: u16,
    pub follow: bool,
    pub follow_header: String,
}

impl LogsState {
    pub fn open(paths: &Paths, profile: &str) -> Self {
        let mut state = Self::empty(profile);
        state.reload(paths);
        state.load_selected();
        state
    }

    pub fn follow(profile: &str) -> Self {
        Self {
            follow: true,
            follow_header: format!("starting {profile}…"),
            ..Self::empty(profile)
        }
    }

    fn empty(profile: &str) -> Self {
        Self {
            profile: profile.to_string(),
            entries: Vec::new(),
            selected: 0,
            content: None,
            content_lines: 0,
            viewport: std::cell::Cell::new(0),
            scroll: 0,
            follow_offset: 0,
            follow: false,
            follow_header: String::new(),
        }
    }

    pub fn set_content(&mut self, content: Option<String>) {
        self.content_lines = content
            .as_ref()
            .map_or(0, |text| sanitize_log_text(text).lines().count());
        self.content = content;
    }

    fn page(&self) -> u16 {
        self.viewport.get().max(1).saturating_sub(1) as u16
    }

    pub fn reload(&mut self, paths: &Paths) {
        self.entries = list_runs(paths, &self.profile);
        self.selected = 0;
        self.load_selected();
    }

    pub fn load_selected(&mut self) {
        let raw = self
            .entries
            .get(self.selected)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| sanitize_log_text(&text));
        self.set_content(raw);
        self.scroll = 0;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.follow {
            let page = self.page();
            return match key.code {
                KeyCode::Esc | KeyCode::Char('h' | 'H') => true,
                KeyCode::Up | KeyCode::Char('k' | 'K') => {
                    self.follow_offset = self.follow_offset.saturating_add(1);
                    false
                }
                KeyCode::Down | KeyCode::Char('j' | 'J') => {
                    self.follow_offset = self.follow_offset.saturating_sub(1);
                    false
                }
                KeyCode::PageUp => {
                    self.follow_offset = self.follow_offset.saturating_add(page);
                    false
                }
                KeyCode::PageDown => {
                    self.follow_offset = self.follow_offset.saturating_sub(page);
                    false
                }
                KeyCode::Home => {
                    self.follow_offset = self.content_lines.saturating_sub(1) as u16;
                    false
                }
                KeyCode::End => {
                    self.follow_offset = 0;
                    false
                }
                _ => false,
            };
        }
        let page = self.page() as usize;
        match key.code {
            KeyCode::Esc | KeyCode::Char('h' | 'H') => return true,
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                if self.content.is_some() {
                    self.scroll = self.scroll.saturating_sub(1);
                } else {
                    self.selected = self.selected.saturating_sub(1);
                    self.load_selected();
                }
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                if self.content.is_some() {
                    self.scroll = self.scroll.saturating_add(1);
                } else if self.selected + 1 < self.entries.len() {
                    self.selected += 1;
                    self.load_selected();
                }
            }
            KeyCode::PageUp => {
                if self.content.is_some() {
                    self.scroll = self.scroll.saturating_sub(page as u16);
                } else {
                    self.selected = self.selected.saturating_sub(page);
                    self.load_selected();
                }
            }
            KeyCode::PageDown => {
                if self.content.is_some() {
                    let max = self.content_lines.saturating_sub(1) as u16;
                    self.scroll = self.scroll.saturating_add(page as u16).min(max);
                } else if self.selected + 1 < self.entries.len() {
                    self.selected = (self.selected + page).min(self.entries.len() - 1);
                    self.load_selected();
                }
            }
            KeyCode::Home => {
                if self.content.is_some() {
                    self.scroll = 0;
                } else {
                    self.selected = 0;
                    self.load_selected();
                }
            }
            KeyCode::End => {
                if self.content.is_some() {
                    self.scroll = self.content_lines.saturating_sub(1) as u16;
                } else if !self.entries.is_empty() {
                    self.selected = self.entries.len() - 1;
                    self.load_selected();
                }
            }
            KeyCode::Enter => {
                if self.content.is_none() && !self.entries.is_empty() {
                    self.load_selected();
                }
            }
            KeyCode::Backspace => {
                self.content = None;
            }
            KeyCode::Char('r' | 'R') => {
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

pub fn sanitize_log_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    for n in chars.by_ref() {
                        if n == '\x07' {
                            break;
                        }
                        if n == '\x1b' {
                            let _ = chars.next();
                            break;
                        }
                    }
                }
                Some(_) => {}
                None => {}
            },
            '\n' => out.push('\n'),
            '\t' => out.push_str("    "),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {}
            c => out.push(c),
        }
    }
    out
}

pub fn tail(paths: &Paths, profile: &str, lines: usize) -> Option<String> {
    let newest = list_runs(paths, profile).into_iter().next()?;
    let content = std::fs::read_to_string(newest).ok()?;
    let sanitized = sanitize_log_text(&content);
    let mut tail: Vec<&str> = sanitized.lines().collect();
    if tail.len() > lines {
        tail = tail.split_off(tail.len() - lines);
    }
    Some(tail.join("\n"))
}

pub fn render(state: &LogsState, frame: &mut Frame, area: Rect) {
    state.viewport.set(area.height.saturating_sub(2) as usize);
    if state.follow {
        let visible = state.viewport.get();
        let total = state.content_lines;
        let from_top = total
            .saturating_sub(visible)
            .saturating_sub(state.follow_offset as usize) as u16;
        let paragraph = Paragraph::new(state.content.clone().unwrap_or_default())
            .scroll((from_top, 0))
            .block(
                Block::bordered()
                    .title(state.follow_header.clone())
                    .style(Style::new().fg(Color::Cyan)),
            );
        frame.render_widget(paragraph, area);
        return;
    }
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
        .block(Block::bordered().title(format!("runs of {}", state.profile)))
        .style(Style::new().fg(Color::Cyan));
    frame.render_widget(list, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::path::PathBuf;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

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

    #[test]
    fn follow_scrolling_counts_from_the_bottom_and_repins() {
        let mut state = LogsState::follow("alpha");
        assert_eq!(state.follow_offset, 0, "starts pinned to the newest line");
        assert!(!state.handle_key(key(KeyCode::Up)));
        assert_eq!(state.follow_offset, 1);
        assert!(!state.handle_key(key(KeyCode::Char('k'))));
        assert_eq!(state.follow_offset, 2);
        assert!(!state.handle_key(key(KeyCode::Down)));
        assert_eq!(state.follow_offset, 1);
        assert!(!state.handle_key(key(KeyCode::Char('j'))));
        assert_eq!(
            state.follow_offset, 0,
            "reaching the bottom re-engages auto-scroll"
        );
        assert!(!state.handle_key(key(KeyCode::Down)));
        assert_eq!(state.follow_offset, 0, "down at the bottom stays pinned");
        assert!(state.handle_key(key(KeyCode::Esc)), "esc still leaves");
    }

    #[test]
    fn sanitizer_strips_real_ansi_noise_from_run_logs() {
        let raw = "Updating Proton-EM...\r\r\x1b[2K\r\x1b[32mAlready up to date: Proton-EM";
        assert_eq!(
            sanitize_log_text(raw),
            "Updating Proton-EM...Already up to date: Proton-EM",
            "CSI erase/color sequences and carriage returns vanish (fixture from the 20260906-233646 host log)"
        );
        assert_eq!(
            sanitize_log_text("\x1b[1mbold\x1b[m and \x1b[4munder\x1b[0m\ttab"),
            "bold and under    tab",
            "SGR sequences drop and tabs expand"
        );
        assert_eq!(
            sanitize_log_text("keep unicode \u{2015} and \u{00a7} intact\n"),
            "keep unicode \u{2015} and \u{00a7} intact\n",
            "printable unicode survives, newlines survive"
        );
        assert_eq!(sanitize_log_text("\x08\x7fdrop"), "drop");
    }

    #[test]
    fn follow_page_and_home_end_keys() {
        let mut state = LogsState::follow("alpha");
        state.set_content(Some(
            (1..=100)
                .map(|i| format!("l{i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
        state.viewport.set(11);

        assert!(!state.handle_key(key(KeyCode::PageUp)));
        assert_eq!(state.follow_offset, 10);
        assert!(!state.handle_key(key(KeyCode::PageDown)));
        assert_eq!(state.follow_offset, 0, "page down re-pins at the bottom");

        assert!(!state.handle_key(key(KeyCode::Home)));
        assert_eq!(state.follow_offset, 99, "home jumps to the very top");
        assert!(!state.handle_key(key(KeyCode::End)));
        assert_eq!(state.follow_offset, 0, "end returns to the pinned bottom");
    }

    #[test]
    fn browse_page_and_home_end_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        let dir = paths.logs_dir("alpha");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("20260101-010101.log"),
            (1..=50)
                .map(|i| format!("l{i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        let mut state = LogsState::open(&paths, "alpha");
        state.viewport.set(11);

        state.handle_key(key(KeyCode::End));
        assert_eq!(state.scroll, 49, "end jumps to the last line");
        state.handle_key(key(KeyCode::Home));
        assert_eq!(state.scroll, 0);
        state.handle_key(key(KeyCode::PageDown));
        assert_eq!(state.scroll, 10);
        state.handle_key(key(KeyCode::PageUp));
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn follow_render_pins_the_newest_line_and_scrolls_on_demand() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut state = LogsState::follow("alpha");
        state.set_content(Some(
            (1..=40)
                .map(|i| format!("line-{i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ));

        let mut terminal = Terminal::new(TestBackend::new(60, 10)).unwrap();
        terminal
            .draw(|frame| render(&state, frame, frame.area()))
            .unwrap();
        let pinned = terminal.backend().to_string();
        assert!(
            pinned.contains("line-40"),
            "pinned at the bottom shows the newest line:\n{pinned}"
        );
        assert!(!pinned.contains("line-1"));

        state.handle_key(key(KeyCode::Up));
        state.handle_key(key(KeyCode::Up));
        terminal
            .draw(|frame| render(&state, frame, frame.area()))
            .unwrap();
        let scrolled = terminal.backend().to_string();
        assert!(
            scrolled.contains("line-38"),
            "scrolled back two lines shows the older window:\n{scrolled}"
        );
        assert!(!scrolled.contains("line-40"));
    }
}
