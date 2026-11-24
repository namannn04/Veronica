use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use veronica_usage::aggregate::{self, DayRange, SourceSelection};
use veronica_usage::collector;

use super::{fail, CommandResult};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageView {
    pub dashboard: Option<aggregate::Dashboard>,
    pub generated_at: Option<String>,
    pub sources: Vec<SourceOption>,
    pub models: Vec<String>,
    pub has_data: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceOption {
    pub id: String,
    pub label: String,
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn usage_view(
    state: State<'_, AppState>,
    days: Option<usize>,
    sources: Vec<String>,
) -> UsageView {
    let guard = state.usage.lock().expect("usage lock");
    let Some(document) = guard.as_ref() else {
        return UsageView {
            dashboard: None,
            generated_at: None,
            sources: Vec::new(),
            models: Vec::new(),
            has_data: false,
        };
    };

    let range = match days {
        Some(days) => DayRange::last_days(days, chrono::Local::now().date_naive()),
        None => DayRange::default(),
    };
    let selection = if sources.is_empty() {
        SourceSelection::All
    } else {
        SourceSelection::Only(sources)
    };

    let mut models: Vec<String> = document
        .daily
        .iter()
        .flat_map(|day| day.by_source.values())
        .flatten()
        .map(|row| row.model_name.clone())
        .collect();
    models.sort_unstable();
    models.dedup();

    UsageView {
        dashboard: Some(aggregate::dashboard(document, &range, &selection)),
        generated_at: Some(document.generated_at.clone()),
        sources: document
            .sources
            .iter()
            .map(|id| SourceOption {
                id: id.clone(),
                label: document.label_for(id),
            })
            .collect(),
        models,
        has_data: true,
    }
}

#[tauri::command]
pub(crate) async fn usage_export(
    state: State<'_, AppState>,
    cards: Vec<String>,
    days: Option<usize>,
    sources: Option<Vec<String>>,
) -> CommandResult<Vec<String>> {
    use veronica_usage::cards::{self, Card};

    let chosen: Vec<Card> = if cards.is_empty() {
        Card::ALL.to_vec()
    } else {
        let mut chosen = Vec::new();
        for raw in &cards {
            for card in Card::parse(raw).ok_or_else(|| format!("unknown card {raw:?}"))? {
                if !chosen.contains(&card) {
                    chosen.push(card);
                }
            }
        }
        chosen
    };

    let (board, label) = {
        let guard = state.usage.lock().expect("usage lock");
        let document = guard
            .as_ref()
            .ok_or_else(|| "No usage has been collected yet.".to_string())?;
        let range = match days {
            Some(days) => DayRange::last_days(days, chrono::Local::now().date_naive()),
            None => DayRange::default(),
        };
        let label = match (range.start.as_deref(), range.end.as_deref()) {
            (Some(start), Some(end)) if start == end => start.to_string(),
            (Some(start), Some(end)) => format!("{start} to {end}"),
            _ => "all time".to_string(),
        };
        let selection = match sources.filter(|sources| !sources.is_empty()) {
            Some(sources) => SourceSelection::Only(sources),
            None => SourceSelection::All,
        };
        (
            veronica_usage::dashboard(document, &range, &selection),
            label,
        )
    };
    if board.totals.tokens == 0 {
        return Err("No usage in this window, so there is nothing to put on a card.".to_string());
    }

    let directory = veronica_core::paths::home_dir()
        .map(|home| home.join("Pictures"))
        .filter(|pictures| pictures.is_dir())
        .or_else(veronica_core::paths::home_dir)
        .ok_or_else(|| "cannot resolve the home directory".to_string())?;
    let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let mut written = Vec::new();
        for card in chosen {
            let png = cards::render_png(&cards::render_svg(card, &board, &label)).map_err(fail)?;
            let path = directory.join(cards::file_name(card, &stamp, "png"));
            std::fs::write(&path, &png)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            written.push(path.display().to_string());
        }
        Ok(written)
    })
    .await
    .map_err(|error| format!("the export could not run: {error}"))?
}

#[tauri::command]
pub(crate) async fn usage_refresh(app: AppHandle) -> CommandResult<String> {
    {
        let state = app.state::<AppState>();
        let mut refreshing = state.refreshing.lock().expect("refresh lock");
        if *refreshing {
            return Err("A refresh is already running.".to_string());
        }
        *refreshing = true;
    }

    let result = run_refresh(&app).await;

    let state = app.state::<AppState>();
    *state.refreshing.lock().expect("refresh lock") = false;

    result
}

async fn run_refresh(app: &AppHandle) -> CommandResult<String> {
    let (script, out_dir, cache_dir) = {
        let state = app.state::<AppState>();
        (
            state.directories.collector_script(),
            state.directories.usage_dir(),
            state.directories.cache.clone(),
        )
    };

    collector::install_script(&script).map_err(fail)?;

    let emitter = app.clone();
    let outcome = collector::refresh(&script, &out_dir, &cache_dir, move |event| {
        let _ = emitter.emit("usage-progress", event);
    })
    .await
    .map_err(fail)?;

    let generated_at = outcome.document.generated_at.clone();
    {
        let state = app.state::<AppState>();
        *state.usage.lock().expect("usage lock") = Some(outcome.document);
    }
    let _ = app.emit("usage-updated", &generated_at);
    Ok(generated_at)
}

