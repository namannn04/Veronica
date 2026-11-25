use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, params, reply};
use veronica_sdk::settings::Settings;
use veronica_sdk::Context;
use veronica_usage::aggregate::{self, DayRange, SourceSelection};
use veronica_usage::alerts::{self, NotifierState, NotifySettings};
use veronica_usage::limits::{
    level_for_risk, pacing_delta, pacing_zone, smart_risk, LimitWindow, LimitWindowKind,
    PacingZone, UsageLevel,
};

pub(crate) const METHODS: [&str; 10] = [
    "settings",
    "configure",
    "view",
    "export",
    "refresh",
    "refreshing",
    "limits",
    "alertsView",
    "alertsTest",
    "alertsReset",
];

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

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Window {
    days: Option<usize>,
    sources: Vec<String>,
    cards: Vec<String>,
}

fn range(days: Option<usize>) -> DayRange {
    match days {
        Some(days) => DayRange::last_days(days, chrono::Local::now().date_naive()),
        None => DayRange::default(),
    }
}

fn selection(sources: Vec<String>) -> SourceSelection {
    if sources.is_empty() {
        SourceSelection::All
    } else {
        SourceSelection::Only(sources)
    }
}

fn document(directories: &AppDirectories) -> anyhow::Result<Option<veronica_usage::UsageDocument>> {
    veronica_usage::collector::read_document(&directories.usage_file())
}

