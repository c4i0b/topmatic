pub struct Preset {
    pub label: &'static str,
    pub description: &'static str,
    pub suggested_name: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        label: "All",
        description: "everything installed; missing steps just skip (recommended)",
        suggested_name: "all-daily",
    },
    Preset {
        label: "Dev tools",
        description: "runtimes, editors and tooling only",
        suggested_name: "dev-daily",
    },
    Preset {
        label: "Flatpak",
        description: "apps and runtimes only",
        suggested_name: "flatpak-daily",
    },
];

pub use crate::domain::presets::{fallback_catalog, steps_for};
