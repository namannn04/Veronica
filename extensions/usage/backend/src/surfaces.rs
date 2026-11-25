use std::time::Duration;

use serde_json::{json, Value};
use veronica_sdk::format::money;
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::settings::Settings;
use veronica_sdk::Context;
use veronica_usage::aggregate::{self, DayRange, HeatmapCell, SourceSelection};
use veronica_usage::cards::tokens;
use veronica_usage::gauges::{Gauge, GaugeReport};

pub(crate) const ACTIVITY: &str = "activity";
pub(crate) const SPEND: &str = "spend";
pub(crate) const LIMITS: &str = "limits";
pub(crate) const PROVIDER_KEY: &str = "limitsProvider";
const HEATMAP_DAYS: usize = 140;
const LIMITS_EVERY: Duration = Duration::from_mins(5);
const SHOWN_GAUGES: usize = 3;

pub(crate) fn provider(settings: &Settings) -> &'static str {
    match settings.get(PROVIDER_KEY).and_then(Value::as_str) {
        Some(value) if value.eq_ignore_ascii_case("codex") => "codex",
        _ => "claude",
    }
}

fn cells(heatmap: &[HeatmapCell]) -> Value {
    let start = heatmap.len().saturating_sub(HEATMAP_DAYS);
    Value::Array(
        heatmap[start..]
            .iter()
            .map(|cell| {
                json!({
                    "period": cell.period,
                    "level": cell.level,
                    "cost": cell.cost,
                    "tokens": cell.tokens,
                })
            })
            .collect(),
    )
}

pub(crate) fn usage_states(document: Option<&veronica_usage::UsageDocument>) -> (Value, Value) {
    let Some(document) = document else {
        return (
            json!({ "cells": [], "caption": "No activity collected yet. Agent usage fills this in after its first collection." }),
            json!({ "value": "-", "label": "Spend, all time", "caption": "Nothing collected yet" }),
        );
    };
    let board = aggregate::dashboard(document, &DayRange::default(), &SourceSelection::All);
    (
        json!({ "cells": cells(&board.heatmap), "caption": "Daily estimated spend" }),
        json!({
            "value": money(board.totals.cost),
            "label": "Spend, all time",
            "caption": format!("{} tokens", tokens(board.totals.tokens)),
        }),
    )
}

fn title_case(provider: &str) -> String {
    let mut letters = provider.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
        .unwrap_or_default()
}

pub(crate) fn limits_state(report: &GaugeReport, provider: &str) -> Value {
    let name = title_case(provider);
    let gauges: Vec<&Gauge> = report
        .gauges
        .iter()
        .filter(|gauge| gauge.provider.eq_ignore_ascii_case(provider))
        .take(SHOWN_GAUGES)
        .collect();
    let Some(pressing) = gauges.iter().max_by(|a, b| a.percent.total_cmp(&b.percent)) else {
        let attention = report.notes.iter().any(|note| note.starts_with(&name));
        return json!({
            "value": 0.0,
            "label": name,
            "caption": if attention {
                format!("{name} needs attention. Open rate limits for details.")
            } else {
                "No limit data yet.".to_string()
            },
        });
    };
    let caption = gauges
        .iter()
        .map(|gauge| format!("{} {}%", gauge.window, gauge.percent.round()))
        .collect::<Vec<_>>()
        .join(" · ");
    json!({
        "value": (pressing.percent / 100.0).clamp(0.0, 1.0),
        "label": format!("{name} {}", pressing.window),
        "caption": caption,
    })
}

async fn update(context: &Context, surface: &str, state: Value) {
    if let Err(error) = context.host().surface_update(surface, state).await {
        tracing::debug!("{surface} was not published: {}", error.message);
    }
}

pub(crate) async fn publish_usage(context: &Context, directories: &AppDirectories) {
    let document = veronica_usage::collector::read_document(&directories.usage_file())
        .ok()
        .flatten();
    let (activity, spend) = usage_states(document.as_ref());
    update(context, ACTIVITY, activity).await;
    update(context, SPEND, spend).await;
}

pub(crate) async fn publish_limits(context: &Context, directories: &AppDirectories) {
    let settings = Settings::load(&directories.settings_file()).unwrap_or_default();
    let report = veronica_usage::gauges::collect(
        chrono::Utc::now(),
        veronica_usage::gauges::DEFAULT_PACING_MARGIN,
    )
    .await;
    update(context, LIMITS, limits_state(&report, provider(&settings))).await;
}

pub(crate) async fn run(context: Context, directories: AppDirectories) {
    publish_usage(&context, &directories).await;
    loop {
        publish_limits(&context, &directories).await;
        tokio::time::sleep(LIMITS_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use veronica_usage::gauges::{Gauge, GaugeReport};
    use veronica_usage::limits::{PacingZone, UsageLevel};

    use super::{limits_state, usage_states};

    fn gauge(provider: &str, window: &str, percent: f64) -> Gauge {
        Gauge {
            provider: provider.into(),
            window: window.into(),
            percent,
            resets_in_secs: None,
            risk: 0.0,
            level: UsageLevel::Green,
            zone: PacingZone::OnTrack,
            pace_delta: None,
        }
    }

    #[test]
    fn the_ring_follows_the_most_used_window_of_the_chosen_provider() {
        let report = GaugeReport {
            gauges: vec![
                gauge("Claude", "Session", 42.0),
                gauge("Claude", "Weekly", 71.4),
                gauge("Codex", "Session", 90.0),
            ],
            notes: Vec::new(),
        };
        let claude = limits_state(&report, "claude");
        assert_eq!(claude["label"], "Claude Weekly");
        assert!((claude["value"].as_f64().unwrap() - 0.714).abs() < 1e-9);
        assert_eq!(claude["caption"], "Session 42% · Weekly 71%");
        assert_eq!(limits_state(&report, "codex")["label"], "Codex Session");
    }

    #[test]
    fn a_provider_without_gauges_explains_why() {
        let report = GaugeReport {
            gauges: Vec::new(),
            notes: vec!["Codex is signed out".into()],
        };
        assert_eq!(
            limits_state(&report, "codex")["caption"],
            "Codex needs attention. Open rate limits for details."
        );
        assert_eq!(
            limits_state(&report, "claude")["caption"],
            "No limit data yet."
        );
    }

    #[test]
    fn nothing_collected_still_gives_valid_cards() {
        let (activity, spend) = usage_states(None);
        assert_eq!(activity["cells"], serde_json::json!([]));
        assert_eq!(spend["value"], "-");
    }
}
