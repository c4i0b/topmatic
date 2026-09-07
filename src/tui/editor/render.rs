use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::domain::profile::NotifyPolicy;
use crate::domain::schedule::{SchedulePreset, Weekday};

use super::super::focus_marker;
use super::{EditorState, ScheduleRow, Section};

impl EditorState {
    pub(crate) fn steps_header(&self) -> Line<'static> {
        let mut label = format!("steps (selected: {})", self.selected_steps.len());
        if self.steps_filter.is_engaged() {
            label.push_str(&format!(
                " [{}/{}]",
                self.filtered_steps().len(),
                self.catalog.len()
            ));
            label.push_str(&format!(" filter: {}", self.steps_filter.filter_query()));
        }
        Line::from(vec![
            focus_marker(self.section == Section::Steps),
            Span::styled(label, Style::new().fg(Color::Cyan)),
        ])
    }

    pub(crate) fn body_lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![self.steps_header()];
        let (steps_start, steps_end) = self.steps_window();
        let focus_steps = self.section == Section::Steps;
        let columns = self.steps_columns.max(1);
        let rows = (steps_end - steps_start).div_ceil(columns);
        let grid: Vec<Vec<Option<String>>> = (0..rows)
            .map(|row| {
                (0..columns)
                    .map(|column| {
                        let index = steps_start + row * columns + column;
                        if index >= steps_end {
                            return None;
                        }
                        self.filtered_steps().get(index).map(|id| {
                            let marker = if self.selected_steps.contains(*id) {
                                "[x]"
                            } else {
                                "[ ]"
                            };
                            let cursor = focus_steps && index == self.list_index;
                            format!("{}{marker} {id}", if cursor { "▶ " } else { "  " })
                        })
                    })
                    .collect()
            })
            .collect();
        let column_widths: Vec<usize> = (0..columns)
            .map(|column| {
                grid.iter()
                    .filter_map(|row| row.get(column))
                    .filter_map(|cell| cell.as_deref())
                    .map(|cell: &str| cell.chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        for (row_index, row) in grid.iter().enumerate() {
            let mut with_gaps: Vec<Span> = Vec::with_capacity(row.len() * 2);
            for (column, cell) in row.iter().enumerate() {
                if let Some(cell) = cell {
                    if column > 0 {
                        with_gaps.push(Span::raw("  "));
                    }
                    let padding = column_widths[column].saturating_sub(cell.chars().count());
                    with_gaps.push(Span::raw(format!("{cell}{}", " ".repeat(padding))));
                }
            }
            let focused_row = focus_steps
                && (steps_start + row_index * columns..steps_start + (row_index + 1) * columns)
                    .contains(&self.list_index);
            let line = Line::from(with_gaps);
            if focused_row {
                lines.push(line.style(Style::new().bg(Color::DarkGray)));
            } else {
                lines.push(line);
            }
        }

        lines.push(Line::from(""));
        let focus_schedule = self.section == Section::Schedule;
        lines.push(Line::from(vec![
            focus_marker(focus_schedule),
            Span::styled("schedule", Style::new().fg(Color::Cyan)),
        ]));
        for (row_index, row) in self.schedule_rows().iter().enumerate() {
            let cursor = focus_schedule && row_index == self.schedule_index;
            lines.push(Line::from(format!(
                "{}{}",
                if cursor { "▶ " } else { "  " },
                self.schedule_row_text(*row)
            )));
        }

        lines.push(Line::from(""));
        let focus_options = self.section == Section::Options;
        lines.push(Line::from(vec![
            focus_marker(focus_options),
            Span::styled("options", Style::new().fg(Color::Cyan)),
        ]));
        lines.push(Line::from(format!(
            "{}{}",
            if focus_options { "▶ " } else { "  " },
            self.option_row_text()
        )));

        lines
    }

    fn schedule_row_text(&self, row: ScheduleRow) -> String {
        match row {
            ScheduleRow::Preset => format!("preset: {}", preset_label(&self.schedule.preset)),
            ScheduleRow::Weekday => {
                let weekday = if let SchedulePreset::Weekly { weekday, .. } = &self.schedule.preset
                {
                    weekday.as_systemd()
                } else {
                    Weekday::Mon.as_systemd()
                };
                format!("weekday: {weekday}")
            }
        }
    }

    fn option_row_text(&self) -> String {
        format!("notify: {}", notify_label(self.notify))
    }
}

pub(crate) fn preset_label(preset: &SchedulePreset) -> String {
    match preset {
        SchedulePreset::EveryNHours { hours } => format!("every {hours}h"),
        SchedulePreset::Daily { hour: 0, minute: 0 } => "daily".to_string(),
        SchedulePreset::Daily { hour, minute } => format!("daily {hour:02}:{minute:02}"),
        SchedulePreset::Weekly {
            weekday,
            hour: 0,
            minute: 0,
        } => format!("weekly {}", weekday.as_systemd()),
        SchedulePreset::Weekly {
            weekday,
            hour,
            minute,
        } => format!("weekly {} {hour:02}:{minute:02}", weekday.as_systemd()),
        SchedulePreset::Biweekly => "every 2 weeks".to_string(),
        SchedulePreset::Monthly => "monthly".to_string(),
        SchedulePreset::Custom { calendar } => format!("custom: {calendar}"),
        SchedulePreset::LegacySpread { .. } => "daily".to_string(),
    }
}

pub(crate) fn notify_label(policy: NotifyPolicy) -> &'static str {
    match policy {
        NotifyPolicy::Always => "always",
        NotifyPolicy::OnFailure => "on failure",
        NotifyPolicy::Never => "never",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::{NotifyPolicy, Profile, Scope};
    use crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC;
    use crate::domain::schedule::Schedule;
    use crate::domain::schedule::quick_choices;

    fn editing_editor() -> EditorState {
        let profile = Profile {
            name: "all-daily".to_string(),
            steps: vec!["flatpak".to_string()],
            schedule: Schedule::default(),
            notify: NotifyPolicy::OnFailure,
            scope: Scope::User,
        };
        EditorState::new(
            Some(&profile),
            vec!["flatpak".to_string()],
            crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
        )
    }

    fn line_text(line: &Line<'_>) -> String {
        line.iter().map(|span| span.content.as_ref()).collect()
    }

    #[test]
    fn body_lines_render_sections_rows_and_save_row_in_order() {
        let mut editor = editing_editor();
        editor.schedule = quick_choices(DEFAULT_RANDOM_DELAY_SEC)[3].1.clone();
        let text: Vec<String> = editor.body_lines().iter().map(line_text).collect();
        let joined = text.join("\n");

        let row_position = |prefix: &str| {
            text.iter()
                .position(|line| line.trim_start().starts_with(prefix))
                .unwrap_or_else(|| panic!("{prefix:?} missing from {text:?}"))
        };
        let preset = row_position("preset:");
        let weekday = row_position("weekday:");
        assert!(preset < weekday);
        assert!(
            !joined.contains("all-daily"),
            "the name is confirmed in its own popup, not the save row"
        );
        let schedule_header = row_position("schedule");
        assert!(
            text[schedule_header].trim() == "schedule",
            "the schedule header carries no selected option: {:?}",
            text[schedule_header]
        );
        assert!(joined.contains("preset: weekly Mon"));
        assert!(joined.contains("notify: on failure"));
        assert!(
            !joined.contains("midnight anchored"),
            "the midnight hint is gone from the editor body"
        );
    }

    #[test]
    fn editor_tape_golden_steps_header_is_exactly_the_documented_line() {
        let catalog: Vec<String> = (0..162).map(|i| format!("step_{i}")).collect();
        let editor = EditorState::from_preset(
            catalog.clone(),
            catalog,
            "all-daily",
            crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
        );

        let text: String = editor
            .steps_header()
            .iter()
            .map(|span| span.content.clone())
            .collect();
        assert_eq!(
            text, "▸steps (selected: 162)",
            "the screenshot steps line renders exactly this text, nothing more"
        );
    }

    #[test]
    fn editor_tape_golden_steps_header_never_splits_a_token() {
        let catalog: Vec<String> = (0..162).map(|i| format!("step_{i}")).collect();
        let editor = EditorState::from_preset(
            catalog.clone(),
            catalog,
            "all-daily",
            crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
        );
        let golden = editor
            .steps_header()
            .iter()
            .map(|span| span.content.clone())
            .collect::<String>();

        use ratatui::{
            Terminal,
            backend::TestBackend,
            widgets::{Block, Paragraph, Wrap},
        };
        let assert_no_split = |width: u16| {
            let mut terminal = Terminal::new(TestBackend::new(width, 8)).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    let paragraph = Paragraph::new(editor.steps_header())
                        .block(Block::bordered().title("edit profile"))
                        .wrap(Wrap { trim: true });
                    frame.render_widget(paragraph, area);
                })
                .unwrap();
            let rows = terminal
                .backend()
                .buffer()
                .content()
                .chunks(width as usize)
                .map(|cells| cells.iter().map(|c| c.symbol()).collect::<String>())
                .collect::<Vec<_>>();
            for word in golden.split_ascii_whitespace() {
                assert!(
                    rows.iter().any(|row| row.contains(word)),
                    "word {word:?} is split across rows by the editor steps header:\n{}",
                    rows.join("\n")
                );
            }
            assert!(
                !rows.join("\n").contains("ignoring"),
                "no stray garbage in the steps header render"
            );
        };
        assert_no_split(40);
        assert_no_split(119);
    }

    #[test]
    fn steps_header_shows_the_filter_query_only_while_filtering() {
        let mut editor = editing_editor();
        let idle = line_text(&editor.steps_header());
        assert!(
            !idle.contains("filter"),
            "no filter suffix when not filtering: {idle:?}"
        );
        editor.steps_filter.start();
        editor.steps_filter.edit.value = "flat".to_string();
        let typing = line_text(&editor.steps_header());
        assert!(
            typing.contains("filter: flat▏"),
            "the engaged filter shows its query with a caret while typing: {typing:?}"
        );
        editor.steps_filter.handle(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        let committed = line_text(&editor.steps_header());
        assert!(
            committed.contains("filter: flat") && !committed.contains('▏'),
            "a committed filter stays visible without the caret: {committed:?}"
        );
    }

    #[test]
    fn every_step_renders_when_columns_fit_the_height() {
        let catalog: Vec<String> = (0..20).map(|i| format!("step_{i}")).collect();
        let mut editor = EditorState::from_preset(
            catalog.clone(),
            catalog,
            "all-daily",
            crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
        );
        editor.set_steps_columns(4);
        let text: String = editor
            .body_lines()
            .iter()
            .map(line_text)
            .collect::<Vec<_>>()
            .join("\n");
        for step in 0..20 {
            assert!(
                text.contains(&format!("step_{step}")),
                "step_{step} must be visible without paging"
            );
        }
    }

    #[test]
    fn multi_column_steps_lay_out_cells_across_the_row() {
        let catalog: Vec<String> = (0..6).map(|i| format!("step_{i}")).collect();
        let mut editor = EditorState::from_preset(
            catalog.clone(),
            catalog,
            "all-daily",
            crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
        );
        editor.set_steps_columns(3);
        let lines: Vec<String> = editor.body_lines().iter().map(line_text).collect();
        let row = lines
            .iter()
            .find(|line| line.contains("step_0"))
            .expect("first grid row renders");
        let pos = |name: &str| {
            row.find(name)
                .unwrap_or_else(|| panic!("{name:?} missing from row: {row:?}"))
        };
        assert!(pos("step_0") < pos("step_1") && pos("step_1") < pos("step_2"));
        let second = lines
            .iter()
            .find(|line| line.contains("step_3"))
            .expect("second grid row renders");
        assert!(second.contains("step_4") && second.contains("step_5"));
        assert!(
            !row.contains("step_3"),
            "cells wrap onto the next rendered row, not across the first"
        );
    }

    #[test]
    fn multi_column_grid_aligns_every_column_across_rows() {
        let catalog = vec![
            "a".to_string(),
            "very-long-step-name".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "medium-step".to_string(),
        ];
        let mut editor = EditorState::from_preset(
            catalog.clone(),
            catalog,
            "all-daily",
            crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
        );
        editor.set_steps_columns(3);
        let lines: Vec<String> = editor.body_lines().iter().map(line_text).collect();
        let first = lines
            .iter()
            .find(|line| line.contains("very-long-step-name"))
            .expect("first grid row renders");
        let second = lines
            .iter()
            .find(|line| line.contains("medium-step"))
            .expect("second grid row renders");

        let columns_of = |line: &str, names: &[&str]| -> Vec<usize> {
            names
                .iter()
                .map(|name| {
                    line.find(&format!("] {name}"))
                        .unwrap_or_else(|| panic!("{name:?} missing from row: {line:?}"))
                })
                .collect()
        };
        let first_offsets = columns_of(first, &["a", "very-long-step-name", "b"]);
        let second_offsets = columns_of(second, &["c", "d", "medium-step"]);

        assert_eq!(
            first_offsets[1] - first_offsets[0],
            second_offsets[1] - second_offsets[0],
            "column 2 starts at the same offset on every row"
        );
        assert_eq!(
            first_offsets[2] - first_offsets[1],
            second_offsets[2] - second_offsets[1],
            "column 3 starts at the same offset on every row"
        );
    }

    #[test]
    fn focused_grid_row_gets_a_full_width_background() {
        let catalog: Vec<String> = (0..9).map(|i| format!("step_{i}")).collect();
        let mut editor = EditorState::from_preset(
            catalog.clone(),
            catalog,
            "all-daily",
            crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
        );
        editor.set_steps_columns(3);
        editor.section = Section::Steps;
        editor.handle_key(crossterm::event::KeyCode::Down.into());
        let lines = editor.body_lines();
        let focused = lines
            .iter()
            .find(|line| line.iter().any(|span| span.content.contains("step_3")))
            .expect("cursor row renders");
        assert_eq!(
            focused.style,
            Style::new().bg(Color::DarkGray),
            "the whole cursor row is highlighted (fzf --highlight-line pattern)"
        );
        let other = lines
            .iter()
            .find(|line| line.iter().any(|span| span.content.contains("step_0")))
            .expect("first row renders");
        assert_ne!(other.style, Style::new().bg(Color::DarkGray));
    }

    #[test]
    fn multi_column_grid_keeps_the_cursor_marker_on_the_focused_cell() {
        let catalog: Vec<String> = (0..6).map(|i| format!("step_{i}")).collect();
        let mut editor = EditorState::from_preset(
            catalog.clone(),
            catalog,
            "all-daily",
            crate::domain::schedule::DEFAULT_RANDOM_DELAY_SEC,
        );
        editor.set_steps_columns(3);
        editor.section = Section::Steps;
        editor.handle_key(crossterm::event::KeyCode::Right.into());
        let lines: Vec<String> = editor.body_lines().iter().map(line_text).collect();
        let row = lines
            .iter()
            .find(|line| line.contains("step_1"))
            .expect("focused row renders");
        assert!(
            row.contains("▶ [x] step_1"),
            "the cursor marker lands on the focused cell: {row:?}"
        );
    }
}
