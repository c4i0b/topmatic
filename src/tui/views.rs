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
use super::overlay::Overlay;
use super::presets;

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
    }
    if let Some((_, overlay)) = &app.confirm {
        overlay.render(frame, body);
    }
    if let Some(overlay) = &app.help {
        overlay.render(frame, body);
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
        Some(false) => Span::styled("linger: off", Style::new().fg(Color::Yellow)),
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
        Some((format!(" /{}", app.filter.text()), Color::Yellow))
    } else {
        status_prefix(&app.message, &app.last_action)
    };
    let styled = match prefix {
        Some((text, color)) => vec![Span::styled(text, Style::new().fg(color))],
        None => Vec::new(),
    };
    let mut spans = styled;
    spans.push(Span::styled(
        format!(" {hints}"),
        Style::new().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(crate) fn status_prefix(message: &str, last_action: &str) -> Option<(String, Color)> {
    if !message.is_empty() {
        Some((format!(" {message} "), Color::Red))
    } else if !last_action.is_empty() {
        Some((format!(" {last_action} "), Color::Yellow))
    } else {
        None
    }
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
        View::Dashboard => "/ filter  n new  e edit  d delete  r run now  l logs  ? help  q quit",
        View::Editor(_) => {
            "↑↓ move  enter edit/save  tab section  / filter steps  esc back  q quit"
        }
        View::PresetPicker { .. } => "enter choose  esc back  q quit",
        View::Logs(state) if state.follow => "x stop  esc background  q quit",
        View::Logs(_) => "enter open  h back  r refresh  esc back  q quit",
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

pub(crate) struct KeyGroup<'a> {
    pub title: &'a str,
    pub keys: &'a [(&'a str, &'a str)],
}

fn group_lines(group: &KeyGroup) -> Vec<Line<'static>> {
    let width = group
        .keys
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(1);
    let mut lines = vec![
        Span::styled(
            group.title.to_string(),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )
        .into(),
    ];
    for (key, label) in group.keys {
        lines.push(Span::raw(format!("  {key:width$}  {label}")).into());
    }
    lines.push(Line::from(""));
    lines
}

pub(crate) fn dashboard_help() -> Vec<Line<'static>> {
    let groups = &[
        KeyGroup {
            title: "Profiles",
            keys: &[
                ("↑/↓ or j/k", "move selection"),
                ("enter / e", "edit the selected profile"),
                ("n", "new profile (from presets)"),
                ("d", "delete profile — always asks first"),
                ("r", "run now, opens the live view"),
                ("l", "browse run logs"),
                ("/", "filter profiles by name"),
            ],
        },
        KeyGroup {
            title: "Global",
            keys: &[("? / esc", "close this help"), ("q", "quit topmatic")],
        },
    ];
    let mut lines: Vec<Line<'static>> = Vec::new();
    for group in groups {
        lines.extend(group_lines(group));
    }
    lines.push(Line::from(Span::styled(
        "schedules run at midnight; missed runs catch up on next boot",
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "keep running logged out: loginctl enable/disable-linger",
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "config: ~/.config/topmatic/config.toml (source of truth)",
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "topgrade runs isolated, no sudo",
        Style::new().fg(Color::DarkGray),
    )));
    lines
}

pub(crate) fn editor_help() -> Vec<Line<'static>> {
    let groups = &[
        KeyGroup {
            title: "Move & edit",
            keys: &[
                ("tab / shift-tab", "switch section"),
                ("↑/↓ or j/k", "move in the current section"),
                ("enter / space", "act on the highlighted row"),
                ("/", "filter the steps list"),
            ],
        },
        KeyGroup {
            title: "Save & leave",
            keys: &[
                (
                    "save row → enter",
                    "type a name, then Confirm on the summary",
                ),
                ("esc", "cancel a popup, the editor, or this help"),
                ("q", "quit topmatic"),
            ],
        },
        KeyGroup {
            title: "Rows",
            keys: &[
                ("preset", "frequency (bi-weekly renders as two lines)"),
                ("notify", "always / on failure / never"),
                ("steps", "pick what runs; a * marks unsaved changes"),
            ],
        },
    ];
    let mut lines: Vec<Line<'static>> = Vec::new();
    for group in groups {
        lines.extend(group_lines(group));
    }
    lines
}

pub(crate) fn help_overlay(title: &str, lines: Vec<Line<'static>>) -> Overlay {
    Overlay::new(title, lines, &[], 0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::editor::EditorState;

    #[test]
    fn status_prefix_prioritizes_errors_then_info() {
        assert_eq!(
            status_prefix("", ""),
            None,
            "nothing to say renders nothing"
        );
        assert_eq!(
            status_prefix("a FAILED (exit 1)", ""),
            Some((" a FAILED (exit 1) ".to_string(), Color::Red)),
            "errors render in red"
        );
        assert_eq!(
            status_prefix("", "deleted all-daily"),
            Some((" deleted all-daily ".to_string(), Color::Yellow)),
            "info renders in yellow"
        );
        assert_eq!(
            status_prefix("start failed: boom", "started all-daily"),
            Some((" start failed: boom ".to_string(), Color::Red)),
            "an error outranks the last action"
        );
    }

    #[test]
    fn help_sections_fit_the_overlay_panel() {
        for (which, lines) in [("dashboard", dashboard_help()), ("editor", editor_help())] {
            for line in lines {
                let width = line.width();
                let text: String = line.iter().map(|s| s.content.as_ref()).collect();
                assert!(
                    width <= 62,
                    "{which} help line overflows the overlay panel ({width} cols): {text:?}"
                );
            }
        }
    }

    #[test]
    fn dashboard_help_groups_keys_by_section() {
        let text: String = dashboard_help()
            .iter()
            .flat_map(|line| line.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("\n");
        for section in ["Profiles", "Global"] {
            assert!(text.contains(section), "missing {section:?}:\n{text}");
        }
        for action in [
            "edit the selected profile",
            "delete profile",
            "run now",
            "filter profiles",
        ] {
            assert!(text.contains(action), "missing {action:?}:\n{text}");
        }
    }

    #[test]
    fn editor_help_is_contextual() {
        let text: String = editor_help()
            .iter()
            .flat_map(|line| line.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("\n");
        for action in [
            "switch section",
            "filter the steps list",
            "Confirm on the summary",
        ] {
            assert!(text.contains(action), "missing {action:?}:\n{text}");
        }
        assert!(
            !text.contains("loginctl"),
            "editor help has no dashboard noise:\n{text}"
        );
    }

    #[test]
    fn help_overlay_is_modal_and_read_only() {
        let overlay = help_overlay("help", dashboard_help());
        assert!(overlay.options.is_empty(), "help has no selectable options");
        let text: String = overlay
            .lines
            .iter()
            .flat_map(|l| l.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(text.contains("Profiles"));
    }

    #[test]
    fn help_explains_what_lingering_is_for() {
        let text: String = dashboard_help()
            .iter()
            .flat_map(|line| line.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("loginctl enable/disable-linger"),
            "help must say how to control linger:\n{text}"
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
        assert!(
            footer_hints(
                &View::Editor(Box::new(EditorState::new(
                    None,
                    Vec::new(),
                    crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC
                )),),
                false
            )
            .contains("enter edit/save")
        );
        assert!(footer_hints(&View::Dashboard, false).contains("q quit"));
        assert!(
            footer_hints(&View::Logs(crate::tui::logs::LogsState::follow("x")), false)
                .contains("x stop")
        );
        assert!(footer_hints(&View::Dashboard, true).starts_with("filter"));
    }
}