fn view(directories: &AppDirectories, window: Window) -> anyhow::Result<UsageView> {
    let Some(document) = document(directories)? else {
        return Ok(UsageView {
            dashboard: None,
            generated_at: None,
            sources: Vec::new(),
            models: Vec::new(),
            has_data: false,
        });
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
    Ok(UsageView {
        dashboard: Some(aggregate::dashboard(
            &document,
            &range(window.days),
            &selection(window.sources),
        )),
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
    })
}

fn export(directories: &AppDirectories, window: Window) -> anyhow::Result<Vec<String>> {
    use veronica_usage::cards::{self, Card};

    let mut chosen: Vec<Card> = Vec::new();
    for raw in &window.cards {
        for card in Card::parse(raw).with_context(|| format!("unknown card {raw:?}"))? {
            if !chosen.contains(&card) {
                chosen.push(card);
            }
        }
    }
    if chosen.is_empty() {
        chosen = Card::ALL.to_vec();
    }
    let document = document(directories)?.context("No usage has been collected yet.")?;
    let range = range(window.days);
    let label = match (range.start.as_deref(), range.end.as_deref()) {
        (Some(start), Some(end)) if start == end => start.to_string(),
        (Some(start), Some(end)) => format!("{start} to {end}"),
        _ => "all time".to_string(),
    };
    let board = veronica_usage::dashboard(&document, &range, &selection(window.sources));
    anyhow::ensure!(
        board.totals.tokens != 0,
        "No usage in this window, so there is nothing to put on a card."
    );
    let home = veronica_sdk::paths::home_dir().context("cannot resolve the home directory")?;
    let pictures = home.join("Pictures");
    let directory = if pictures.is_dir() { pictures } else { home };
    let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();
    let mut written = Vec::new();
    for card in chosen {
        let png = cards::render_png(&cards::render_svg(card, &board, &label))?;
        let path = directory.join(cards::file_name(card, &stamp, "png"));
        std::fs::write(&path, &png).with_context(|| format!("cannot write {}", path.display()))?;
        written.push(path.display().to_string());
    }
    Ok(written)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertsView {
    pub settings: NotifySettings,
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
    pub level: UsageLevel,
    pub zone: PacingZone,
}

async fn alerts_view(directories: &AppDirectories) -> anyhow::Result<AlertsView> {
    let settings = Settings::load(&directories.settings_file())?;
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
            UsageLevel::from_percent(window.percent, notify.thresholds)
        },
        zone: window.resets_at.map_or(PacingZone::OnTrack, |at| {
            pacing_zone(
                pacing_delta(window.percent, at, kind.duration_secs(), now),
                notify.pacing_margin,
            )
        }),
    };

    if !notify.master {
        return Ok(AlertsView {
            poll_seconds,
            session: None,
            week: None,
            session_reminder_at: None,
            week_reminder_at: None,
            note: None,
            settings: notify,
        });
    }
    let (session, week, note) = match veronica_usage::claude::limits_for_user(now).await {
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

async fn alerts_test() -> anyhow::Result<String> {
    let connection = zbus::Connection::session()
        .await
        .context("no session bus, so no notifications")?;
    crate::alerts::send_test(&connection).await?;
    Ok("Sent. Check your notification list if no banner appeared.".to_string())
}

const OWNED_PREFIXES: [&str; 2] = ["notify", "limits"];

fn owned(key: &str) -> bool {
    OWNED_PREFIXES.iter().any(|prefix| key.starts_with(prefix))
}

fn owned_settings(directories: &AppDirectories) -> anyhow::Result<serde_json::Map<String, Value>> {
    let settings = Settings::load(&directories.settings_file())?;
    Ok(settings
        .as_map()
        .iter()
        .filter(|(key, _)| owned(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect())
}

#[derive(Deserialize)]
struct Change {
    key: String,
    value: Value,
}

fn configure(directories: &AppDirectories, change: Change) -> anyhow::Result<()> {
    anyhow::ensure!(
        owned(&change.key),
        "{} is not a usage or alert setting",
        change.key
    );
    let path = directories.settings_file();
    let mut settings = Settings::load(&path)?;
    if change.value.is_null() {
        settings.remove(&change.key);
    } else {
        settings.set(&change.key, change.value);
    }
    settings.save(&path)
}

fn alerts_reset(directories: &AppDirectories) -> anyhow::Result<()> {
    let path = directories.alerts_state_file();
    let mut notifier = NotifierState::load(&path);
    notifier.reset_tracking();
    notifier.save(&path)
}

pub(crate) async fn call(
    context: &Context,
    method: &str,
    arguments: Value,
) -> Result<Value, ErrorObject> {
    let directories = AppDirectories::current().map_err(|error| failure(&error))?;
    match method {
        "settings" => reply(&owned_settings(&directories).map_err(|e| failure(&e))?),
        "configure" => {
            let limits = {
                let change: Change = params(arguments)?;
                let limits = change.key.starts_with("limits");
                configure(&directories, change).map_err(|e| failure(&e))?;
                limits
            };
            if limits {
                crate::surfaces::publish_limits(context, &directories).await;
            }
            Ok(Value::Null)
        }
        "view" => reply(&view(&directories, params(arguments)?).map_err(|e| failure(&e))?),
        "export" => {
            let window: Window = params(arguments)?;
            let written = tokio::task::spawn_blocking(move || export(&directories, window))
                .await
                .map_err(|error| failure(&error.into()))?
                .map_err(|error| failure(&error))?;
            reply(&written)
        }
        "refresh" => reply(
            &crate::refresh::run(context, &directories)
                .await
                .map_err(|e| failure(&e))?,
        ),
        "refreshing" => reply(&crate::refresh::running()),
        "limits" => reply(
            &veronica_usage::gauges::collect(
                chrono::Utc::now(),
                veronica_usage::gauges::DEFAULT_PACING_MARGIN,
            )
            .await,
        ),
        "alertsView" => reply(&alerts_view(&directories).await.map_err(|e| failure(&e))?),
        "alertsTest" => reply(&alerts_test().await.map_err(|e| failure(&e))?),
        "alertsReset" => {
            alerts_reset(&directories).map_err(|e| failure(&e))?;
            Ok(Value::Null)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::{UsageView, WatchedWindow};
    use veronica_usage::limits::{PacingZone, UsageLevel};

    #[test]
    fn the_page_reads_camel_case() {
        let window = serde_json::to_value(WatchedWindow {
            percent: 81.0,
            resets_at: None,
            resets_in: Some("2 h 14 min".into()),
            level: UsageLevel::Orange,
            zone: PacingZone::OnTrack,
        })
        .unwrap();
        assert_eq!(window["resetsIn"], "2 h 14 min");
        assert_eq!(window["level"], "orange");
        assert_eq!(window["zone"], "onTrack");
        let view = serde_json::to_value(UsageView {
            dashboard: None,
            generated_at: None,
            sources: Vec::new(),
            models: Vec::new(),
            has_data: false,
        })
        .unwrap();
        assert_eq!(view["hasData"], false);
    }
}
