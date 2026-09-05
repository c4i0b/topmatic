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
    Custom {
        calendar: String,
    },
}

impl SchedulePreset {
    pub fn on_calendar(&self) -> String {
        match self {
            SchedulePreset::Hourly => "*-*-* *:00:00".to_string(),
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
        }
    }
}

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
            preset: SchedulePreset::Daily {
                hour: 12,
                minute: 0,
            },
            randomized_delay_sec: default_randomized_delay_sec(),
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
            SchedulePreset::Custom { calendar } => format!("custom: {calendar}"),
        }
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
    fn default_schedule_is_daily_at_noon_with_delay() {
        let schedule = Schedule::default();
        assert_eq!(schedule.preset.on_calendar(), "*-*-* 12:00:00");
        assert_eq!(schedule.randomized_delay_sec, 900);
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
        let schedule: Schedule = toml::from_str("preset = 'hourly'").unwrap();
        assert_eq!(schedule.randomized_delay_sec, 900);
    }
}
