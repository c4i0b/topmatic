use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::domain::profile::NotifyPolicy;
use crate::domain::schedule::{SchedulePreset, Weekday};

use super::super::focus_marker;
use super::{EditorState, ScheduleRow, Section};

impl EditorState {
    pub(crate) fn steps_header(&self) -> Line<'static> {
        let steps_total = self.filtered_steps().len();
        let display_first = if steps_total == 0 {
            0
        } else {
            self.steps_window().0 + 1
        };
        let filter_label = if self.steps_filter.active {
            format!("filter: {}▏", self.steps_filter.text())
        } else {
            "/ filter".to_string()
        };
        Line::from(vec![
            focus_marker(self.section == Section::Steps),
            Span::raw(format!(
                " steps (selected: {}) [{}-{}/{}] {filter_label}",
                self.selected_steps.len(),
                display_first,
                self.steps_window().1,
                steps_total,
            )),
        ])
    }

    pub(crate) fn body_lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![self.steps_header()];
        let (steps_start, steps_end) = self.steps_window();
        let focus_steps = self.section == Section::Steps;
        for (offset, id) in self.filtered_steps()[steps_start..steps_end]
            .iter()
            .enumerate()
        {
            let index = steps_start + offset;
            let marker = if self.selected_steps.contains(*id) {
                "[x]"
            } else {
                "[ ]"
            };
            lines.push(Line::from(format!(
                "{}{marker} {}",
                if focus_steps && index == self.list_index {
                    "▶ "
                } else {
                    "  "
                },
                id
            )));
        }

        lines.push(Line::from(""));
        let focus_schedule = self.section == Section::Schedule;
        lines.push(Line::from(vec![
            focus_marker(focus_schedule),
            Span::styled(
                format!("schedule: {}", self.schedule.summary()),
                Style::new().fg(Color::Cyan),
            ),
        ]));
        for (row_index, row) in self.schedule_rows().iter().enumerate() {
            let cursor = focus_schedule && row_index == self.schedule_index;
            lines.push(Line::from(format!(
                "{}{}",
                if cursor { "▶ " } else { "  " },
                self.schedule_row_text(*row)
            )));
        }
        lines.push(Line::from(Span::styled(
            " midnight anchored · missed runs catch up on next boot",
            Style::new().fg(Color::DarkGray),
        )));

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

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "─".repeat(20),
            Style::new().fg(Color::DarkGray),
        )));
        lines.push(Line::from(format!(
            "{}{}",
            if self.section == Section::Save {
                "▶ "
            } else {
                "  "
            },
            self.save_row_text()
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
            ScheduleRow::Custom => format!("custom OnCalendar: {}▏", self.custom.value),
        }
    }

    fn option_row_text(&self) -> String {
        format!("notify: {}", notify_label(self.notify))
    }

    fn save_row_text(&self) -> String {
        let name = if self.creating {
            self.suggested_name
                .clone()
                .unwrap_or_else(|| "new profile".to_string())
        } else {
            self.original_name.clone().unwrap_or_default()
        };
        format!("save \"{name}\"{}", if self.is_dirty() { " *" } else { "" })
    }
}

fn preset_label(preset: &SchedulePreset) -> String {
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
        editor.schedule = quick_choices(DEFAULT_RANDOM_DELAY_SEC)[1].1.clone();
        let text: Vec<String> = editor.body_lines().iter().map(line_text).collect();
        let joined = text.join("\n");

        let row_position = |prefix: &str| {
            text.iter()
                .position(|line| line.trim_start().starts_with(prefix))
                .unwrap_or_else(|| panic!("{prefix:?} missing from {text:?}"))
        };
        let preset = row_position("preset:");
        let weekday = row_position("weekday:");
        let save = row_position("save \"all-daily\"");
        assert!(preset < weekday && weekday < save);
        assert!(joined.contains("schedule: weekly Mon"));
        assert!(joined.contains("notify: on failure"));
        assert!(joined.contains("missed runs catch up on next boot"));
    }

    #[test]
    fn save_row_marks_dirty_state() {
        let mut editor = editing_editor();
        editor.notify = NotifyPolicy::Never;
        let lines = editor.body_lines();
        let last = lines.last().unwrap();
        assert!(line_text(last).contains("save \"all-daily\" *"));
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
            text, "▸ steps (selected: 162) [1-10/162] / filter",
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
}
