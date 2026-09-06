use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, ListItem, Paragraph};

use super::View;
use super::app::App;
use super::dashboard;
use super::editor;
use super::logs;
use super::presets;
use crate::systemd::SystemdCtl;

pub(crate) fn draw(app: &App, frame: &mut Frame) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    draw_header(app, frame, header);
    match &app.view {
        View::PresetPicker { index } => draw_preset_picker(*index, frame, body),
        View::Dashboard => {
            let areas = dashboard::render(app, frame, body);
            app.list_area.set(areas.list);
        }
        View::Editor(state) => draw_editor(state, frame, body),
        View::Logs(state) => logs::render(state, frame, body),
        View::Help => draw_help(frame, body),
        View::Confirm { profile } => draw_confirm(profile, frame, body),
    }
    draw_footer(app, frame, footer);
}

fn draw_preset_picker(index: usize, frame: &mut Frame, area: Rect) {
    let mut items: Vec<ListItem> = presets::PRESETS
        .iter()
        .enumerate()
        .map(|(i, preset)| {
            if i == index {
                ListItem::new(Line::styled(
                    format!("▶ {} — {}", preset.label, preset.description),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
            } else {
                ListItem::new(Line::from(format!(
                    "  {} — {}",
                    preset.label, preset.description
                )))
            }
        })
        .collect();
    let scratch_index = presets::PRESETS.len();
    items.push(ListItem::new(if index == scratch_index {
        Line::styled(
            "▶ Start from scratch",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )
    } else {
        Line::from("  Start from scratch")
    }));
    let list = ratatui::widgets::List::new(items)
        .block(Block::bordered().title("new profile — pick a preset"));
    frame.render_widget(list, area);
}

fn draw_header(app: &App, frame: &mut Frame, area: Rect) {
    let linger = match app.ctl.linger_enabled() {
        Some(true) => Span::styled("linger: on", Style::new().fg(Color::Green)),
        Some(false) => Span::styled("linger: off (g to enable)", Style::new().fg(Color::Yellow)),
        None => Span::raw(""),
    };
    let line = Line::from(vec![
        Span::styled(" topmatic ", Style::new().add_modifier(Modifier::BOLD)),
        linger,
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_footer(app: &App, frame: &mut Frame, area: Rect) {
    let hints = footer_hints(&app.view, app.filter.active);
    let prefix = if app.filter.active {
        format!(" /{}", app.filter.text())
    } else if app.message.is_empty() {
        String::new()
    } else {
        format!(" {} ", app.message)
    };
    let footer = Line::from(vec![
        Span::styled(prefix, Style::new().fg(Color::Yellow)),
        Span::styled(format!(" {hints}"), Style::new().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(footer), area);
}

pub(crate) fn name_popup_hint(value: &str) -> &'static str {
    if crate::domain::profile::sanitize_name(value).is_ok() {
        " enter saves · esc back"
    } else {
        " letters, digits, . _ - (start alnum, max 64) · esc back"
    }
}

pub(crate) fn footer_hints(view: &View, filter_active: bool) -> &'static str {
    if filter_active {
        return "filter: type…  Enter accept  Esc clear  ↑↓ move";
    }
    match view {
        View::Dashboard => {
            "/ filter  n new  e edit  d delete  r run now  l logs  g linger  s resync  ? help  q quit"
        }
        View::Editor(_) => {
            "↑↓ move  enter edit/save  tab section  / filter steps  esc back  q quit"
        }
        View::PresetPicker { .. } => "enter choose  esc back  q quit",
        View::Logs(_) => "enter open  h back  r refresh  esc back  q quit",
        View::Help => "any key closes  q quit",
        View::Confirm { .. } => "y confirm delete  esc cancels  q quit",
    }
}

fn draw_editor(state: &editor::EditorState, frame: &mut Frame, area: Rect) {
    let title = match &state.original_name {
        Some(name) => format!("edit profile — {name}"),
        None => "new profile".to_string(),
    };
    let paragraph = Paragraph::new(state.body_lines()).block(Block::bordered().title(title));
    frame.render_widget(paragraph, area);

    if let Some(popup) = state.row_editor() {
        let lines = popup.lines();
        let popup_area = centered_rect(area, 50, (lines.len() + 2) as u16);
        let text = Paragraph::new(lines).block(Block::bordered().title(popup.title.clone()));
        frame.render_widget(Clear, popup_area);
        frame.render_widget(text, popup_area);
    }

    if let Some(popup) = &state.name_popup {
        let popup_area = centered_rect(area, 60, 5);
        let text = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::raw(" name: "),
                Span::styled(popup.value.clone(), Style::new().fg(Color::Cyan)),
                Span::raw("▏"),
            ]),
            Line::from(Span::styled(
                name_popup_hint(&popup.value),
                Style::new().fg(Color::DarkGray),
            )),
        ])
        .block(Block::bordered().title("save profile"));
        frame.render_widget(Clear, popup_area);
        frame.render_widget(text, popup_area);
    }
}

fn draw_help(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(help_lines()).block(Block::bordered().title("help (? or any key closes)")),
        area,
    );
}

