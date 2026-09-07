use ratatui::style::{Color, Style};
use ratatui::text::Span;

pub const KEY_STYLE: Style = Style::new()
    .fg(Color::Yellow)
    .add_modifier(ratatui::style::Modifier::BOLD);
pub const VERB_STYLE: Style = Style::new().fg(Color::DarkGray);

pub fn legend_spans(table: &[Binding], in_steps: bool) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for binding in table.iter().filter(|b| {
        b.footer
            && match b.scope {
                Scope::All => true,
                Scope::StepsOnly => in_steps,
                Scope::RowsOnly => !in_steps,
            }
    }) {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(binding.key, KEY_STYLE));
        if !binding.verb.is_empty() {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(binding.verb, VERB_STYLE));
        }
    }
    spans
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    All,
    StepsOnly,
    RowsOnly,
}

pub struct Binding {
    pub key: &'static str,
    pub verb: &'static str,
    pub help_key: &'static str,
    pub desc: &'static str,
    pub group: &'static str,
    pub footer: bool,
    pub scope: Scope,
}

const MOVE_AND_EDIT: &str = "Move & edit";
const SAVE_AND_LEAVE: &str = "Save & leave";
const PROFILES: &str = "Profiles";
const GLOBAL: &str = "Global";

pub const DASHBOARD: &[Binding] = &[
    Binding {
        key: "↑↓",
        verb: "move",
        help_key: "↑/↓ or j/k",
        desc: "move the profile selection",
        group: PROFILES,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "n",
        verb: "new",
        help_key: "n",
        desc: "open the profile picker",
        group: PROFILES,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "e",
        verb: "edit",
        help_key: "enter / e",
        desc: "edit the selected profile",
        group: PROFILES,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "d",
        verb: "delete",
        help_key: "d",
        desc: "delete the profile · always asks first",
        group: PROFILES,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "r",
        verb: "run now",
        help_key: "r",
        desc: "run now, opens the live view",
        group: PROFILES,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "l",
        verb: "logs",
        help_key: "l",
        desc: "browse run logs",
        group: PROFILES,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "/",
        verb: "filter",
        help_key: "/",
        desc: "filter profiles by name",
        group: PROFILES,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "L",
        verb: "activity",
        help_key: "L",
        desc: "toggle the activity panel",
        group: PROFILES,
        footer: false,
        scope: Scope::All,
    },
    Binding {
        key: "?",
        verb: "help",
        help_key: "? / esc",
        desc: "close this help",
        group: GLOBAL,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        footer: true,
        scope: Scope::All,
    },
];

pub const EDITOR: &[Binding] = &[
    Binding {
        key: "↑↓←→",
        verb: "move",
        help_key: "↑↓←→ or hjkl",
        desc: "move in the steps grid",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::StepsOnly,
    },
    Binding {
        key: "↑↓",
        verb: "move",
        help_key: "↑/↓ or j/k",
        desc: "move in the schedule and options rows",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::RowsOnly,
    },
    Binding {
        key: "space/enter",
        verb: "toggle",
        help_key: "space / enter",
        desc: "toggle steps · choose the highlighted row",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::StepsOnly,
    },
    Binding {
        key: "ctrl+a",
        verb: "all",
        help_key: "ctrl+a / ctrl+d",
        desc: "mark / clear every filtered step",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::StepsOnly,
    },
    Binding {
        key: "ctrl+d",
        verb: "none",
        help_key: "",
        desc: "",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::StepsOnly,
    },
    Binding {
        key: "enter",
        verb: "choose",
        help_key: "",
        desc: "",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::RowsOnly,
    },
    Binding {
        key: "/",
        verb: "filter",
        help_key: "/",
        desc: "filter the steps list",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::StepsOnly,
    },
    Binding {
        key: "tab",
        verb: "switch section",
        help_key: "tab / shift-tab",
        desc: "switch section",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "ctrl+s",
        verb: "save",
        help_key: "ctrl+s",
        desc: "save from anywhere in the editor",
        group: SAVE_AND_LEAVE,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "esc",
        verb: "back",
        help_key: "esc",
        desc: "save or leave · asks when unsaved",
        group: SAVE_AND_LEAVE,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: SAVE_AND_LEAVE,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "?",
        verb: "help",
        help_key: "?",
        desc: "show this help",
        group: SAVE_AND_LEAVE,
        footer: false,
        scope: Scope::All,
    },
];

pub const PRESET_PICKER: &[Binding] = &[
    Binding {
        key: "enter",
        verb: "choose",
        help_key: "enter",
        desc: "start this preset",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "esc/l",
        verb: "back",
        help_key: "esc / l / h",
        desc: "back to the dashboard",
        group: GLOBAL,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        footer: true,
        scope: Scope::All,
    },
];

pub const LOGS_FOLLOW: &[Binding] = &[
    Binding {
        key: "x",
        verb: "stop",
        help_key: "x",
        desc: "stop the running profile",
        group: PROFILES,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "↑↓",
        verb: "scroll",
        help_key: "↑↓ / pgup/pgdn / home/end",
        desc: "scroll the live log (auto-follows at the bottom)",
        group: MOVE_AND_EDIT,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "esc/l",
        verb: "back",
        help_key: "esc / l / h",
        desc: "back to the dashboard",
        group: GLOBAL,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        footer: true,
        scope: Scope::All,
    },
];

pub const LOGS_BROWSE: &[Binding] = &[
    Binding {
        key: "↑↓",
        verb: "move",
        help_key: "↑/↓ or j/k",
        desc: "move between runs and scroll an open log",
        group: PROFILES,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "enter",
        verb: "open",
        help_key: "enter",
        desc: "open the highlighted run",
        group: PROFILES,
        footer: false,
        scope: Scope::All,
    },
    Binding {
        key: "esc/l",
        verb: "back",
        help_key: "esc / l / enter / h",
        desc: "back to the dashboard; enter closes an open run",
        group: GLOBAL,
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        footer: true,
        scope: Scope::All,
    },
];

