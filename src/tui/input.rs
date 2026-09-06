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

    pub fn filter_query(&self) -> String {
        format!("{}{}", self.edit.value, if self.active { "▏" } else { "" })
    }

    pub fn matches(&self, haystack: &str) -> bool {
        let needle = self.edit.value.to_lowercase();
        needle.is_empty() || haystack.to_lowercase().contains(&needle)
    }

    pub fn start(&mut self) {
        self.active = true;
    }

    pub fn clear_query(&mut self) {
        self.edit = LineEdit::new(String::new());
        self.active = false;
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

pub enum FilterableAction {
    Navigated,
    Filtered,
}

pub fn handle_filter_typing(
    filter: &mut FilterState,
    key: KeyEvent,
    index: &mut usize,
    len: usize,
    stride: usize,
) -> FilterableAction {
    let stride = stride.max(1);
    match key.code {
        KeyCode::Up => {
            *index = index.saturating_sub(stride);
            FilterableAction::Navigated
        }
        KeyCode::Down => {
            *index = if len == 0 {
                0
            } else {
                (*index + stride).min(len - 1)
            };
            FilterableAction::Navigated
        }
        _ => {
            filter.handle(key);
            FilterableAction::Filtered
        }
    }
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

    #[test]
    fn clear_query_drops_the_match_and_leaves_edit_mode() {
        let mut filter = FilterState::new();
        filter.start();
        filter.edit.value = "ca".to_string();
        filter.clear_query();
        assert!(!filter.is_engaged());
        assert!(filter.matches("anything"));
    }

    #[test]
    fn filter_typing_navigates_with_the_list_stride() {
        let mut filter = FilterState::new();
        filter.start();
        let mut index = 0usize;

        assert!(matches!(
            handle_filter_typing(&mut filter, key(KeyCode::Down), &mut index, 10, 3),
            FilterableAction::Navigated
        ));
        assert_eq!(index, 3, "down moves by the stride");
        handle_filter_typing(&mut filter, key(KeyCode::Up), &mut index, 10, 3);
        assert_eq!(index, 0, "up saturates at the top");

        handle_filter_typing(&mut filter, key(KeyCode::Down), &mut index, 10, 1);
        assert_eq!(index, 1, "a linear list uses stride 1");
        handle_filter_typing(&mut filter, key(KeyCode::Down), &mut index, 5, 3);
        assert_eq!(index, 4, "down clamps to the last surviving row");
        handle_filter_typing(&mut filter, key(KeyCode::Down), &mut index, 0, 3);
        assert_eq!(index, 0, "an empty list keeps the index at zero");
    }

    #[test]
    fn filter_typing_edits_the_query_for_everything_else() {
        let mut filter = FilterState::new();
        filter.start();
        let mut index = 2usize;
        assert!(matches!(
            handle_filter_typing(&mut filter, key(KeyCode::Char('f')), &mut index, 10, 3),
            FilterableAction::Filtered
        ));
        assert_eq!(filter.text(), "f");
        assert_eq!(index, 2, "plain keys never move the selection");
        assert!(matches!(
            handle_filter_typing(&mut filter, key(KeyCode::Enter), &mut index, 10, 3),
            FilterableAction::Filtered
        ));
        assert!(!filter.active);
    }
}
