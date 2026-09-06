use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, ListItem, Paragraph};

use crate::activity::{ActivityEntry, ActivityKind};

use super::View;
use super::app::App;
use super::dashboard;
use super::editor;
use super::logs;
use super::overlay::Overlay;
use super::presets;

pub(crate) fn draw(app: &App, frame: &mut Frame) {
    let panel_open = app.activity_panel && matches!(app.view, View::Dashboard);
    let [header, body, footer] = if panel_open {
        let [header, body, panel, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Max(8),
            Constraint::Length(2),
        ])
        .areas(frame.area());
        app.activity_area.set(panel);
        draw_activity_panel(app, frame, panel);
        [header, body, footer]
    } else {
        app.activity_area.set(Rect::default());
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .areas(frame.area());
        [header, body, footer]
    };

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
    let filter_typing =
        app.filter.active || matches!(&app.view, View::Editor(state) if state.steps_filter.active);
    let hints = footer_hints(&app.view, filter_typing);
    let prefix = if let Some(active_filter) = active_filter(app) {
        Some((format!(" /{}", active_filter), Color::Yellow))
    } else if let Some(job) = &app.in_flight {
        in_flight_prefix(job)
    } else if !app.message.is_empty() {
        Some((format!(" {} ", app.message), Color::Red))
    } else {
        app.activity
            .lock()
            .unwrap()
            .tail()
            .and_then(activity_prefix)
    };
    match prefix {
        Some((text, color)) => {
            let [status_area, hints_area] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(text, Style::new().fg(color))])),
                status_area,
            );
            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(
                    format!(" {hints}"),
                    Style::new().fg(Color::DarkGray),
                )])),
                hints_area,
            );
        }
        None => frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!(" {hints}"),
                Style::new().fg(Color::DarkGray),
            )])),
            area,
        ),
    }
}

const SPINNER_FRAMES: [char; 4] = ['◐', '◓', '◑', '◒'];

pub(crate) fn spinner_frame(elapsed: Duration) -> char {
    let index = (elapsed.as_millis() / 100) as usize % SPINNER_FRAMES.len();
    SPINNER_FRAMES[index]
}

pub(crate) fn in_flight_prefix(job: &super::app::BackgroundJob) -> Option<(String, Color)> {
    Some((
        format!(" {} {}… ", spinner_frame(job.started.elapsed()), job.label),
        Color::Yellow,
    ))
}

pub(crate) fn activity_prefix(entry: &ActivityEntry) -> Option<(String, Color)> {
    Some((format!(" {} ", entry.text), activity_color(entry.kind)))
}

fn active_filter(app: &App) -> Option<String> {
    match &app.view {
        View::Dashboard if app.filter.is_engaged() => Some(app.filter.filter_query()),
        View::Editor(state) if state.steps_filter.is_engaged() => {
            Some(state.steps_filter.filter_query())
        }
        _ => None,
    }
}

pub(crate) fn activity_color(kind: ActivityKind) -> Color {
    match kind {
        ActivityKind::Error => Color::Red,
        ActivityKind::Command => Color::Magenta,
        ActivityKind::Action => Color::Yellow,
    }
}

fn activity_lines(
    entries: &[ActivityEntry],
    height: usize,
    offset: usize,
) -> Vec<(ActivityKind, String, String)> {
    let total = entries.len();
    let offset = offset.min(total.saturating_sub(1));
    let end = total - offset;
    let start = end.saturating_sub(height);
    entries[start..end]
        .iter()
        .map(|entry| {
            (
                entry.kind,
                entry.at.format("%H:%M:%S").to_string(),
                entry.text.clone(),
            )
        })
        .collect()
}

