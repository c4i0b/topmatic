use crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectTarget {
    Preset,
    Weekday,
    Notify,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowEditor {
    pub title: String,
    pub options: Vec<String>,
    pub current: usize,
    pub index: usize,
    pub target: SelectTarget,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RowEditorEvent {
    Changed,
    Confirmed,
    Cancelled,
}

impl RowEditor {
    pub fn select(title: &str, options: Vec<String>, current: usize, target: SelectTarget) -> Self {
        Self {
            title: title.to_string(),
            options,
            index: current,
            current,
            target,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> RowEditorEvent {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                self.index = self.index.saturating_sub(1);
                RowEditorEvent::Changed
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                self.index = (self.index + 1).min(self.options.len().saturating_sub(1));
                RowEditorEvent::Changed
            }
            KeyCode::Enter => RowEditorEvent::Confirmed,
            KeyCode::Esc => RowEditorEvent::Cancelled,
            _ => RowEditorEvent::Changed,
        }
    }

    pub fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for (position, option) in self.options.iter().enumerate() {
            lines.push(Line::from(format!(
                "{}{} {}{}",
                if position == self.index { "▶ " } else { "  " },
                if position == self.current { "[" } else { " " },
                option,
                if position == self.current { "]" } else { " " },
            )));
        }
        lines.push(Line::from(Span::styled(
            " enter confirm · esc cancel",
            Style::new().fg(Color::DarkGray),
        )));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn popup() -> RowEditor {
        RowEditor::select(
            "frequency",
            vec![
                "daily".to_string(),
                "weekly".to_string(),
                "custom".to_string(),
            ],
            0,
            SelectTarget::Preset,
        )
    }

    #[test]
    fn cursor_starts_on_current_and_moves_with_clamping() {
        let mut editor = popup();
        assert_eq!(editor.index, 0);
        editor.handle_key(key(KeyCode::Up));
        assert_eq!(editor.index, 0, "cursor clamps at the top");
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Char('j')));
        assert_eq!(editor.index, 2);
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Char('j')));
        assert_eq!(editor.index, 2, "cursor clamps at the bottom");
        editor.handle_key(key(KeyCode::Char('k')));
        assert_eq!(editor.index, 1);
    }

    #[test]
    fn enter_confirms_and_esc_cancels() {
        let mut editor = popup();
        editor.handle_key(key(KeyCode::Down));
        assert_eq!(
            editor.handle_key(key(KeyCode::Enter)),
            RowEditorEvent::Confirmed
        );
        assert_eq!(
            editor.handle_key(key(KeyCode::Esc)),
            RowEditorEvent::Cancelled
        );
    }

    #[test]
    fn q_and_other_keys_are_swallowed_not_quit() {
        let mut editor = popup();
        assert_eq!(
            editor.handle_key(key(KeyCode::Char('q'))),
            RowEditorEvent::Changed
        );
        assert_eq!(editor.index, 0, "q does not move the cursor");
    }

    #[test]
    fn lines_mark_cursor_and_current_value() {
        let mut editor = popup();
        editor.current = 1;
        editor.index = 2;
        let text: Vec<String> = editor
            .lines()
            .iter()
            .map(|line| line.iter().map(|span| span.content.as_ref()).collect())
            .collect();
        assert_eq!(text[0], "    daily ");
        assert_eq!(text[1], "  [ weekly]");
        assert_eq!(text[2], "▶   custom ");
    }
}
