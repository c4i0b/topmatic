pub struct Binding {
    pub key: &'static str,
    pub verb: &'static str,
    pub help_key: &'static str,
    pub desc: &'static str,
    pub group: &'static str,
    pub footer: bool,
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
        desc: "move selection",
        group: PROFILES,
        footer: true,
    },
    Binding {
        key: "n",
        verb: "new",
        help_key: "n",
        desc: "new profile (from presets)",
        group: PROFILES,
        footer: true,
    },
    Binding {
        key: "e",
        verb: "edit",
        help_key: "enter / e",
        desc: "edit the selected profile",
        group: PROFILES,
        footer: true,
    },
    Binding {
        key: "d",
        verb: "delete",
        help_key: "d",
        desc: "delete profile — always asks first",
        group: PROFILES,
        footer: true,
    },
    Binding {
        key: "r",
        verb: "run now",
        help_key: "r",
        desc: "run now, opens the live view",
        group: PROFILES,
        footer: true,
    },
    Binding {
        key: "l",
        verb: "logs",
        help_key: "l",
        desc: "browse run logs",
        group: PROFILES,
        footer: true,
    },
    Binding {
        key: "/",
        verb: "filter",
        help_key: "/",
        desc: "filter profiles by name",
        group: PROFILES,
        footer: true,
    },
    Binding {
        key: "L",
        verb: "activity",
        help_key: "L",
        desc: "toggle the activity panel",
        group: PROFILES,
        footer: false,
    },
    Binding {
        key: "?",
        verb: "help",
        help_key: "? / esc",
        desc: "close this help",
        group: GLOBAL,
        footer: true,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        footer: true,
    },
];

pub const EDITOR: &[Binding] = &[
    Binding {
        key: "↑↓←→",
        verb: "move",
        help_key: "↑↓←→ or hjkl",
        desc: "move in the current section",
        group: MOVE_AND_EDIT,
        footer: true,
    },
    Binding {
        key: "enter",
        verb: "edit",
        help_key: "enter / space",
        desc: "act on the highlighted row",
        group: MOVE_AND_EDIT,
        footer: true,
    },
    Binding {
        key: "tab",
        verb: "section",
        help_key: "tab / shift-tab",
        desc: "switch section",
        group: MOVE_AND_EDIT,
        footer: true,
    },
    Binding {
        key: "/",
        verb: "filter",
        help_key: "/",
        desc: "filter the steps list",
        group: MOVE_AND_EDIT,
        footer: false,
    },
    Binding {
        key: "ctrl+s",
        verb: "save",
        help_key: "ctrl+s",
        desc: "save from anywhere in the editor",
        group: SAVE_AND_LEAVE,
        footer: true,
    },
    Binding {
        key: "esc",
        verb: "back",
        help_key: "esc",
        desc: "save or leave — asks when there are unsaved changes",
        group: SAVE_AND_LEAVE,
        footer: true,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: SAVE_AND_LEAVE,
        footer: true,
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
    },
    Binding {
        key: "esc",
        verb: "back",
        help_key: "esc",
        desc: "back to the dashboard",
        group: GLOBAL,
        footer: true,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        footer: true,
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
    },
    Binding {
        key: "↑↓",
        verb: "scroll",
        help_key: "↑↓ / pgup/pgdn",
        desc: "scroll the live log (auto-follows at the bottom)",
        group: MOVE_AND_EDIT,
        footer: true,
    },
    Binding {
        key: "esc",
        verb: "back",
        help_key: "esc",
        desc: "back to the dashboard",
        group: GLOBAL,
        footer: true,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        footer: true,
    },
];

pub const LOGS_BROWSE: &[Binding] = &[
    Binding {
        key: "enter",
        verb: "open",
        help_key: "enter",
        desc: "open the highlighted run",
        group: PROFILES,
        footer: true,
    },
    Binding {
        key: "r",
        verb: "refresh",
        help_key: "r",
        desc: "reload the run list",
        group: PROFILES,
        footer: true,
    },
    Binding {
        key: "esc",
        verb: "back",
        help_key: "esc / h",
        desc: "back to the list or dashboard",
        group: GLOBAL,
        footer: true,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        footer: true,
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
    },
    Binding {
        key: "Enter",
        verb: "accept",
        help_key: "",
        desc: "",
        group: "",
        footer: true,
    },
    Binding {
        key: "Esc",
        verb: "clear",
        help_key: "",
        desc: "",
        group: "",
        footer: true,
    },
    Binding {
        key: "↑↓",
        verb: "move",
        help_key: "",
        desc: "",
        group: "",
        footer: true,
    },
];

pub fn footer_tokens(table: &[Binding]) -> String {
    table
        .iter()
        .filter(|b| b.footer)
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
            footer_tokens(DASHBOARD),
            "↑↓ move  n new  e edit  d delete  r run now  l logs  / filter  ? help  q quit"
        );
        assert_eq!(
            footer_tokens(EDITOR),
            "↑↓←→ move  enter edit  tab section  ctrl+s save  esc back  q quit"
        );
        assert_eq!(
            footer_tokens(PRESET_PICKER),
            "enter choose  esc back  q quit"
        );
        assert_eq!(
            footer_tokens(LOGS_FOLLOW),
            "x stop  ↑↓ scroll  esc back  q quit"
        );
        assert_eq!(
            footer_tokens(LOGS_BROWSE),
            "enter open  r refresh  esc back  q quit"
        );
    }

    #[test]
    fn footer_legends_fit_eighty_columns() {
        for table in [DASHBOARD, EDITOR, PRESET_PICKER, LOGS_FOLLOW, LOGS_BROWSE] {
            let legend = footer_tokens(table);
            assert!(
                legend.chars().count() <= 80,
                "legend overflows the common terminal width: {legend}"
            );
        }
    }

    #[test]
    fn readme_documents_every_footer_action() {
        let readme = include_str!("../../README.md");
        for table in [DASHBOARD, EDITOR, PRESET_PICKER, LOGS_FOLLOW, LOGS_BROWSE] {
            for binding in table.iter().filter(|b| b.footer) {
                assert!(
                    readme.contains(binding.key),
                    "README misses the {:?} key of a footer action",
                    binding.key
                );
                if !binding.verb.is_empty() {
                    assert!(
                        readme.contains(binding.verb),
                        "README misses the {:?} verb of the {} action",
                        binding.verb,
                        binding.key
                    );
                }
            }
        }
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