fn draw_activity_panel(app: &App, frame: &mut Frame, area: Rect) {
    let height = area.height.saturating_sub(1) as usize;
    let snapshot: Vec<ActivityEntry> = app
        .activity
        .lock()
        .unwrap()
        .entries()
        .iter()
        .cloned()
        .collect();
    let lines = activity_lines(&snapshot, height, app.activity_scroll);
    let title = if app.activity_scroll > 0 {
        format!(" activity ({} back)", app.activity_scroll)
    } else {
        " activity".to_string()
    };
    let lines = lines
        .into_iter()
        .map(|(kind, time, text)| {
            Line::from(vec![
                Span::styled(time, Style::new().fg(Color::DarkGray)),
                Span::styled(format!(" {text}"), Style::new().fg(activity_color(kind))),
            ])
        })
        .collect::<Vec<_>>();
    let paragraph = Paragraph::new(lines).block(Block::bordered().title(title));
    frame.render_widget(paragraph, area);
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
            "L activity  / filter  n new  e edit  d delete  r run now  l logs  ? help  q quit"
        }
        View::Editor(_) => "↑↓←→ move  enter edit/save  tab section  esc back  q quit",
        View::PresetPicker { .. } => "enter choose  esc back  q quit",
        View::Logs(state) if state.follow => "x stop  esc back  q quit",
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

