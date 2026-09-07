#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Foot {
    Always,
    DirtyOnly,
    CleanOnly,
    Never,
}

pub struct Binding {
    pub key: &'static str,
    pub verb: &'static str,
    pub help_key: &'static str,
    pub desc: &'static str,
    pub group: &'static str,
    pub foot: Foot,
}

const MOVE_AND_EDIT: &str = "Move & edit";
const SAVE_AND_LEAVE: &str = "Save & leave";
const PROFILES: &str = "Profiles";
const GLOBAL: &str = "Global";

pub const DASHBOARD: &[Binding] = &[
    Binding {
        key: "L",
        verb: "activity",
        help_key: "L",
        desc: "toggle the activity panel",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "/",
        verb: "filter",
        help_key: "/",
        desc: "filter profiles by name",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "n",
        verb: "new",
        help_key: "n",
        desc: "new profile (from presets)",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "e",
        verb: "edit",
        help_key: "enter / e",
        desc: "edit the selected profile",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "d",
        verb: "delete",
        help_key: "d",
        desc: "delete profile — always asks first",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "r",
        verb: "run now",
        help_key: "r",
        desc: "run now, opens the live view",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "l",
        verb: "logs",
        help_key: "l",
        desc: "browse run logs",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "j/k",
        verb: "",
        help_key: "↑/↓ or j/k",
        desc: "move selection",
        group: PROFILES,
        foot: Foot::Never,
    },
    Binding {
        key: "?",
        verb: "help",
        help_key: "? / esc",
        desc: "close this help",
        group: GLOBAL,
        foot: Foot::Always,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        foot: Foot::Always,
    },
];

pub const EDITOR: &[Binding] = &[
    Binding {
        key: "ctrl+s",
        verb: "save",
        help_key: "ctrl+s",
        desc: "save from anywhere in the editor",
        group: SAVE_AND_LEAVE,
        foot: Foot::Always,
    },
    Binding {
        key: "esc",
        verb: "save/leave",
        help_key: "esc",
        desc: "save or leave — asks when there are unsaved changes",
        group: SAVE_AND_LEAVE,
        foot: Foot::DirtyOnly,
    },
    Binding {
        key: "↑↓←→",
        verb: "move",
        help_key: "↑↓←→ or hjkl",
        desc: "move in the current section",
        group: MOVE_AND_EDIT,
        foot: Foot::Always,
    },
    Binding {
        key: "enter",
        verb: "edit",
        help_key: "enter / space",
        desc: "act on the highlighted row",
        group: MOVE_AND_EDIT,
        foot: Foot::CleanOnly,
    },
    Binding {
        key: "tab",
        verb: "section",
        help_key: "tab / shift-tab",
        desc: "switch section",
        group: MOVE_AND_EDIT,
        foot: Foot::Always,
    },
    Binding {
        key: "/",
        verb: "filter",
        help_key: "/",
        desc: "filter the steps list",
        group: MOVE_AND_EDIT,
        foot: Foot::Never,
    },
    Binding {
        key: "esc",
        verb: "back",
        help_key: "",
        desc: "",
        group: SAVE_AND_LEAVE,
        foot: Foot::CleanOnly,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: SAVE_AND_LEAVE,
        foot: Foot::Always,
    },
];

pub const PRESET_PICKER: &[Binding] = &[
    Binding {
        key: "enter",
        verb: "choose",
        help_key: "enter",
        desc: "start this preset",
        group: MOVE_AND_EDIT,
        foot: Foot::Always,
    },
    Binding {
        key: "esc",
        verb: "back",
        help_key: "esc",
        desc: "back to the dashboard",
        group: GLOBAL,
        foot: Foot::Always,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        foot: Foot::Always,
    },
];

pub const LOGS_FOLLOW: &[Binding] = &[
    Binding {
        key: "x",
        verb: "stop",
        help_key: "x",
        desc: "stop the running profile",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "↑↓",
        verb: "scroll",
        help_key: "↑↓ / pgup/pgdn",
        desc: "scroll the live log (auto-follows at the bottom)",
        group: MOVE_AND_EDIT,
        foot: Foot::Always,
    },
    Binding {
        key: "esc",
        verb: "back",
        help_key: "esc",
        desc: "back to the dashboard",
        group: GLOBAL,
        foot: Foot::Always,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        foot: Foot::Always,
    },
];

pub const LOGS_BROWSE: &[Binding] = &[
    Binding {
        key: "enter",
        verb: "open",
        help_key: "enter",
        desc: "open the highlighted run",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "h",
        verb: "back",
        help_key: "esc / h",
        desc: "back to the list or dashboard",
        group: GLOBAL,
        foot: Foot::Always,
    },
    Binding {
        key: "r",
        verb: "refresh",
        help_key: "r",
        desc: "reload the run list",
        group: PROFILES,
        foot: Foot::Always,
    },
    Binding {
        key: "esc",
        verb: "back",
        help_key: "",
        desc: "",
        group: GLOBAL,
        foot: Foot::Always,
    },
    Binding {
        key: "q",
        verb: "quit",
        help_key: "q",
        desc: "quit topmatic",
        group: GLOBAL,
        foot: Foot::Always,
    },
];

pub fn footer_tokens(table: &[Binding], dirty: bool) -> String {
    table
        .iter()
        .filter(|b| match b.foot {
            Foot::Always => true,
            Foot::DirtyOnly => dirty,
            Foot::CleanOnly => !dirty,
            Foot::Never => false,
        })
        .map(|b| format!("{} {}", b.key, b.verb))
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
            "L activity  / filter  n new  e edit  d delete  r run now  l logs  ? help  q quit"
        );
        assert_eq!(
            footer_tokens(EDITOR, false),
            "ctrl+s save  ↑↓←→ move  enter edit  tab section  esc back  q quit"
        );
        assert_eq!(
            footer_tokens(EDITOR, true),
            "ctrl+s save  esc save/leave  ↑↓←→ move  tab section  q quit"
        );
        assert_eq!(
            footer_tokens(PRESET_PICKER, false),
            "enter choose  esc back  q quit"
        );
        assert_eq!(
            footer_tokens(LOGS_FOLLOW, false),
            "x stop  ↑↓ scroll  esc back  q quit"
        );
        assert_eq!(
            footer_tokens(LOGS_BROWSE, false),
            "enter open  h back  r refresh  esc back  q quit"
        );
    }

    #[test]
    fn help_groups_keep_first_appearance_order_without_duplicates() {
        let groups = help_groups(EDITOR);
        assert_eq!(
            groups.first().map(|(title, _)| *title),
            Some(SAVE_AND_LEAVE)
        );
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
            for binding in table.iter().filter(|b| b.foot != Foot::Never) {
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