pub const FILTER_TYPING: &[Binding] = &[
    Binding {
        key: "type…",
        verb: "",
        help_key: "",
        desc: "",
        group: "",
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "Enter",
        verb: "accept",
        help_key: "",
        desc: "",
        group: "",
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "Esc",
        verb: "clear",
        help_key: "",
        desc: "",
        group: "",
        footer: true,
        scope: Scope::All,
    },
    Binding {
        key: "↑↓",
        verb: "move",
        help_key: "",
        desc: "",
        group: "",
        footer: true,
        scope: Scope::All,
    },
];

#[cfg(test)]
pub fn footer_tokens(table: &[Binding], in_steps: bool) -> String {
    table
        .iter()
        .filter(|b| {
            b.footer
                && match b.scope {
                    Scope::All => true,
                    Scope::StepsOnly => in_steps,
                    Scope::RowsOnly => !in_steps,
                }
        })
        .map(|b| {
            if b.verb.is_empty() {
                b.key.to_string()
            } else {
                format!("{} {}", b.key, b.verb)
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

pub fn help_groups(table: &[Binding]) -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    let mut groups: Vec<(&'static str, Vec<(&'static str, &'static str)>)> = Vec::new();
    for binding in table.iter().filter(|b| !b.help_key.is_empty()) {
        let entry = (binding.help_key, binding.desc);
        match groups.iter_mut().find(|(title, _)| title == &binding.group) {
            Some((_, rows)) => {
                if !rows.contains(&entry) {
                    rows.push(entry);
                }
            }
            None => groups.push((binding.group, vec![entry])),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_lines_match_the_documented_legends() {
        assert_eq!(
            footer_tokens(DASHBOARD, false),
            "↑↓ move  n new  e edit  d delete  r run now  l logs  / filter  ? help  q quit"
        );
        assert_eq!(
            footer_tokens(EDITOR, true),
            "↑↓←→ move  space/enter toggle  ctrl+a all  ctrl+d none  / filter  tab switch section  ctrl+s save  esc back  q quit"
        );
        assert_eq!(
            footer_tokens(EDITOR, false),
            "↑↓ move  enter choose  tab switch section  ctrl+s save  esc back  q quit"
        );
        assert_eq!(
            footer_tokens(PRESET_PICKER, false),
            "enter choose  esc/l back  q quit"
        );
        assert_eq!(
            footer_tokens(LOGS_FOLLOW, false),
            "x stop  ↑↓ scroll  esc/l back  q quit"
        );
        assert_eq!(
            footer_tokens(LOGS_BROWSE, false),
            "↑↓ move  esc/l back  q quit"
        );
    }

    #[test]
    fn footer_legends_fit_eighty_columns() {
        for table in [DASHBOARD, EDITOR, PRESET_PICKER, LOGS_FOLLOW, LOGS_BROWSE] {
            let legend = footer_tokens(table, false);
            assert!(
                legend.chars().count() <= 105,
                "legend overflows the common terminal width: {legend}"
            );
        }
    }

    #[test]
    fn legend_spans_highlight_keys_and_dim_verbs() {
        let spans = legend_spans(PRESET_PICKER, false);
        assert_eq!(
            spans.len(),
            11,
            "3 tokens as key/space/verb with two-space gaps"
        );
        assert_eq!(spans[0].content, "enter");
        assert_eq!(spans[0].style, KEY_STYLE);
        assert_eq!(spans[2].content, "choose");
        assert_eq!(spans[2].style, VERB_STYLE);
        assert_eq!(spans[3].content, "  ");
        assert_eq!(spans[7].content, "  ");
        let text: String = spans.iter().map(|s| s.content.clone()).collect();
        assert_eq!(text, "enter choose  esc/l back  q quit");
    }

    #[test]
    fn readme_points_at_the_in_app_help() {
        let readme = include_str!("../../README.md");
        assert!(
            readme.contains("footer") && readme.contains("`?`"),
            "the README delegates key documentation to the in-app footer and help overlay"
        );
    }

    #[test]
    fn help_groups_keep_first_appearance_order_without_duplicates() {
        let groups = help_groups(EDITOR);
        assert_eq!(groups.first().map(|(title, _)| *title), Some(MOVE_AND_EDIT));
        let rows: Vec<&(&'static str, &'static str)> =
            groups.iter().flat_map(|(_, rows)| rows.iter()).collect();
        let unique: std::collections::HashSet<&&(&'static str, &'static str)> =
            rows.iter().collect();
        assert_eq!(rows.len(), unique.len());
        assert!(rows.contains(&&("ctrl+s", "save from anywhere in the editor")));
    }

    #[test]
    fn every_footer_action_also_exists_in_the_help() {
        for table in [DASHBOARD, EDITOR, PRESET_PICKER, LOGS_FOLLOW, LOGS_BROWSE] {
            let groups = help_groups(table);
            let rows: Vec<&(&'static str, &'static str)> =
                groups.iter().flat_map(|(_, rows)| rows.iter()).collect();
            for binding in table.iter().filter(|b| b.footer) {
                let covered = if binding.help_key.is_empty() {
                    rows.iter()
                        .any(|(key, _)| key.contains(binding.key) || binding.key.contains(key))
                } else {
                    rows.iter().any(|(key, _)| *key == binding.help_key)
                };
                assert!(
                    covered,
                    "footer action {} ({}) is missing from the help of this surface",
                    binding.key, binding.verb
                );
            }
        }
    }
}