#[test]
fn activity_lines_tail_anchor_and_scroll() {
    use crate::activity::{ActivityEntry, ActivityKind};

    let now = chrono::Local::now();
    let entries: Vec<ActivityEntry> = ["a", "b", "c", "d"]
        .iter()
        .map(|t| ActivityEntry {
            at: now,
            kind: ActivityKind::Action,
            text: t.to_string(),
        })
        .collect();
    let lines = activity_lines(&entries, 2, 0);
    assert_eq!(lines.len(), 2, "offset 0 returns the last height entries");
    assert_eq!(lines[0].2, "c");
    assert_eq!(lines[1].2, "d");
    let lines = activity_lines(&entries, 2, 1);
    assert_eq!(lines[0].2, "b");
    assert_eq!(lines[1].2, "c");
    assert_eq!(
        activity_lines(&entries, 10, 0).len(),
        4,
        "height greater than total returns all entries"
    );
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
                ("L", "toggle the activity panel"),
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
        "• midnight — missed runs catch up on next boot",
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "• linger — user timers survive logout",
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "• linger on — loginctl enable-linger",
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "• linger off — loginctl disable-linger",
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "• config — ~/.config/topmatic/config.toml",
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "• topgrade — isolated runs, no sudo",
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
                ("↑↓←→ or hjkl", "move in the current section"),
                ("enter / space", "act on the highlighted row"),
                ("/", "filter the steps list"),
            ],
        },
        KeyGroup {
            title: "Save & leave",
            keys: &[
                (
                    "save row → enter",
                    "type a name; enter saves and returns home",
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
    fn activity_prefix_styles_each_kind_with_trailing_space() {
        let entry = |kind| ActivityEntry {
            at: chrono::Local::now(),
            kind,
            text: "started all-daily".to_string(),
        };
        assert_eq!(
            activity_prefix(&entry(ActivityKind::Error)),
            Some((" started all-daily ".to_string(), Color::Red)),
            "errors render in red"
        );
        assert_eq!(
            activity_prefix(&entry(ActivityKind::Action)),
            Some((" started all-daily ".to_string(), Color::Yellow)),
            "actions render in yellow"
        );
        assert_eq!(
            activity_prefix(&entry(ActivityKind::Command)),
            Some((" started all-daily ".to_string(), Color::Magenta)),
            "commands render in magenta"
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
            "toggle the activity panel",
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
            "enter saves and returns home",
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
            text.contains("linger on — loginctl enable-linger"),
            "help must say how to turn linger on:\n{text}"
        );
        assert!(
            text.contains("linger off — loginctl disable-linger"),
            "help must say how to turn linger off:\n{text}"
        );
        assert!(
            text.contains("user timers survive logout"),
            "help must say what linger is for:\n{text}"
        );
    }

    #[test]
    fn help_notes_are_concise_topics_without_semicolons() {
        let text: String = dashboard_help()
            .iter()
            .flat_map(|line| line.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !text.contains(';'),
            "notes should not use semicolons:\n{text}"
        );
        let bullets: Vec<String> = dashboard_help()
            .iter()
            .map(|line| line.iter().map(|s| s.content.as_ref()).collect::<String>())
            .filter(|line| line.starts_with('•'))
            .collect();
        assert!(
            bullets.len() >= 3,
            "notes should read as concise topic bullets:\n{bullets:?}"
        );
        for bullet in &bullets {
            assert!(
                bullet.chars().count() <= 48,
                "a note bullet should be terse (<=48 cols): {bullet:?}"
            );
        }
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
        assert!(
            !footer_hints(
                &View::Editor(Box::new(EditorState::new(
                    None,
                    Vec::new(),
                    crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC
                )),),
                false
            )
            .contains("/ filter steps"),
            "the editor hint no longer advertises '/ filter'"
        );
        assert!(footer_hints(&View::Dashboard, false).contains("q quit"));
        assert!(
            footer_hints(&View::Logs(crate::tui::logs::LogsState::follow("x")), false)
                .contains("x stop")
        );
        assert!(footer_hints(&View::Dashboard, true).starts_with("filter"));
    }

    #[test]
    fn active_filter_reports_dashboard_and_editor_steps_queries() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let enter = || KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let (mut app, _tmp) = test_harness();
        assert_eq!(active_filter(&app), None, "no engaged filter shows nothing");

        app.filter.start();
        app.filter.edit.value = "ca".to_string();
        assert_eq!(
            active_filter(&app).as_deref(),
            Some("ca▏"),
            "the caret shows while typing"
        );
        app.filter.handle(enter());
        assert_eq!(
            active_filter(&app).as_deref(),
            Some("ca"),
            "a committed filter stays visible in the footer"
        );

        let state = EditorState::new(
            None,
            vec!["cargo".to_string()],
            crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
        );
        app.view = View::Editor(Box::new(state));
        assert_eq!(
            active_filter(&app),
            None,
            "the dashboard filter does not leak into the editor"
        );

        let View::Editor(boxed) = app.view else {
            unreachable!("the editor view is set above")
        };
        let mut state = *boxed;
        state.steps_filter.start();
        state.steps_filter.edit.value = "flat".to_string();
        app.view = View::Editor(Box::new(state));
        assert_eq!(active_filter(&app).as_deref(), Some("flat▏"));
        let View::Editor(mut boxed) = app.view else {
            unreachable!("the editor view is set above")
        };
        boxed.steps_filter.handle(enter());
        app.view = View::Editor(boxed);
        assert_eq!(
            active_filter(&app).as_deref(),
            Some("flat"),
            "a committed steps filter also surfaces in the footer"
        );
    }

    fn test_harness() -> (crate::tui::app::App, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let paths =
            crate::paths::Paths::with_bases(tmp.path().join("cfg"), tmp.path().join("state"));
        let ctl = crate::systemd::test_support::FakeCtl::new(tmp.path().join("units"));
        let app = crate::tui::app::App::assemble(
            crate::config::AppConfig::default(),
            Vec::new(),
            paths,
            Box::new(ctl),
            Box::new(|| {
                Box::new(crate::systemd::test_support::FakeCtl::new(
                    std::path::PathBuf::from("/nonexistent"),
                ))
            }),
            std::sync::Arc::new(std::sync::Mutex::new(
                crate::activity::ActivityLog::in_memory(4),
            )),
            crate::tui::app::ResolvedBins::new(std::path::PathBuf::from("/bin/topmatic"), None),
        );
        (app, tmp)
    }

    #[test]
    fn spinner_frames_rotate_with_elapsed_time() {
        let zero = spinner_frame(Duration::ZERO);
        let later = spinner_frame(Duration::from_secs(1));
        assert_ne!(zero, later, "spinner must animate over time");
        assert!(SPINNER_FRAMES.contains(&zero));
        assert!(SPINNER_FRAMES.contains(&later));
    }

    #[test]
    fn in_flight_prefix_shows_the_busy_spinner_and_label() {
        let job = crate::tui::app::BackgroundJob {
            label: "saving all-daily".to_string(),
            started: std::time::Instant::now(),
            shared: std::sync::Arc::new(std::sync::Mutex::new(None)),
        };
        let (text, color) = in_flight_prefix(&job).unwrap();
        assert!(text.contains("saving all-daily"), "got: {text:?}");
        assert!(
            SPINNER_FRAMES.contains(&text.trim().chars().next().unwrap()),
            "footer leads with a spinner frame: {text:?}"
        );
        assert_eq!(color, Color::Yellow);
    }
}
