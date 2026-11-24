use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use veronica_protocol::rpc::{error_code, ErrorObject, NotifyParams};
use veronica_system::notify;

use super::host::{Pending, Shell};
use crate::state::AppState;

const MAIN_WINDOW: &str = "main";

pub(crate) struct AppShell {
    app: AppHandle,
    banners: Arc<Mutex<HashMap<String, u32>>>,
}

impl AppShell {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self {
            app,
            banners: Arc::default(),
        }
    }
}

impl Shell for AppShell {
    fn emit(&self, event: &str, payload: Value) {
        if let Err(error) = self.app.emit_to(MAIN_WINDOW, event, payload) {
            tracing::warn!(target: "veronica", "cannot emit {event}: {error}");
        }
    }

    fn notify(&self, extension: &str, params: NotifyParams) -> Result<(), ErrorObject> {
        let settings = self.app.state::<AppState>().settings_snapshot();
        if !settings.bool_or("notifyMaster", true) {
            return Err(ErrorObject::new(
                error_code::UNAVAILABLE,
                "notifications are turned off",
            ));
        }
        let key = params.replace_key.map(|key| format!("{extension}/{key}"));
        let banners = Arc::clone(&self.banners);
        let extension = extension.to_string();
        tauri::async_runtime::spawn(async move {
            let replaced = key
                .as_ref()
                .and_then(|key| lock(&banners).get(key).copied())
                .unwrap_or(0);
            let notification =
                notify::Notification::new(params.title, params.body.unwrap_or_default())
                    .replacing(replaced);
            let posted = match zbus::Connection::session().await {
                Ok(connection) => notify::post(&connection, &notification).await,
                Err(error) => Err(error.into()),
            };
            match (posted, key) {
                (Ok(id), Some(key)) => {
                    lock(&banners).insert(key, id);
                }
                (Ok(_), None) => {}
                (Err(error), _) => {
                    tracing::warn!(target: "veronica", "cannot post a notification for {extension}: {error:#}");
                }
            }
        });
        Ok(())
    }

    fn open_external(&self, url: &str) -> Result<(), String> {
        veronica_core::host::spawn_detached(veronica_core::host::command("xdg-open").arg(url))
            .map(drop)
            .map_err(|error| format!("cannot open {url}: {error}"))
    }

    fn copy(&self, text: String) -> Pending<Result<(), String>> {
        Box::pin(async move {
            veronica_system::selection::write(&text)
                .await
                .map(drop)
                .map_err(|error| format!("{error:#}"))
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
