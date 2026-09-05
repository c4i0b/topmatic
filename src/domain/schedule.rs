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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpreadPeriod {
    Daily,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "preset", rename_all = "kebab-case")]
pub enum SchedulePreset {
    Hourly,
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
    Spread {
        period: SpreadPeriod,
    },
    Custom {
        calendar: String,
    },
}

impl SchedulePreset {
    pub fn on_calendar(&self) -> String {
        match self {
            SchedulePreset::Hourly => "*-*-* *:00:00".to_string(),
            SchedulePreset::EveryNHours { hours } => format!("*-*-* 00/{hours:02}:00:00"),
            SchedulePreset::Daily { hour, minute } => format!("*-*-* {hour:02}:{minute:02}:00"),
            SchedulePreset::Weekly {
                weekday,
                hour,
                minute,
            } => format!("{} *-*-* {hour:02}:{minute:02}:00", weekday.as_systemd()),
            SchedulePreset::Spread { period } => match period {
                SpreadPeriod::Daily => "daily".to_string(),
                SpreadPeriod::Weekly => "weekly".to_string(),
            },
            SchedulePreset::Custom { calendar } => calendar.clone(),
        }
    }
}

pub const SPREAD_DELAY_SEC: u64 = 1_800;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    #[serde(flatten)]
    pub preset: SchedulePreset,
    #[serde(default = "default_randomized_delay_sec")]
    pub randomized_delay_sec: u64,
}

fn default_randomized_delay_sec() -> u64 {
    900
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            preset: SchedulePreset::Spread {
                period: SpreadPeriod::Daily,
            },
            randomized_delay_sec: SPREAD_DELAY_SEC,
        }
    }
}

impl Schedule {
    pub fn summary(&self) -> String {
        match &self.preset {
            SchedulePreset::Hourly => "hourly".to_string(),
            SchedulePreset::EveryNHours { hours } => format!("every {hours}h"),
            SchedulePreset::Daily { hour, minute } => format!("daily {hour:02}:{minute:02}"),
            SchedulePreset::Weekly {
                weekday,
                hour,
                minute,
            } => format!("weekly {} {hour:02}:{minute:02}", weekday.as_systemd()),
            SchedulePreset::Spread { period } => format!(
                "{} spread (≤{})",
                match period {
                    SpreadPeriod::Daily => "daily",
                    SpreadPeriod::Weekly => "weekly",
                },
                format_delay(self.randomized_delay_sec)
            ),
            SchedulePreset::Custom { calendar } => format!("custom: {calendar}"),
        }
    }
}

pub fn quick_choices() -> Vec<(&'static str, Schedule)> {
    vec![
        (
            "daily with random jitter",
            Schedule {
                preset: SchedulePreset::Spread {
                    period: SpreadPeriod::Daily,
                },
                randomized_delay_sec: SPREAD_DELAY_SEC,
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
            (SchedulePreset::Hourly, "*-*-* *:00:00".to_string()),
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
                SchedulePreset::Spread {
                    period: SpreadPeriod::Daily,
                },
                "daily".to_string(),
            ),
            (
                SchedulePreset::Spread {
                    period: SpreadPeriod::Weekly,
                },
                "weekly".to_string(),
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
    fn default_schedule_is_daily_with_a_small_jitter() {
        let schedule = Schedule::default();
        assert_eq!(
            schedule.preset,
            SchedulePreset::Spread {
                period: SpreadPeriod::Daily
            }
        );
        assert_eq!(schedule.preset.on_calendar(), "daily");
        assert_eq!(schedule.randomized_delay_sec, 1_800);
    }

    #[test]
    fn spread_summaries_mention_window_and_delay() {
        let schedule = Schedule::default();
        assert_eq!(schedule.summary(), "daily spread (≤30min)");
    }

    #[test]
    fn formats_delays_in_human_units() {
        assert_eq!(format_delay(43_200), "12h");
        assert_eq!(format_delay(900), "15min");
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
    fn spread_round_trips_through_toml() {
        let schedule = Schedule::default();
        let text = toml::to_string(&schedule).unwrap();
        assert!(text.contains("preset = \"spread\""));
        assert!(text.contains("period = \"daily\""));
        let back: Schedule = toml::from_str(&text).unwrap();
        assert_eq!(schedule, back);
    }

    #[test]
    fn quick_choices_offer_distinct_schedules() {
        let choices = quick_choices();
        assert!(choices.len() >= 4);
        let summaries: Vec<String> = choices.iter().map(|(_, s)| s.summary()).collect();
        let mut unique = summaries.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), summaries.len());
        assert_eq!(choices[0].1, Schedule::default());
    }

    #[test]
    fn missing_randomized_delay_falls_back_to_default() {
        let schedule: Schedule = toml::from_str("preset = 'hourly'").unwrap();
        assert_eq!(schedule.randomized_delay_sec, 900);
    }
}