#[tauri::command]
pub(crate) async fn usage_limits() -> CommandResult<veronica_usage::GaugeReport> {
    Ok(veronica_usage::gauges::collect(
        chrono::Utc::now(),
        veronica_usage::gauges::DEFAULT_PACING_MARGIN,
    )
    .await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertsView {
    pub settings: veronica_usage::alerts::NotifySettings,
    pub poll_seconds: u64,
    pub session: Option<WatchedWindow>,
    pub week: Option<WatchedWindow>,
    pub note: Option<String>,
    pub session_reminder_at: Option<String>,
    pub week_reminder_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatchedWindow {
    pub percent: f64,
    pub resets_at: Option<String>,
    pub resets_in: Option<String>,
    pub level: veronica_usage::limits::UsageLevel,
    pub zone: veronica_usage::limits::PacingZone,
}

#[tauri::command]
pub(crate) async fn alerts_view(app: AppHandle) -> CommandResult<AlertsView> {
    use veronica_usage::alerts::{self, NotifySettings};
    use veronica_usage::limits::{
        level_for_risk, pacing_delta, pacing_zone, smart_risk, LimitWindow, LimitWindowKind,
    };

    let settings = app.state::<AppState>().settings_snapshot();
    let notify = NotifySettings::from_settings(&settings);
    let poll_seconds = crate::alerts::poll_interval(&settings).as_secs();
    let now = chrono::Utc::now();

    let describe = |window: LimitWindow, kind: LimitWindowKind| WatchedWindow {
        percent: window.percent,
        resets_at: window.resets_at.map(|at| at.to_rfc3339()),
        resets_in: window
            .resets_at
            .filter(|at| *at > now)
            .map(|at| alerts::countdown(now, at)),
        level: if notify.smart_color {
            level_for_risk(smart_risk(
                window.percent,
                window.resets_at,
                kind.duration_secs(),
                notify.pacing_margin,
                now,
            ))
        } else {
            veronica_usage::limits::UsageLevel::from_percent(window.percent, notify.thresholds)
        },
        zone: window
            .resets_at
            .map_or(veronica_usage::limits::PacingZone::OnTrack, |at| {
                pacing_zone(
                    pacing_delta(window.percent, at, kind.duration_secs(), now),
                    notify.pacing_margin,
                )
            }),
    };

    let limits = if notify.master {
        veronica_usage::claude::limits_for_user(now).await
    } else {
        return Ok(AlertsView {
            poll_seconds,
            session: None,
            week: None,
            session_reminder_at: None,
            week_reminder_at: None,
            note: None,
            settings: notify,
        });
    };
    let (session, week, note) = match limits {
        Ok(Some(limits)) => (limits.session, limits.week, None),
        Ok(None) => (
            None,
            None,
            Some("Claude is not signed in on this computer, so there is nothing to watch.".into()),
        ),
        Err(error) => (None, None, Some(format!("{error:#}"))),
    };

    Ok(AlertsView {
        poll_seconds,
        session: session.map(|window| describe(window, LimitWindowKind::Session)),
        week: week.map(|window| describe(window, LimitWindowKind::Weekly)),
        session_reminder_at: alerts::reminder_fire_at(
            session.and_then(|window| window.resets_at),
            notify.reminder_session_offset_min,
            now,
        )
        .filter(|_| notify.reminder_session)
        .map(|at| at.to_rfc3339()),
        week_reminder_at: alerts::reminder_fire_at(
            week.and_then(|window| window.resets_at),
            notify.reminder_weekly_offset_min,
            now,
        )
        .filter(|_| notify.reminder_weekly)
        .map(|at| at.to_rfc3339()),
        note,
        settings: notify,
    })
}

#[tauri::command]
pub(crate) async fn alerts_test() -> CommandResult<String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("no session bus, so no notifications: {error}"))?;
    match crate::alerts::send_test(&connection).await {
        Ok(_) => Ok("Sent. Check your notification list if no banner appeared.".to_string()),
        Err(error) => Err(fail(error)),
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn alerts_reset(state: State<'_, AppState>) -> CommandResult<()> {
    let path = state.directories.alerts_state_file();
    let mut notifier = veronica_usage::alerts::NotifierState::load(&path);
    notifier.reset_tracking();
    notifier.save(&path).map_err(fail)
}

#[cfg(test)]
mod wire_shape_tests {
    use super::*;
    use crate::commands::assert_camel_case;

    #[test]
    fn the_watched_window_is_camel_case_throughout() {
        let window = WatchedWindow {
            percent: 81.0,
            resets_at: Some("2026-08-27T18:00:00Z".into()),
            resets_in: Some("2 h 14 min".into()),
            level: veronica_usage::limits::UsageLevel::Orange,
            zone: veronica_usage::limits::PacingZone::OnTrack,
        };
        assert_camel_case(&window, "WatchedWindow");
        let json = serde_json::to_value(&window).unwrap();
        assert_eq!(json["resetsIn"], "2 h 14 min");
        assert_eq!(json["level"], "orange");
        assert_eq!(json["zone"], "onTrack");
    }

    #[test]
    fn the_alerts_view_is_camel_case_including_its_nested_settings() {
        let view = AlertsView {
            settings: veronica_usage::alerts::NotifySettings::default(),
            poll_seconds: 60,
            session: None,
            week: None,
            note: None,
            session_reminder_at: None,
            week_reminder_at: None,
        };
        assert_camel_case(&view, "AlertsView");
        let json = serde_json::to_value(&view).unwrap();
        assert!(json["pollSeconds"].is_number());
        assert!(json["settings"]["thresholds"]["warningPercent"].is_number());
    }
}
