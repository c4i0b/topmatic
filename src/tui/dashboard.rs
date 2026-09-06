use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use crate::domain::profile::Profile;
use crate::runner::RunOutcome;

use super::App;

#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub name: String,
    pub schedule: String,
    pub next_run: Option<DateTime<Utc>>,
    pub status: Option<RunOutcome>,
    pub timer_active: bool,
}

fn state_marker(row: &ProfileRow) -> Span<'static> {
    if row.timer_active {
        Span::styled("•".to_string(), Style::new().fg(Color::Green))
    } else {
        Span::styled("•".to_string(), Style::new().fg(Color::Yellow))
    }
}

fn status_span(status: &Option<RunOutcome>) -> Span<'static> {
    match status {
        None => Span::raw("never run".to_string()),
        Some(outcome) if outcome.skipped => Span::styled(
            "skipped (in progress)".to_string(),
            Style::new().fg(Color::Yellow),
        ),
        Some(outcome) if outcome.success => Span::styled(
            outcome.finished_at.format("ok %d %b %H:%M").to_string(),
            Style::new().fg(Color::Green),
        ),
        Some(outcome) => Span::styled(
            outcome.finished_at.format("FAILED %d %b %H:%M").to_string(),
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    }
}

fn countdown(next: DateTime<Utc>) -> String {
    let now = Utc::now();
    if next <= now {
        return "due now".to_string();
    }
    let seconds = (next - now).num_seconds();
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (hours, minutes) = (rest / 3_600, (rest % 3_600) / 60);
    if days > 0 {
        format!("in {days}d {hours}h")
    } else if hours > 0 {
        format!("in {hours}h {minutes:02}m")
    } else {
        format!("in {minutes}m")
    }
}

fn detail_lines<'a>(profile: &Profile, row: &ProfileRow, log_tail: Option<&str>) -> Vec<Line<'a>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                profile.name.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            state_marker(row),
            Span::raw(if row.timer_active {
                " timer active"
            } else {
                " timer not active"
            }),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("schedule   ", Style::new().fg(Color::Cyan)),
            Span::raw(row.schedule.clone()),
        ]),
        Line::from(vec![
            Span::styled("next fire  ", Style::new().fg(Color::Cyan)),
            Span::raw(
                row.next_run
                    .map(|next| {
                        format!(
                            "{} ({})",
                            next.with_timezone(&chrono::Local).format("%a %d %b %H:%M"),
                            countdown(next)
                        )
                    })
                    .unwrap_or_else(|| "—".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("last run   ", Style::new().fg(Color::Cyan)),
            status_span(&row.status),
            Span::raw(
                row.status
                    .as_ref()
                    .map(|outcome| format!(" · {:.1}s", outcome.duration_secs))
                    .unwrap_or_default(),
            ),
        ]),
        Line::from(vec![
            Span::styled("steps      ", Style::new().fg(Color::Cyan)),
            Span::styled(step_preview(&profile.steps), Style::new().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("options    ", Style::new().fg(Color::Cyan)),
            Span::raw(format!("notify {}", notify_label(profile.notify),)),
        ]),
    ];
    if let Some(tail) = log_tail {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "─ last log ".to_string(),
            Style::new().fg(Color::DarkGray),
        )));
        for line in tail.lines() {
            lines.push(Line::from(line.to_string()));
        }
    }
    lines
}

fn notify_label(policy: crate::domain::profile::NotifyPolicy) -> &'static str {
    match policy {
        crate::domain::profile::NotifyPolicy::Always => "always",
        crate::domain::profile::NotifyPolicy::OnFailure => "on failure",
        crate::domain::profile::NotifyPolicy::Never => "never",
    }
}

const STEPS_PREVIEW: usize = 6;

fn step_preview(steps: &[String]) -> String {
    if steps.len() <= STEPS_PREVIEW {
        return steps.join(" ");
    }
    let shown: Vec<&str> = steps[..STEPS_PREVIEW].iter().map(String::as_str).collect();
    let extra = steps.len() - STEPS_PREVIEW;
    format!("{} … and {extra} more", shown.join(" "))
}

pub struct PaneAreas {
    pub list: Rect,
}

