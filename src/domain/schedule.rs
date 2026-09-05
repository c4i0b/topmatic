use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    pub const ALL: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    pub fn as_systemd(self) -> &'static str {
        match self {
            Weekday::Mon => "Mon",
            Weekday::Tue => "Tue",
            Weekday::Wed => "Wed",
            Weekday::Thu => "Thu",
            Weekday::Fri => "Fri",
            Weekday::Sat => "Sat",
            Weekday::Sun => "Sun",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "preset", rename_all = "kebab-case")]
pub enum SchedulePreset {
    EveryNHours {
        hours: u32,
    },
    Daily {
        hour: u32,
        minute: u32,
    },
    Weekly {
        weekday: Weekday,
        hour: u32,
        minute: u32,
    },
    Custom {
        calendar: String,
    },
    #[serde(rename = "spread")]
    LegacySpread {
        #[serde(default)]
        period: Option<String>,
    },
}

impl SchedulePreset {
    pub fn on_calendar(&self) -> String {
        match self {
            SchedulePreset::EveryNHours { hours } => format!("*-*-* 00/{hours:02}:00:00"),
            SchedulePreset::Daily { hour, minute } => {
                format!("*-*-* {hour:02}:{minute:02}:00")
            }
            SchedulePreset::Weekly {
                weekday,
                hour,
                minute,
            } => format!("{} *-*-* {hour:02}:{minute:02}:00", weekday.as_systemd()),
            SchedulePreset::Custom { calendar } => calendar.clone(),
            SchedulePreset::LegacySpread { period } => match period.as_deref() {
                Some("weekly") => "weekly".to_string(),
                _ => "daily".to_string(),
            },
        }
    }

    pub fn normalized(self) -> Self {
        match self {
            SchedulePreset::LegacySpread { .. } => SchedulePreset::Daily { hour: 0, minute: 0 },
            other => other,
        }
    }
}

pub const DEFAULT_DELAY_SEC: u64 = 1_800;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    #[serde(flatten)]
    pub preset: SchedulePreset,
    #[serde(default = "default_randomized_delay_sec")]
    pub randomized_delay_sec: u64,
}

fn default_randomized_delay_sec() -> u64 {
    DEFAULT_DELAY_SEC
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            preset: SchedulePreset::Daily { hour: 0, minute: 0 },
            randomized_delay_sec: DEFAULT_DELAY_SEC,
        }
    }
}

impl Schedule {
    pub fn summary(&self) -> String {
        match &self.preset {
            SchedulePreset::EveryNHours { hours } => format!("every {hours}h"),
            SchedulePreset::Daily { hour, minute } => format!("daily {hour:02}:{minute:02}"),
            SchedulePreset::Weekly {
                weekday,
                hour,
                minute,
            } => format!("weekly {} {hour:02}:{minute:02}", weekday.as_systemd()),
            SchedulePreset::Custom { calendar } => format!("custom: {calendar}"),
            SchedulePreset::LegacySpread { .. } => "daily".to_string(),
        }
    }
}

pub fn quick_choices() -> Vec<(&'static str, Schedule)> {
    vec![
        (
            "daily with random jitter",
            Schedule {
                preset: SchedulePreset::Daily { hour: 0, minute: 0 },
                randomized_delay_sec: DEFAULT_DELAY_SEC,
            },
        ),
        (
            "daily at a fixed time",
            Schedule {
                preset: SchedulePreset::Daily {
                    hour: 12,
                    minute: 0,
                },
                randomized_delay_sec: 0,
            },
        ),
        (
            "weekly",
            Schedule {
                preset: SchedulePreset::Weekly {
                    weekday: Weekday::Mon,
                    hour: 12,
                    minute: 0,
                },
                randomized_delay_sec: 900,
            },
        ),
        (
            "every 6 hours",
            Schedule {
                preset: SchedulePreset::EveryNHours { hours: 6 },
                randomized_delay_sec: 900,
            },
        ),
    ]
}

