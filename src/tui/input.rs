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
}