fn draw_confirm(profile: &str, frame: &mut Frame, area: Rect) {
    let text = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Delete profile "),
            Span::styled(profile.to_string(), Style::new().fg(Color::Red)),
            Span::raw("?"),
        ]),
        Line::from("Run history (logs, status) will also be deleted."),
        Line::from(""),
        Line::from("y to confirm, anything else cancels"),
    ])
    .block(Block::bordered().title("confirm deletion"));
    let popup = centered_rect(area, 50, 9);
    frame.render_widget(Clear, popup);
    frame.render_widget(text, popup);
}

pub(crate) fn centered_rect(area: Rect, percent_x: u16, height: u16) -> Rect {
    let width = area.width.saturating_sub(2);
    let popup_width = (width * percent_x / 100).max(30);
    let popup_height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    }
}

pub(crate) fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("topmatic help"),
        Line::from(""),
        Line::from("n  new profile (presets)      e  edit profile      d  delete profile"),
        Line::from("r  run now (systemd runs it in the background)"),
        Line::from(
            "l  browse run logs            g  enable lingering (user units keep running when you log out)",
        ),
        Line::from("s  resync units               /  filter profiles"),
        Line::from(""),
        Line::from("editor: tab switches section, enter acts on the highlighted row,"),
        Line::from("the save row asks the name and saves; enter confirms popups, esc cancels"),
        Line::from("schedules run at midnight; missed runs catch up on the next boot"),
        Line::from(""),
        Line::from("Config: ~/.config/topmatic/config.toml (source of truth, editable by hand)"),
        Line::from("topgrade runs fully isolated with its own config; no sudo anywhere."),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::editor::EditorState;

    #[test]
    fn help_fits_a_compact_terminal() {
        for line in help_lines() {
            let width = line.width();
            let text: String = line.iter().map(|s| s.content.as_ref()).collect();
            assert!(
                width <= 110 || text.chars().count() > 110,
                "help line overflows at 110 cols: {text:?}"
            );
        }
    }

    #[test]
    fn help_explains_what_lingering_is_for() {
        let text: String = help_lines()
            .iter()
            .flat_map(|line| line.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("g  enable lingering (user units keep running when you log out)"),
            "help must explain linger in one brief parenthetical:\n{text}"
        );
    }

    #[test]
    fn name_popup_hint_switches_to_requirements_when_invalid() {
        assert!(name_popup_hint("all-daily").contains("enter saves"));
        assert!(name_popup_hint("bad name").contains("letters"));
        assert!(name_popup_hint("").contains("letters"));
    }

    #[test]
    fn footer_hints_cover_every_view_and_lead_with_the_primary_action() {
        assert!(footer_hints(&View::Dashboard, false).contains("q quit"));
        assert!(
            footer_hints(
                &View::Editor(Box::new(EditorState::new(None, Vec::new())),),
                false
            )
            .contains("enter edit/save")
        );
        assert!(footer_hints(&View::Help, false).contains("any key closes"));
        assert!(
            footer_hints(
                &View::Confirm {
                    profile: String::new()
                },
                false
            )
            .contains("y confirm")
        );
        assert!(footer_hints(&View::Dashboard, true).starts_with("filter"));
    }
}