pub fn matches_quick_choice(schedule: &Schedule) -> Option<usize> {
    quick_choices()
        .iter()
        .position(|(_, choice)| choice == schedule)
}

pub fn format_delay(seconds: u64) -> String {
    if seconds >= 3600 && seconds.is_multiple_of(3600) {
        format!("{}h", seconds / 3600)
    } else if seconds >= 60 {
        format!("{}min", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_on_calendar_for_each_preset() {
        let cases = [
            (
                SchedulePreset::EveryNHours { hours: 6 },
                "*-*-* 00/06:00:00".to_string(),
            ),
            (
                SchedulePreset::Daily {
                    hour: 12,
                    minute: 0,
                },
                "*-*-* 12:00:00".to_string(),
            ),
            (
                SchedulePreset::Daily {
                    hour: 3,
                    minute: 30,
                },
                "*-*-* 03:30:00".to_string(),
            ),
            (
                SchedulePreset::Weekly {
                    weekday: Weekday::Mon,
                    hour: 9,
                    minute: 15,
                },
                "Mon *-*-* 09:15:00".to_string(),
            ),
            (
                SchedulePreset::Custom {
                    calendar: "Fri *-*-* 10:00:00".to_string(),
                },
                "Fri *-*-* 10:00:00".to_string(),
            ),
        ];
        for (preset, expected) in cases {
            assert_eq!(preset.on_calendar(), expected, "{preset:?}");
        }
    }

    #[test]
    fn default_schedule_is_daily_anchor_with_jitter() {
        let schedule = Schedule::default();
        assert_eq!(schedule.preset.on_calendar(), "*-*-* 00:00:00");
        assert_eq!(schedule.randomized_delay_sec, 1_800);
    }

    #[test]
    fn legacy_spread_normalizes_to_daily_anchor() {
        let legacy = SchedulePreset::LegacySpread {
            period: Some("daily".to_string()),
        };
        assert_eq!(
            legacy.clone().normalized(),
            SchedulePreset::Daily { hour: 0, minute: 0 }
        );
        assert_eq!(legacy.on_calendar(), "daily");
    }

    #[test]
    fn legacy_spround_round_trips_from_old_config() {
        let text = "preset = \"spread\"\nperiod = \"daily\"\nrandomized_delay_sec = 1800";
        let schedule: Schedule = toml::from_str(text).unwrap();
        assert!(matches!(
            schedule.preset,
            SchedulePreset::LegacySpread { .. }
        ));
        let normalized = Schedule {
            preset: schedule.preset.normalized(),
            randomized_delay_sec: schedule.randomized_delay_sec,
        };
        assert_eq!(normalized, Schedule::default());
    }

    #[test]
    fn quick_choices_match_their_own_schedules() {
        for (index, (_, choice)) in quick_choices().iter().enumerate() {
            assert_eq!(matches_quick_choice(choice), Some(index));
        }
        assert_eq!(matches_quick_choice(&Schedule::default()), Some(0));
    }

    #[test]
    fn formats_delays_in_human_units() {
        assert_eq!(format_delay(3_600), "1h");
        assert_eq!(format_delay(1_800), "30min");
        assert_eq!(format_delay(45), "45s");
    }

    #[test]
    fn schedule_round_trips_through_toml() {
        let schedule = Schedule {
            preset: SchedulePreset::Weekly {
                weekday: Weekday::Sat,
                hour: 18,
                minute: 45,
            },
            randomized_delay_sec: 300,
        };
        let text = toml::to_string(&schedule).unwrap();
        let back: Schedule = toml::from_str(&text).unwrap();
        assert_eq!(schedule, back);
    }

    #[test]
    fn missing_randomized_delay_falls_back_to_default() {
        let schedule: Schedule = toml::from_str("preset = 'daily'\nhour = 5\nminute = 0").unwrap();
        assert_eq!(schedule.randomized_delay_sec, DEFAULT_DELAY_SEC);
    }
}
