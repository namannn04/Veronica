//! Veronica — native control center for Ubuntu.

// Tauri's own main is the entry point; there is no console window to hide on
// Linux, so no windows_subsystem attribute is needed.
mod art;
mod commands;
mod state;
mod tray;

use anyhow::Result;
use tauri::{Manager, WindowEvent};
use veronica_core::AppDirectories;

use state::AppState;

fn main() {
    init_logging();
    force_x11_backend();

    if let Err(error) = run() {
        eprintln!("veronica: {error:#}");
        std::process::exit(1);
    }
}

fn init_logging() {
    let filter = std::env::var("VERONICA_LOG").unwrap_or_else(|_| "warn,veronica=info".to_string());
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
}

/// Ask GTK for the X11 backend before it initialises.
///
/// The notch overlay has to place itself at the top centre of the display and
/// stay above other windows. A Wayland client cannot position its own toplevel
/// or raise itself, so on a Wayland session the overlay would appear wherever
/// the compositor decided. Under XWayland those hints work, and GNOME sessions
/// always run XWayland.
///
/// Set `VERONICA_GDK_BACKEND=wayland` to override, accepting that the notch will
/// not be positionable.
fn force_x11_backend() {
    if let Ok(override_backend) = std::env::var("VERONICA_GDK_BACKEND") {
        std::env::set_var("GDK_BACKEND", override_backend);
        return;
    }
    // Respect an existing choice rather than overriding the user's environment.
    if std::env::var_os("GDK_BACKEND").is_some() {
        return;
    }
    // Only meaningful when an X display is actually reachable.
    if std::env::var_os("DISPLAY").is_some() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

fn run() -> Result<()> {
    let directories = AppDirectories::current()?;
    directories.prepare()?;

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::diagnostics,
            commands::capabilities,
            commands::usage_view,
            commands::usage_refresh,
            commands::settings_all,
            commands::settings_set,
            commands::system_snapshot,
            commands::microphone_state,
            commands::microphone_toggle,
            commands::media_now_playing,
            commands::media_control,
            commands::calendar_agenda,
            commands::machines_probe,
            commands::machines_add,
            commands::machines_remove,
            commands::machines_discover,
            commands::notifications_list,
            commands::notifications_dismiss,
            commands::notifications_clear,
            commands::show_main_window,
            commands::open_external,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // Probing the desktop portal needs D-Bus, so the session is
            // resolved on the async runtime and the state is seeded with the
            // synchronous answer first.
            let session = veronica_core::DesktopSession::detect();
            let state = AppState::new(directories.clone(), session)?;
            app.manage(state);

            tray::install(&handle)?;

            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
            }

            // Watch the bus for notifications. This runs for the process's
            // lifetime; if the bus refuses monitoring, the feature is simply
            // absent rather than fatal.
            let watcher = handle.clone();
            tauri::async_runtime::spawn(async move {
                let emitter = watcher.clone();
                let result = veronica_system::notifications::watch(move |notification| {
                    use tauri::Emitter;
                    let state = emitter.state::<AppState>();
                    state.push_notification(notification.clone());
                    if let Err(error) = emitter.emit("notifications-received", notification) {
                        tracing::warn!(target: "veronica", "cannot emit notification: {error}");
                    }
                })
                .await;
                if let Err(error) = result {
                    tracing::info!("notification history unavailable: {error:#}");
                }
            });

            let refine = handle.clone();
            tauri::async_runtime::spawn(async move {
                let resolved = veronica_system::detect_session().await;
                let state = refine.state::<AppState>();
                *state.session.lock().expect("session lock") = resolved;
                // Capability-dependent screens re-read once the probe lands.
                use tauri::Emitter;
                let _ = refine.emit("session-resolved", ());
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Veronica lives in the tray, so closing the main window hides
                // it instead of quitting and losing the collector schedule.
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())?;

    Ok(())
}
