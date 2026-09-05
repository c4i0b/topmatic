use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Row as TableRow, Table};

use crate::domain::schedule::{Schedule, SchedulePreset};
use crate::runner::RunOutcome;

use super::App;

#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub name: String,
    pub enabled: bool,
    pub schedule: String,
    pub next_run: Option<DateTime<Utc>>,
    pub status: Option<RunOutcome>,
}

pub fn schedule_summary(schedule: &Schedule) -> String {
    match &schedule.preset {
        SchedulePreset::Hourly => "hourly".to_string(),
        SchedulePreset::EveryNHours { hours } => format!("every {hours}h"),
        SchedulePreset::Daily { hour, minute } => format!("daily {hour:02}:{minute:02}"),
        SchedulePreset::Weekly {
            weekday,
            hour,
            minute,
        } => format!("weekly {} {hour:02}:{minute:02}", weekday.as_systemd()),
        SchedulePreset::Custom { calendar } => format!("custom: {calendar}"),
    }
}

fn status_span(status: &Option<RunOutcome>) -> Span<'static> {
    match status {
        None => Span::raw("never run"),
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

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let rows: Vec<TableRow> = app
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let selected = index == app.selected;
            let base_style = if selected {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            let cells = vec![
                Cell::from(row.name.clone()),
                Cell::from(if row.enabled { "yes" } else { "paused" }).style(if row.enabled {
                    Style::new().fg(Color::Green)
                } else {
                    Style::new().fg(Color::DarkGray)
                }),
                Cell::from(row.schedule.clone()),
                Cell::from(
                    row.next_run
                        .map(|next| next.format("%a %d %b %H:%M").to_string())
                        .unwrap_or_else(|| "-".to_string()),
                ),
                Cell::from(Line::from(status_span(&row.status))),
            ];
            TableRow::new(cells).style(base_style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            ratatui::layout::Constraint::Length(24),
            ratatui::layout::Constraint::Length(7),
            ratatui::layout::Constraint::Length(22),
            ratatui::layout::Constraint::Length(17),
            ratatui::layout::Constraint::Min(18),
        ],
    )
    .header(
        TableRow::new(vec![
            "Profile", "Enabled", "Schedule", "Next run", "Last run",
        ])
        .style(Style::new().add_modifier(Modifier::BOLD)),
    )
    .block(
        ratatui::widgets::Block::bordered().title("topmatic — user-level update jobs (topgrade)"),
    );

    frame.render_widget(table, area);
}
