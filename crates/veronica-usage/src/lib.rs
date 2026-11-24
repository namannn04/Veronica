pub mod aggregate;
pub mod alerts;
pub mod cards;
pub mod claude;
pub mod codex;
pub mod collector;
pub mod credentials;
pub mod gauges;
pub mod limits;
pub mod models;

pub use aggregate::{dashboard, Dashboard, DayRange, SourceSelection};
pub use cards::Card;
pub use collector::{CollectorEvent, RefreshOutcome};
pub use gauges::{Gauge, GaugeReport};
pub use limits::{LimitProvider, LimitWindow, ProviderLimits, UsageLevel, UsageThresholds};
pub use models::{UsageDocument, SCHEMA_VERSION};

pub(crate) fn paths_home() -> Option<std::path::PathBuf> {
    veronica_core::paths::home_dir()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "std has no checked float-to-integer conversion; `as` truncates toward zero, saturates at the bounds and maps NaN to zero, which is what every caller wants"
)]
pub(crate) fn truncate_to_i64(value: f64) -> i64 {
    value as i64
}
