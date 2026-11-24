use std::sync::{Arc, OnceLock};

use tauri::AppHandle;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

const BUS_NAME: &str = "io.github.namannn04.Veronica";
const OBJECT_PATH: &str = "/io/github/namannn04/Veronica";
const INTERFACE: &str = "io.github.namannn04.Veronica.Instance";

pub(crate) enum Claim {
    First(Guard),
    AlreadyRunning,
}

pub(crate) struct Guard {
    _connection: zbus::Connection,
    app: Arc<OnceLock<AppHandle>>,
}

impl Guard {
    pub(crate) fn attach(&self, app: &AppHandle) {
        let _ = self.app.set(app.clone());
    }
}

struct Instance {
    app: Arc<OnceLock<AppHandle>>,
}

#[zbus::interface(name = "io.github.namannn04.Veronica.Instance")]
impl Instance {
    fn show_services(&self) {
        if let Some(app) = self.app.get() {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Err(error) = crate::main_window::show(&handle, Some("services")) {
                    tracing::warn!("cannot open developer services: {error:#}");
                }
            });
        }
    }

    fn show_notch_tools(&self) {
        if let Some(app) = self.app.get() {
            let _ = app.run_on_main_thread(move || {
                if let Err(error) = crate::notch_tools::show() {
                    tracing::warn!("cannot open notch tools: {error:#}");
                }
            });
        }
    }

    fn show_file_shelf(&self) {
        if let Some(app) = self.app.get() {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Err(error) = crate::file_shelf::show(&handle) {
                    tracing::warn!("cannot open file shelf: {error:#}");
                }
            });
        }
    }

    fn activate(&self, background: bool) {
        if background {
            return;
        }
        let Some(app) = self.app.get() else {
            return;
        };
        let target = app.clone();
        let shown = app.run_on_main_thread(move || {
            if let Err(error) = crate::main_window::show(&target, None) {
                tracing::warn!("cannot show Veronica for a second launch: {error:#}");
            }
        });
        if let Err(error) = shown {
            tracing::warn!("cannot reach the main thread for a second launch: {error}");
        }
    }
}

pub(crate) fn claim(background: bool, shelf: bool, tools: bool, services: bool) -> Option<Claim> {
    tauri::async_runtime::block_on(async move {
        let connection = match zbus::Connection::session().await {
            Ok(connection) => connection,
            Err(error) => {
                tracing::debug!("no session bus for the single-instance guard: {error}");
                return None;
            }
        };
        let app = Arc::new(OnceLock::new());
        if let Err(error) = connection
            .object_server()
            .at(OBJECT_PATH, Instance { app: app.clone() })
            .await
        {
            tracing::debug!("cannot serve the single-instance object: {error}");
            return None;
        }
        let taken = match connection
            .request_name_with_flags(BUS_NAME, RequestNameFlags::DO_NOT_QUEUE)
            .await
        {
            Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => false,
            Ok(RequestNameReply::Exists | RequestNameReply::InQueue)
            | Err(zbus::Error::NameTaken) => true,
            Err(error) => {
                tracing::debug!("cannot claim {BUS_NAME}: {error}");
                return None;
            }
        };
        if !taken {
            return Some(Claim::First(Guard {
                _connection: connection,
                app,
            }));
        }
        let result = if services {
            connection
                .call_method(
                    Some(BUS_NAME),
                    OBJECT_PATH,
                    Some(INTERFACE),
                    "ShowServices",
                    &(),
                )
                .await
        } else if tools {
            connection
                .call_method(
                    Some(BUS_NAME),
                    OBJECT_PATH,
                    Some(INTERFACE),
                    "ShowNotchTools",
                    &(),
                )
                .await
        } else if shelf {
            connection
                .call_method(
                    Some(BUS_NAME),
                    OBJECT_PATH,
                    Some(INTERFACE),
                    "ShowFileShelf",
                    &(),
                )
                .await
        } else {
            connection
                .call_method(
                    Some(BUS_NAME),
                    OBJECT_PATH,
                    Some(INTERFACE),
                    "Activate",
                    &(background,),
                )
                .await
        };
        match result {
            Ok(_) => Some(Claim::AlreadyRunning),
            Err(error) => {
                tracing::warn!("Veronica is already running but did not answer: {error}");
                None
            }
        }
    })
}
