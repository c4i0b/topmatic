use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;

use chrono::{DateTime, Local, TimeZone};

pub const DEFAULT_CAPACITY: usize = 300;
const MAX_LOG_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityKind {
    Action,
    Command,
    Error,
}

impl ActivityKind {
    fn marker(self) -> char {
        match self {
            ActivityKind::Action => 'A',
            ActivityKind::Command => 'C',
            ActivityKind::Error => 'E',
        }
    }

    fn from_marker(marker: char) -> Option<Self> {
        match marker {
            'A' => Some(ActivityKind::Action),
            'C' => Some(ActivityKind::Command),
            'E' => Some(ActivityKind::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityEntry {
    pub at: DateTime<Local>,
    pub kind: ActivityKind,
    pub text: String,
}

pub struct ActivityLog {
    entries: VecDeque<ActivityEntry>,
    capacity: usize,
    file: Option<PathBuf>,
    max_bytes: u64,
    written: u64,
}

impl ActivityLog {
    pub fn in_memory(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
            file: None,
            max_bytes: MAX_LOG_BYTES,
            written: 0,
        }
    }

    pub fn with_file(capacity: usize, file: PathBuf) -> Self {
        Self::with_file_bounded(capacity, file, MAX_LOG_BYTES)
    }

    pub fn with_file_bounded(capacity: usize, file: PathBuf, max_bytes: u64) -> Self {
        let mut log = Self {
            entries: VecDeque::new(),
            capacity,
            file: Some(file),
            max_bytes,
            written: 0,
        };
        log.load_history();
        log
    }

    pub fn log(&mut self, kind: ActivityKind, text: impl Into<String>) {
        if self.capacity == 0 {
            return;
        }
        self.entries.push_back(ActivityEntry {
            at: Local::now(),
            kind,
            text: text.into(),
        });
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
        self.persist_newest();
    }

    pub fn entries(&self) -> &VecDeque<ActivityEntry> {
        &self.entries
    }

    pub fn tail(&self) -> Option<&ActivityEntry> {
        self.entries.back()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn persist_newest(&mut self) {
        let Some(file) = &self.file else {
            return;
        };
        let Some(entry) = self.entries.back() else {
            return;
        };
        let Ok(line) = format_line(entry) else {
            return;
        };
        let Ok(mut handle) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(file)
        else {
            self.file = None;
            return;
        };
        self.written = self.written.saturating_add(line.len() as u64);
        if handle.write_all(line.as_bytes()).is_err() || handle.flush().is_err() {
            self.file = None;
            return;
        }
        if self.written > self.max_bytes {
            self.rewrite_from_memory();
        }
    }

    fn rewrite_from_memory(&mut self) {
        let Some(file) = &self.file else {
            return;
        };
        let lines: Vec<String> = self
            .entries
            .iter()
            .filter_map(|entry| format_line(entry).ok())
            .collect();
        let mut kept = 0usize;
        let mut bytes = 0u64;
        for line in lines.iter().rev() {
            bytes = bytes.saturating_add(line.len() as u64);
            if bytes > self.max_bytes && kept > 0 {
                bytes = bytes.saturating_sub(line.len() as u64);
                break;
            }
            kept += 1;
        }
        let drop = self.entries.len().saturating_sub(kept);
        for _ in 0..drop {
            self.entries.pop_front();
        }
        let kept_lines: Vec<&String> = lines[lines.len().saturating_sub(kept)..].iter().collect();
        let Ok(mut handle) = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(file)
        else {
            self.file = None;
            return;
        };
        if kept_lines
            .iter()
            .try_for_each(|line| handle.write_all(line.as_bytes()))
            .and_then(|_| handle.flush())
            .is_err()
        {
            self.file = None;
            return;
        }
        self.written = bytes;
    }

    fn load_history(&mut self) {
        let Some(file) = self.file.clone() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&file) else {
            return;
        };
        let mut loaded: Vec<ActivityEntry> = text.lines().filter_map(parse_line).collect();
        if loaded.len() > self.capacity {
            let drop = loaded.len() - self.capacity;
            loaded.drain(0..drop);
        }
        self.entries = loaded.into();
        self.written = std::fs::metadata(&file).map(|meta| meta.len()).unwrap_or(0);
    }
}

fn format_line(entry: &ActivityEntry) -> std::io::Result<String> {
    let ts = entry.at.timestamp();
    let text = entry.text.replace(['\n', '\r'], " ");
    Ok(format!("{} {} {}\n", ts, entry.kind.marker(), text))
}

fn parse_line(line: &str) -> Option<ActivityEntry> {
    let line = line.trim_end_matches('\n');
    let mut parts = line.splitn(3, ' ');
    let ts: i64 = parts.next()?.parse().ok()?;
    let marker = parts.next()?.chars().next()?;
    let kind = ActivityKind::from_marker(marker)?;
    let text = parts.next()?.to_string();
    Some(ActivityEntry {
        at: Local.timestamp_opt(ts, 0).single()?,
        kind,
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(text: &str) -> ActivityEntry {
        ActivityEntry {
            at: Local::now(),
            kind: ActivityKind::Action,
            text: text.to_string(),
        }
    }

    #[test]
    fn keeps_only_the_most_recent_capacity_entries() {
        let mut log = ActivityLog::in_memory(3);
        for i in 0..5 {
            log.log(ActivityKind::Action, format!("step {i}"));
        }
        assert_eq!(log.len(), 3, "old entries are evicted past the capacity");
        let texts: Vec<&str> = log
            .entries()
            .iter()
            .map(|entry| entry.text.as_str())
            .collect();
        assert_eq!(texts, vec!["step 2", "step 3", "step 4"]);
    }

    #[test]
    fn tail_returns_the_newest_entry() {
        let mut log = ActivityLog::in_memory(8);
        assert!(log.tail().is_none());
        log.log(ActivityKind::Error, "boom");
        log.log(ActivityKind::Command, "systemctl --user daemon-reload");
        let tail = log.tail().unwrap();
        assert_eq!(tail.kind, ActivityKind::Command);
        assert!(tail.text.contains("daemon-reload"));
    }

    #[test]
    fn entries_keep_their_kind_and_order() {
        let mut log = ActivityLog::in_memory(8);
        log.log(ActivityKind::Action, "saved all-daily");
        log.log(
            ActivityKind::Command,
            "systemctl --user enable --now topmatic@all-daily.timer",
        );
        log.log(ActivityKind::Error, "sync errors: boom");
        let kinds: Vec<ActivityKind> = log.entries().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ActivityKind::Action,
                ActivityKind::Command,
                ActivityKind::Error
            ]
        );
    }

    #[test]
    fn zero_capacity_swallows_everything() {
        let mut log = ActivityLog::in_memory(0);
        log.log(ActivityKind::Action, "nope");
        assert!(log.is_empty());
    }

    #[test]
    fn format_round_trips_through_parse_line() {
        let entry = action("saved all-daily — run history purged");
        let line = format_line(&entry).unwrap();
        let parsed = parse_line(line.trim_end()).unwrap();
        assert_eq!(parsed.kind, entry.kind);
        assert_eq!(parsed.text, entry.text);
        assert_eq!(parsed.at.timestamp(), entry.at.timestamp());
    }

    #[test]
    fn newlines_are_flattened_into_the_serialized_line() {
        let entry = ActivityEntry {
            at: Local::now(),
            kind: ActivityKind::Command,
            text: "line one\nline two".to_string(),
        };
        let line = format_line(&entry).unwrap();
        assert_eq!(
            line.lines().count(),
            1,
            "a persisted entry is always one line"
        );
        assert!(line.contains("line one line two"));
    }

    #[test]
    fn parse_line_drops_garbage_without_losing_the_rest() {
        let mut log = ActivityLog::with_file_bounded(
            8,
            std::env::temp_dir().join(format!("activity-mixed-{}", std::process::id())),
            4096,
        );
        std::fs::write(log.file.clone().unwrap(), "garbage\nnot parsable\n").unwrap();
        log.load_history();
        assert!(log.is_empty(), "unparseable lines are skipped");
        let _ = std::fs::remove_file(log.file.clone().unwrap());
    }

    #[test]
    fn load_history_restores_only_the_last_capacity_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("activity.log");
        let mut log = ActivityLog::in_memory(64);
        let mut expected = Vec::new();
        for i in 0..5 {
            let entry = ActivityEntry {
                at: Local::now(),
                kind: ActivityKind::Action,
                text: format!("session one {i}"),
            };
            expected.push(entry.text.clone());
            log.log(ActivityKind::Action, format!("session one {i}"));
        }
        std::fs::write(
            &file,
            log.entries()
                .iter()
                .filter_map(|e| format_line(e).ok())
                .collect::<String>(),
        )
        .unwrap();

        let reloaded = ActivityLog::with_file(64, file.clone());
        assert_eq!(reloaded.len(), 5);
        let texts: Vec<&str> = reloaded.entries().iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, expected);
        let _ = dir;
    }

    #[test]
    fn log_appends_to_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("activity.log");
        let mut log = ActivityLog::with_file(64, file.clone());
        assert!(log.is_empty());
        log.log(ActivityKind::Action, "boot");
        log.log(ActivityKind::Error, "sad");
        let on_disk = std::fs::read_to_string(&file).unwrap();
        assert_eq!(on_disk.lines().count(), 2);
        assert!(on_disk.contains(" boot\n") && on_disk.contains("sad\n"));
        let _ = dir;
    }

    #[test]
    fn oversize_log_is_rotated_from_memory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("activity.log");
        let mut log = ActivityLog::with_file_bounded(32, file.clone(), 2048);
        log.log(ActivityKind::Action, "booting");
        let long = "x".repeat(300);
        for _ in 0..20 {
            log.log(ActivityKind::Command, format!("run {long}"));
        }
        let meta = std::fs::metadata(&file).unwrap();
        assert!(
            meta.len() <= 2048,
            "the file is bounded by the cap: {} bytes",
            meta.len()
        );
        assert!(log.written <= 2048);
        assert!(
            !log.is_empty(),
            "rotate keeps the newest tail so the log never goes blank"
        );
        let _ = dir;
    }
}
