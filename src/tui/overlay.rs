use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub title: String,
    pub lines: Vec<Line<'static>>,
    pub options: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAction {
    Selected(usize),
    Cancelled,
}

impl Overlay {
    pub fn new(title: &str, lines: Vec<Line<'static>>, options: &[&str], selected: usize) -> Self {
        Self {
            title: title.to_string(),
            lines,
            options: options.iter().map(|option| option.to_string()).collect(),
            selected,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<OverlayAction> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                self.selected = (self.selected + 1).min(self.options.len().saturating_sub(1));
                None
            }
            KeyCode::Enter => Some(OverlayAction::Selected(self.selected)),
            KeyCode::Esc => Some(OverlayAction::Cancelled),
            _ => None,
        }
    }

    pub fn height(&self) -> u16 {
        let blank = usize::from(!self.lines.is_empty());
        (self.lines.len() + blank + self.options.len() + 3) as u16
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let popup = super::views::centered_rect(area, 60, self.height());
        let mut body = self.lines.clone();
        if !body.is_empty() {
            body.push(Line::from(""));
        }
        for (index, option) in self.options.iter().enumerate() {
            let line = if index == self.selected {
                Line::styled(
                    format!("▶ {option}"),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )
            } else {
                Line::from(format!("  {option}"))
            };
            body.push(line);
        }
        let hint = if self.options.is_empty() {
            "esc closes"
        } else {
            "enter select · esc cancel"
        };
        body.push(Line::from(Span::styled(
            hint,
            Style::new().fg(Color::DarkGray),
        )));
        let paragraph = Paragraph::new(body).block(Block::bordered().title(self.title.clone()));
        frame.render_widget(Clear, popup);
        frame.render_widget(paragraph, popup);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn overlay() -> Overlay {
        Overlay::new(
            "confirm",
            vec![Line::from("delete this?")],
            &["Delete", "Cancel"],
            1,
        )
    }

    #[test]
    fn arrows_move_with_clamping_and_enter_reports_the_selection() {
        let mut overlay = overlay();
        assert_eq!(overlay.selected, 1);
        overlay.handle_key(key(KeyCode::Down));
        assert_eq!(overlay.selected, 1, "clamps at the bottom");
        overlay.handle_key(key(KeyCode::Up));
        assert_eq!(overlay.selected, 0);
        overlay.handle_key(key(KeyCode::Up));
        assert_eq!(overlay.selected, 0, "clamps at the top");
        assert_eq!(
            overlay.handle_key(key(KeyCode::Enter)),
            Some(OverlayAction::Selected(0))
        );
    }

    #[test]
    fn esc_always_cancels_and_q_is_swallowed() {
        let mut overlay = overlay();
        assert_eq!(
            overlay.handle_key(key(KeyCode::Esc)),
            Some(OverlayAction::Cancelled)
        );
        assert_eq!(
            overlay.handle_key(key(KeyCode::Char('q'))),
            None,
            "q must never quit the app from inside an overlay"
        );
        assert_eq!(overlay.selected, 1, "q does not move the cursor");
    }

    #[test]
    fn empty_options_shows_a_read_only_close_hint() {
        use ratatui::{Terminal, backend::TestBackend};
        let overlay = Overlay::new("help", vec![Line::from("a row")], &[], 0);
        let mut terminal = Terminal::new(TestBackend::new(50, 10)).unwrap();
        terminal
            .draw(|frame| overlay.render(frame, frame.area()))
            .unwrap();
        let rows: Vec<String> = terminal
            .backend()
            .buffer()
            .content()
            .chunks(50)
            .map(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect();
        let visible = rows.join("\n");
        assert!(
            visible.contains("esc closes"),
            "read-only overlay hint:\n{visible}"
        );
        let hint_row = rows.iter().find(|r| r.contains("esc closes")).unwrap();
        assert!(
            hint_row.contains("│esc closes"),
            "hint aligns with the body content, one column after the border:\n{visible}"
        );
    }

    #[test]
    fn render_shows_title_cursor_and_hints() {
        use ratatui::{Terminal, backend::TestBackend};
        let overlay = overlay();
        let mut terminal = Terminal::new(TestBackend::new(50, 14)).unwrap();
        terminal
            .draw(|frame| overlay.render(frame, frame.area()))
            .unwrap();
        let rows: Vec<String> = terminal
            .backend()
            .buffer()
            .content()
            .chunks(50)
            .map(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect();
        let visible = rows.join("\n");
        for token in [
            "confirm",
            "delete this?",
            "▶ Cancel",
            "  Delete",
            "esc cancel",
        ] {
            assert!(visible.contains(token), "missing {token:?}:\n{visible}");
        }
    }
}
