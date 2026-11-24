use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use veronica_core::PresenterState;

use crate::state::AppState;

const POLL: Duration = Duration::from_secs(2);

const IDLE_CHECK: Duration = Duration::from_secs(5);

pub async fn run(app: AppHandle) {
    loop {
        let enabled = {
            let state = app.state::<AppState>();
            state.settings_snapshot().bool_or("presenterEnabled", false)
        };

        if !enabled {
            clear_detection(&app);
            tokio::time::sleep(IDLE_CHECK).await;
            continue;
        }

        tick(&app).await;
        tokio::time::sleep(POLL).await;
    }
}

async fn tick(app: &AppHandle) {
    let share = veronica_system::screencast::detect().await;
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();

    let was_active = settings.bool_or("presenterAutoActive", false);
    let was_reason = settings.string("presenterAutoReason").unwrap_or_default();
    let reason = share.reason.clone().unwrap_or_default();

    if was_active == share.sharing && was_reason == reason {
        return;
    }

    if let Err(error) = state.set_setting("presenterAutoActive", share.sharing.into()) {
        tracing::warn!(target: "veronica", "cannot record the share state: {error:#}");
        return;
    }
    if let Err(error) = state.set_setting("presenterAutoReason", reason.clone().into()) {
        tracing::warn!(target: "veronica", "cannot record the share reason: {error:#}");
    }

    if !share.sharing && settings.bool_or("presenterAutoPaused", false) {
        if let Err(error) = state.set_setting("presenterAutoPaused", false.into()) {
            tracing::warn!(target: "veronica", "cannot clear the dismissal: {error:#}");
        }
    }

    tracing::info!(
        target: "veronica",
        "screen sharing {}",
        if share.sharing { "started" } else { "stopped" }
    );
    let _ = app.emit("settings-updated", "presenter");
}

fn clear_detection(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    if !settings.bool_or("presenterAutoActive", false)
        && settings
            .string("presenterAutoReason")
            .unwrap_or_default()
            .is_empty()
    {
        return;
    }
    for (key, value) in [
        ("presenterAutoActive", serde_json::Value::Bool(false)),
        (
            "presenterAutoReason",
            serde_json::Value::String(String::new()),
        ),
        ("presenterAutoPaused", serde_json::Value::Bool(false)),
    ] {
        if let Err(error) = state.set_setting(key, value) {
            tracing::warn!(target: "veronica", "cannot clear {key}: {error:#}");
            return;
        }
    }
    let _ = app.emit("settings-updated", "presenter");
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenterView {
    #[serde(flatten)]
    pub state: PresenterState,
    pub active: bool,
    pub blurred_classes: Vec<&'static str>,
    pub share: veronica_system::screencast::ScreenShareState,
}

pub fn view(
    state: &AppState,
    share: veronica_system::screencast::ScreenShareState,
) -> PresenterView {
    let presenter = PresenterState::read(&state.settings_snapshot());
    PresenterView {
        active: presenter.active(),
        blurred_classes: veronica_core::BlurCategory::ALL
            .into_iter()
            .filter(|category| presenter.blurs(*category))
            .map(|category| category.css_class())
            .collect(),
        state: presenter,
        share,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veronica_core::{BlurCategory, Settings};

    fn presenter(pairs: &[(&str, serde_json::Value)]) -> PresenterState {
        let mut settings = Settings::default();
        for (key, value) in pairs {
            settings.set(key, value.clone());
        }
        PresenterState::read(&settings)
    }

    #[test]
    fn the_poll_is_frequent_enough_to_catch_the_start_of_a_call() {
        assert!(POLL.as_secs() <= 3, "poll was {:?}", POLL);
        assert!(
            IDLE_CHECK >= POLL,
            "idling should be no busier than working"
        );
    }

    #[test]
    fn blurred_classes_follow_the_selected_categories() {
        let state = presenter(&[
            ("presenterEnabled", serde_json::json!(true)),
            ("presenterMode", serde_json::json!(true)),
            ("presenterBlurMusic", serde_json::json!(false)),
        ]);
        let classes: Vec<&str> = BlurCategory::ALL
            .into_iter()
            .filter(|category| state.blurs(*category))
            .map(|category| category.css_class())
            .collect();
        assert!(classes.contains(&"blur-money"));
        assert!(!classes.contains(&"blur-music"));
        assert_eq!(classes.len(), BlurCategory::ALL.len() - 1);
    }

    #[test]
    fn an_inactive_presenter_blurs_nothing() {
        let state = presenter(&[("presenterEnabled", serde_json::json!(true))]);
        assert!(!state.active());
        assert!(BlurCategory::ALL
            .into_iter()
            .all(|category| !state.blurs(category)));
    }
}
