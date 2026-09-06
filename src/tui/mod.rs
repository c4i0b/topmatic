mod app;
mod dashboard;
mod editor;
mod input;
mod logs;
mod overlay;
mod presets;
mod views;

use std::time::Duration;

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

pub use app::App;

const LIST_VISIBLE: usize = 14;

pub enum View {
    Dashboard,
    PresetPicker { index: usize },
    Editor(Box<editor::EditorState>),
    Logs(logs::LogsState),
}

pub fn run() -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    let result = app_loop(&mut terminal);
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn app_loop(terminal: &mut ratatui::DefaultTerminal) -> anyhow::Result<()> {
    let mut app = App::boot()?;
    loop {
        terminal.draw(|frame| views::draw(&app, frame))?;
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key);
                    }
                }
                Event::Mouse(mouse) => app.handle_mouse(mouse),
                _ => {}
            }
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}

pub(crate) fn focus_marker(focused: bool) -> Span<'static> {
    if focused {
        Span::styled(
            "▸",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw(" ")
    }
}
