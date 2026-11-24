use serde_json::{json, Value};
use tauri::{AppHandle, Listener, Manager};
use veronica_core::Capabilities;
use veronica_protocol::ContractId;

use crate::state::AppState;

const TRIGGERS: [&str; 2] = ["settings-updated", "session-resolved"];

fn payloads(state: &AppState) -> [(&'static str, Value); 5] {
    let settings = state.settings_snapshot();
    let presenter =
        crate::presenter::view(state, veronica_system::screencast::ScreenShareState::default());
    let session = state.session.lock().expect("session lock").clone();
    let capabilities = Capabilities::resolve(&session);
    [
        ("host.presenter@1", json!({ "active": presenter.active })),
        (
            "host.privacy@1",
            json!({ "active": presenter.active, "blurred": presenter.blurred_classes }),
        ),
        (
            "host.appearance@1",
            json!({ "theme": settings.get("appearance").cloned().unwrap_or_else(|| json!("system")) }),
        ),
        (
            "host.session@1",
            serde_json::to_value(&session).unwrap_or(Value::Null),
        ),
        (
            "host.capabilities@1",
            serde_json::to_value(&capabilities).unwrap_or(Value::Null),
        ),
    ]
}

pub(crate) fn publish(app: &AppHandle) {
    let state = app.state::<AppState>();
    for (contract, payload) in payloads(&state) {
        if let Ok(contract) = contract.parse::<ContractId>() {
            state.extensions.publish_host_topic(&contract, &payload);
        }
    }
}

pub(crate) fn watch(app: &AppHandle) {
    publish(app);
    for trigger in TRIGGERS {
        let handle = app.clone();
        app.listen_any(trigger, move |_| publish(&handle));
    }
}
