//! Agent usage collection and aggregation.
//!
//! Veronica reuses Edith's `refresh-usage` collector verbatim, so the numbers
//! are identical on both platforms. This crate drives that script, decodes its
//! schema-8 output, and provides the rollups and rate-limit maths the dashboard
//! and the rings render.

pub mod aggregate;
pub mod claude;
pub mod codex;
pub mod credentials;
pub mod gauges;
pub mod collector;
pub mod limits;
pub mod models;

pub use aggregate::{dashboard, Dashboard, DayRange, SourceSelection};
pub use collector::{CollectorEvent, RefreshOutcome};
pub use gauges::{Gauge, GaugeReport};
pub use limits::{LimitProvider, LimitWindow, ProviderLimits, UsageLevel, UsageThresholds};
pub use models::{UsageDocument, SCHEMA_VERSION};

/// The user's home directory, for locating provider credentials.
pub(crate) fn paths_home() -> Option<std::path::PathBuf> {
    veronica_core::paths::home_dir()
}
