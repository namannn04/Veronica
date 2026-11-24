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