pub fn render(app: &App, frame: &mut Frame, area: Rect) -> PaneAreas {
    let [list_area, detail_area] = ratatui::layout::Layout::horizontal([
        ratatui::layout::Constraint::Length(28),
        ratatui::layout::Constraint::Min(40),
    ])
    .areas(area);

    let start = app.list_scroll as usize;
    let items: Vec<ListItem> = app
        .visible_rows()
        .iter()
        .skip(start)
        .take(super::LIST_VISIBLE)
        .enumerate()
        .map(|(offset, row)| {
            let index = start + offset;
            let line = if index == app.selected {
                Line::styled(
                    format!("▶ {}", row.name),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )
            } else {
                Line::from(format!("  {}", row.name))
            };
            ListItem::new(line)
        })
        .collect();

    let filter_title = if app.filtering() {
        format!("Profiles (filter: {})", app.filter_text())
    } else {
        "Profiles".to_string()
    };
    let list = List::new(items).block(Block::bordered().title(filter_title));
    frame.render_widget(list, list_area);

    let detail = match app.selected_profile() {
        Some((profile, row)) => {
            let tail = crate::tui::logs::tail(&app.paths, &row.name, 12);
            detail_lines(profile, row, tail.as_deref())
        }
        None => vec![Line::from("no profile selected")],
    };
    let paragraph = Paragraph::new(detail)
        .block(Block::bordered().title("detail"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(paragraph, detail_area);

    PaneAreas { list: list_area }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::Profile;

    fn profile_with_steps(steps: usize) -> Profile {
        Profile {
            name: "all-daily".to_string(),
            steps: (0..steps).map(|i| format!("step_{i}")).collect(),
            schedule: crate::domain::schedule::Schedule::default(),
            notify: crate::domain::profile::NotifyPolicy::OnFailure,
            scope: crate::domain::profile::Scope::User,
        }
    }

    fn detail_text(detail: &[Line<'_>]) -> Vec<String> {
        detail
            .iter()
            .map(|line| {
                line.iter()
                    .map(|span| span.content.clone())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn steps_preview_is_bounded_for_wide_lists() {
        let profile = profile_with_steps(162);
        assert_eq!(
            step_preview(&profile.steps),
            "step_0 step_1 step_2 step_3 step_4 step_5 … and 156 more"
        );
    }

    #[test]
    fn steps_preview_shows_everything_when_short() {
        let profile = profile_with_steps(3);
        assert_eq!(step_preview(&profile.steps), "step_0 step_1 step_2");
    }

    #[test]
    fn detail_lines_never_split_step_summary_mid_word() {
        let profile = profile_with_steps(162);
        let row = ProfileRow {
            name: "all-daily".to_string(),
            schedule: "daily 00:00".to_string(),
            next_run: None,
            status: None,
            timer_active: true,
        };
        let lines = detail_text(&detail_lines(&profile, &row, None));
        let steps = lines
            .iter()
            .find(|line| line.starts_with("steps      "))
            .expect("steps line present");
        assert_eq!(
            steps.trim_start().trim_start_matches("steps      "),
            "step_0 step_1 step_2 step_3 step_4 step_5 … and 156 more",
            "long profiles render a single clean preview, not a mid-word wrap"
        );
        let label = steps
            .trim_start()
            .replace("step_0 step_1 step_2 step_3 step_4 step_5 ", "");
        assert!(
            label.contains("… and ") && label.ends_with("more"),
            "truncation marker anchors at a word boundary"
        );
    }

    #[test]
    fn every_detail_line_fits_the_narrow_detail_pane() {
        let profile = profile_with_steps(162);
        let row = ProfileRow {
            name: "all-daily".to_string(),
            schedule: "daily 00:00".to_string(),
            next_run: None,
            status: None,
            timer_active: true,
        };
        let pane_width = 80;
        for line in detail_text(&detail_lines(&profile, &row, None)) {
            assert!(
                line.chars().count() <= pane_width,
                "detail line overflows the pane: {line:?}"
            );
        }
    }

    #[test]
    fn dashboard_tape_golden_detail_pane_never_splits_a_token() {
        let profile = Profile {
            name: "all-daily".to_string(),
            steps: vec![
                "am".into(),
                "android_studio".into(),
                "antigravity".into(),
                "app_man".into(),
                "aqua".into(),
                "asdf".into(),
                "atom".into(),
                "atuin".into(),
            ],
            schedule: crate::domain::schedule::Schedule::default(),
            notify: crate::domain::profile::NotifyPolicy::OnFailure,
            scope: crate::domain::profile::Scope::User,
        };
        let row = ProfileRow {
            name: "all-daily".to_string(),
            schedule: "daily 00:00".to_string(),
            next_run: None,
            status: None,
            timer_active: true,
        };

        use ratatui::{Terminal, backend::TestBackend, widgets::Wrap};
        let mut terminal = Terminal::new(TestBackend::new(40, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                let paragraph = Paragraph::new(detail_lines(&profile, &row, None))
                    .block(Block::bordered().title("detail"))
                    .wrap(Wrap { trim: true });
                frame.render_widget(paragraph, area);
            })
            .unwrap();

        let rows = terminal
            .backend()
            .buffer()
            .content()
            .chunks(40)
            .map(|cells| cells.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>();

        let visible = rows.join("\n");
        for token in [
            "timer active",
            "daily 00:00",
            "am",
            "android_studio",
            "antigravity",
            "app_man",
            "aqua",
            "asdf",
        ] {
            assert!(
                rows.iter().any(|row| row.contains(token)),
                "token {token:?} is split across rows by the dashboard.tape wrap\n{visible}"
            );
        }
        let marker = rows
            .iter()
            .position(|row| row.contains("… and"))
            .unwrap_or(0);
        assert!(
            rows[marker..].iter().any(|row| row.contains("more")),
            "truncation suffix '… and N more' is truncated by the dashboard.tape wrap\n{visible}"
        );
    }
}
