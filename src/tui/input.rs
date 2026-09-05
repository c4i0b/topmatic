use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    pub value: String,
}

impl LineEdit {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.value.push(c);
            }
            KeyCode::Backspace => {
                self.value.pop();
            }
            _ => {}
        }
    }
}

pub struct FilterState {
    pub edit: LineEdit,
    pub active: bool,
}

impl FilterState {
    pub fn new() -> Self {
        Self {
            edit: LineEdit::new(String::new()),
            active: false,
        }
    }

    pub fn is_engaged(&self) -> bool {
        self.active || !self.edit.value.is_empty()
    }

    pub fn text(&self) -> &str {
        &self.edit.value
    }

    pub fn matches(&self, haystack: &str) -> bool {
        let needle = self.edit.value.to_lowercase();
        needle.is_empty() || haystack.to_lowercase().contains(&needle)
    }

    pub fn start(&mut self) {
        self.active = true;
    }

    pub fn handle(&mut self, key: KeyEvent) -> FilterAction {
        match key.code {
            KeyCode::Enter => {
                self.active = false;
                FilterAction::Committed
            }
            KeyCode::Esc => {
                self.active = false;
                self.edit = LineEdit::new(String::new());
                FilterAction::Cleared
            }
            _ => {
                self.edit.handle_key(key);
                FilterAction::Changed
            }
        }
    }
}

impl Default for FilterState {
    fn default() -> Self {
        Self::new()
    }
}

pub enum FilterAction {
    Changed,
    Committed,
    Cleared,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn appends_and_deletes_characters() {
        let mut edit = LineEdit::new("flat");
        edit.handle_key(key(KeyCode::Char('-')));
        edit.handle_key(key(KeyCode::Char('d')));
        assert_eq!(edit.value, "flat-d");
        edit.handle_key(key(KeyCode::Backspace));
        assert_eq!(edit.value, "flat-");
    }

    #[test]
    fn ignores_control_shortcuts() {
        let mut edit = LineEdit::new("x");
        edit.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(edit.value, "x");
    }

    #[test]
    fn filter_lifecycle_start_type_commit_clear() {
        let mut filter = FilterState::new();
        assert!(!filter.is_engaged());
        assert!(filter.matches("anything"));

        filter.start();
        assert!(filter.is_engaged());
        assert!(matches!(
            filter.handle(key(KeyCode::Char('f'))),
            FilterAction::Changed
        ));
        assert_eq!(filter.text(), "f");
        assert!(filter.matches("Flatpak"));
        assert!(!filter.matches("cargo"));

        assert!(matches!(
            filter.handle(key(KeyCode::Enter)),
            FilterAction::Committed
        ));
        assert!(
            !filter.active,
            "committed filter stops editing but keeps matching"
        );
        assert!(filter.matches("flatpak"));

        filter.start();
        assert!(matches!(
            filter.handle(key(KeyCode::Esc)),
            FilterAction::Cleared
        ));
        assert_eq!(filter.text(), "");
        assert!(filter.matches("cargo"));
    }
}
