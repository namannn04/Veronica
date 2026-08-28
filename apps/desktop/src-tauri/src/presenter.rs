//! Presenter mode's detector.
//!
//! `veronica-core` owns the model — the gate and the blur categories — and
//! `veronica-system` owns the detection. This connects them: while presenter
//! mode is enabled it watches for a screencast session and records what it finds
//! in the settings, under the same keys Edith uses.
//!
//! The detection result is written to settings rather than held in memory
//! because three processes need it: the app blurs its own figures, the GNOME
//! extension blurs the notch's, and `vr presenter status` reports it. One file
//! they all read is the only way those three cannot disagree.
//!
//! It costs nothing when the feature is off: no poll runs until
//! `presenterEnabled` is set.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use veronica_core::PresenterState;

use crate::state::AppState;

/// How often to look for a screencast session. A local D-Bus introspection, so
/// this is cheap; the interval is short because presenter mode is worthless if
/// it activates ten seconds into a call.
const POLL: Duration = Duration::from_secs(2);

/// How often to re-check the switch while the feature is off.
const IDLE_CHECK: Duration = Duration::from_secs(5);

pub async fn run(app: AppHandle) {
    loop {
        let enabled = {
            let state = app.state::<AppState>();
            state.settings_snapshot().bool_or("presenterEnabled", false)
        };

        if !enabled {
            // Leaving a stale "a share is happening" behind would blur the notch
            // forever after the feature is switched off.
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

    // A dismissal applies to one share. Once it ends, the next one must blur
    // again, or dismissing once would disable detection permanently.
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

/// Drop any recorded detection, so switching the feature off unblurs at once.
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

/// The resolved state, plus what detection currently sees.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenterView {
    #[serde(flatten)]
    pub state: PresenterState,
    /// True while the gate says to blur.
    pub active: bool,
    /// The CSS classes the interface should switch on, one per blurred category.
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
        // Presenter mode that activates half a minute in has already leaked the
        // figures it exists to hide.
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
