//! One Veronica per session.
//!
//! Without a guard, opening Veronica from Activities while it already sits in
//! the tray started a second full process: a second tray icon, a second alerts
//! loop posting the same banners, shortcuts that failed to register, and a
//! second set of power inhibitors. The first process now owns a well-known name
//! on the session bus; a later launch asks it to show its window and exits.
//!
//! The name is the application id, which is also the prefix a Flatpak sandbox
//! is allowed to own.

use std::sync::{Arc, OnceLock};

use tauri::AppHandle;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

const BUS_NAME: &str = "io.github.namannn04.Veronica";
const OBJECT_PATH: &str = "/io/github/namannn04/Veronica";
const INTERFACE: &str = "io.github.namannn04.Veronica.Instance";

/// What this launch found on the bus.
pub(crate) enum Claim {
    /// This is the only Veronica; keep the guard alive for the process.
    First(Guard),
    /// Another Veronica has been asked to show itself; this one should exit.
    AlreadyRunning,
}

/// Holds the bus name for as long as it is alive.
pub(crate) struct Guard {
    _connection: zbus::Connection,
    app: Arc<OnceLock<AppHandle>>,
}

impl Guard {
    /// Hand over the app once it exists, so later launches can reach it.
    pub(crate) fn attach(&self, app: &AppHandle) {
        let _ = self.app.set(app.clone());
    }
}

struct Instance {
    app: Arc<OnceLock<AppHandle>>,
}

#[zbus::interface(name = "io.github.namannn04.Veronica.Instance")]
impl Instance {
    /// Show the window, unless the second launch was itself a background one
    /// (an autostart racing a manual launch has nothing to show).
    fn activate(&self, background: bool) {
        if background {
            return;
        }
        let Some(app) = self.app.get() else {
            // Still starting up; it opens its own window when it gets there.
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

/// Claim the session, or hand this launch to the Veronica that already has it.
///
/// `None` means there is no usable session bus, and the app runs unguarded
/// rather than refusing to start.
pub(crate) fn claim(background: bool) -> Option<Claim> {
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
            .request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())
            .await
        {
            Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => false,
            Ok(RequestNameReply::Exists | RequestNameReply::InQueue) => true,
            // zbus reports an owned name as an error rather than a reply.
            Err(zbus::Error::NameTaken) => true,
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
        match connection
            .call_method(
                Some(BUS_NAME),
                OBJECT_PATH,
                Some(INTERFACE),
                "Activate",
                &(background,),
            )
            .await
        {
            Ok(_) => Some(Claim::AlreadyRunning),
            Err(error) => {
                // The owner is wedged or is something else entirely; starting
                // is more useful than silently doing nothing.
                tracing::warn!("Veronica is already running but did not answer: {error}");
                None
            }
        }
    })
}
