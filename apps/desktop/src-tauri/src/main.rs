//! Veronica — native control center for Ubuntu.

// Tauri's own main is the entry point; there is no console window to hide on
// Linux, so no windows_subsystem attribute is needed.
mod alerts;
mod art;
mod commands;
mod presenter;
mod state;
mod tray;

use anyhow::Result;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
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

fn matches(shortcut: &Shortcut, code: Code) -> bool {
    shortcut.matches(Modifiers::CONTROL | Modifiers::ALT, code)
}

#[derive(Clone, Copy)]
struct ShortcutBinding {
    accelerator: &'static str,
    extension_id: Option<&'static str>,
}

const SHORTCUTS: [ShortcutBinding; 6] = [
    ShortcutBinding {
        accelerator: "Ctrl+Alt+V",
        extension_id: None,
    },
    ShortcutBinding {
        accelerator: "Ctrl+Alt+B",
        extension_id: Some("clipboard"),
    },
    ShortcutBinding {
        accelerator: "Ctrl+Alt+M",
        extension_id: Some("micMute"),
    },
    ShortcutBinding {
        accelerator: "Ctrl+Alt+P",
        extension_id: Some("colorPicker"),
    },
    ShortcutBinding {
        accelerator: "Ctrl+Alt+K",
        extension_id: None,
    },
    ShortcutBinding {
        accelerator: "Ctrl+Alt+E",
        extension_id: Some("emoji"),
    },
];

fn extension_enabled(settings: &veronica_core::Settings, id: &str) -> bool {
    veronica_core::extensions::entry(id).is_some_and(|entry| settings.extension_enabled(entry))
}

fn shortcut_enabled(settings: &veronica_core::Settings, binding: ShortcutBinding) -> bool {
    binding
        .extension_id
        .is_none_or(|id| extension_enabled(settings, id))
}

/// Match registered accelerators to the shared extension switches.
///
/// Each operation is independent because one compositor collision must not
/// prevent the other five shortcuts from following their settings.
pub(crate) fn sync_global_shortcuts(app: &tauri::AppHandle, settings: &veronica_core::Settings) {
    let manager = app.global_shortcut();
    for binding in SHORTCUTS {
        let wanted = shortcut_enabled(settings, binding);
        let registered = manager.is_registered(binding.accelerator);
        let result = match (wanted, registered) {
            (true, false) => manager.register(binding.accelerator),
            (false, true) => manager.unregister(binding.accelerator),
            _ => continue,
        };
        if let Err(error) = result {
            tracing::warn!(
                target: "veronica",
                "cannot {} {}: {error}",
                if wanted { "register" } else { "unregister" },
                binding.accelerator
            );
        }
    }
}

fn show_route(app: &tauri::AppHandle, route: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    let _ = app.emit("navigate", route);
}

fn handle_shortcut(app: &tauri::AppHandle, shortcut: &Shortcut) {
    let settings = app.state::<AppState>().settings_snapshot();
    if matches(shortcut, Code::KeyV) {
        show_route(app, "home");
    } else if matches(shortcut, Code::KeyB) && extension_enabled(&settings, "clipboard") {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            if commands::call_shell_method("ShowClipboard").await.is_err() {
                show_route(&handle, "clipboard");
            }
        });
    } else if matches(shortcut, Code::KeyM) && extension_enabled(&settings, "micMute") {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            match veronica_system::audio::toggle_microphone().await {
                Ok(state) => {
                    let _ = handle.emit("microphone-updated", state);
                }
                Err(error) => tracing::warn!(target: "veronica", "mic shortcut failed: {error:#}"),
            }
        });
    } else if matches(shortcut, Code::KeyP) && extension_enabled(&settings, "colorPicker") {
        tauri::async_runtime::spawn(async {
            if let Err(error) = commands::call_shell_method("PickColor").await {
                tracing::warn!(target: "veronica", "color shortcut failed: {error}");
            }
        });
    } else if matches(shortcut, Code::KeyE) && extension_enabled(&settings, "emoji") {
        show_route(app, "emoji");
    } else if matches(shortcut, Code::KeyK) {
        tauri::async_runtime::spawn(async {
            if let Err(error) = commands::call_shell_method("CleanKeys").await {
                tracing::warn!(target: "veronica", "clean-keys shortcut failed: {error}");
            }
        });
    }
}

fn run() -> Result<()> {
    let directories = AppDirectories::current()?;
    directories.prepare()?;

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        handle_shortcut(app, shortcut);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::diagnostics,
            commands::capabilities,
            commands::usage_view,
            commands::usage_refresh,
            commands::usage_export,
            commands::usage_limits,
            commands::settings_all,
            commands::settings_set,
            commands::backup_export,
            commands::backup_inspect,
            commands::backup_import,
            commands::companion_list,
            commands::companion_note_create,
            commands::companion_update,
            commands::companion_remove,
            commands::companion_recording_status,
            commands::companion_record_start,
            commands::companion_record_stop,
            commands::companion_record_cancel,
            commands::companion_audio,
            commands::attention_status,
            commands::attention_start,
            commands::attention_stop,
            commands::attention_history,
            commands::attention_overview,
            commands::attention_settings,
            commands::attention_settings_save,
            commands::shell_action,
            commands::system_snapshot,
            commands::power_status,
            commands::update_check,
            commands::system_processes,
            commands::system_quit_process,
            commands::audit_site,
            commands::database_connections,
            commands::database_test,
            commands::database_browse,
            commands::database_query,
            commands::database_read,
            commands::database_capabilities,
            commands::database_operations,
            commands::packages_sources,
            commands::packages_updates,
            commands::packages_inventory,
            commands::packages_update,
            commands::packages_removal_plan,
            commands::cleaner_categories,
            commands::cleaner_scan,
            commands::cleaner_clean,
            commands::tools_readiness,
            commands::herdr_board,
            commands::herdr_open,
            commands::quinjet_projects,
            commands::quinjet_open,
            commands::microphone_state,
            commands::emoji_groups,
            commands::emoji_search,
            commands::emoji_recents,
            commands::emoji_copy,
            commands::audio_streams,
            commands::audio_stream_volume,
            commands::audio_stream_toggle_mute,
            commands::bluetooth_state,
            commands::microphone_toggle,
            commands::media_now_playing,
            commands::media_control,
            commands::music_library,
            commands::calendar_agenda,
            commands::calendar_open,
            commands::machines_probe,
            commands::machines_add,
            commands::machines_remove,
            commands::machines_discover,
            commands::machines_terminal,
            commands::machines_files,
            commands::machines_file_download,
            commands::machines_containers,
            commands::machines_container_action,
            commands::machines_container_logs,
            commands::machines_power,
            commands::machines_wake,
            commands::clipboard_list,
            commands::clipboard_remove,
            commands::clipboard_clear,
            commands::presenter_state,
            commands::presenter_set,
            commands::alerts_view,
            commands::alerts_test,
            commands::alerts_reset,
            commands::color_formats,
            commands::color_swatches,
            commands::color_pick,
            commands::color_copy,
            commands::color_forget,
            commands::color_clear,
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

            // Registration can be refused by the compositor or collide with
            // another app; each failure stays non-fatal and the other bindings
            // still get their chance.
            sync_global_shortcuts(&handle, &handle.state::<AppState>().settings_snapshot());

            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
            }

            // The shell extension writes through `vr config set`, outside
            // Tauri's IPC. Re-read that small JSON file so notch quick actions
            // acquire and release the same process-owned inhibitor locks as
            // switches clicked in the desktop app. The first interval tick is
            // immediate, which also restores both locks after a restart.
            let watch_settings = handle.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
                loop {
                    interval.tick().await;
                    let path = watch_settings
                        .state::<AppState>()
                        .directories
                        .settings_file();
                    let Ok(settings) = veronica_core::Settings::load(&path) else {
                        continue;
                    };
                    // A timed session's deadline lives in the file rather than
                    // in a timer, so it survives a restart. This tick is what
                    // enforces it: an expired pair is cleared once, here, and
                    // the switches below then see the session as over.
                    let now = chrono::Utc::now().timestamp_millis();
                    let mut settings = settings;
                    let mut expired = false;
                    for switch in [
                        veronica_core::Awake::KeepAwake,
                        veronica_core::Awake::LidAwake,
                    ] {
                        if veronica_core::AwakeState::read(&settings, switch).has_expired(now) {
                            settings.set(switch.enabled_key(), serde_json::Value::Bool(false));
                            settings.set(switch.until_key(), serde_json::Value::Null);
                            expired = true;
                        }
                    }
                    if expired {
                        if let Err(error) = settings.save(&path) {
                            tracing::warn!(
                                target: "veronica",
                                "cannot end an expired awake session: {error:#}"
                            );
                        }
                    }

                    let lid_awake =
                        veronica_core::AwakeState::read(&settings, veronica_core::Awake::LidAwake)
                            .is_active(now);
                    let prevent_sleep =
                        veronica_core::AwakeState::read(&settings, veronica_core::Awake::KeepAwake)
                            .is_active(now);
                    let changed = {
                        let state = watch_settings.state::<AppState>();
                        let mut current = state.settings.lock().expect("settings lock");
                        let changed = *current != settings;
                        *current = settings.clone();
                        changed
                    };
                    if let Err(error) = commands::sync_lid_awake(&watch_settings, lid_awake).await {
                        tracing::warn!(target: "veronica", "cannot apply Lid Awake: {error}");
                    }
                    if let Err(error) =
                        commands::sync_prevent_sleep(&watch_settings, prevent_sleep).await
                    {
                        tracing::warn!(target: "veronica", "cannot apply Keep Awake: {error}");
                    }
                    if changed {
                        sync_global_shortcuts(&watch_settings, &settings);
                        let _ = watch_settings.emit("settings-updated", "external");
                    }
                }
            });

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

            // Rate-limit alerts. Idles cheaply until the user turns them on,
            // so a fresh install makes no provider requests of its own.
            let alerting = handle.clone();
            tauri::async_runtime::spawn(async move {
                alerts::run(alerting).await;
            });

            // Presenter mode's screen-share detector. Also idles until the
            // feature is switched on.
            let presenting = handle.clone();
            tauri::async_runtime::spawn(async move {
                presenter::run(presenting).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wanted(settings: &veronica_core::Settings) -> Vec<&'static str> {
        SHORTCUTS
            .iter()
            .copied()
            .filter(|binding| shortcut_enabled(settings, *binding))
            .map(|binding| binding.accelerator)
            .collect()
    }

    #[test]
    fn default_shortcuts_follow_the_catalogues_featured_switches() {
        assert_eq!(
            wanted(&veronica_core::Settings::default()),
            ["Ctrl+Alt+V", "Ctrl+Alt+B", "Ctrl+Alt+K"]
        );
    }

    #[test]
    fn extension_switches_add_and_remove_their_shortcuts() {
        let mut settings = veronica_core::Settings::default();
        settings.set("clipboardEnabled", serde_json::Value::Bool(false));
        settings.set("micMuteEnabled", serde_json::Value::Bool(true));
        settings.set("colorPickerEnabled", serde_json::Value::Bool(true));
        settings.set("emojiEnabled", serde_json::Value::Bool(true));

        assert_eq!(
            wanted(&settings),
            [
                "Ctrl+Alt+V",
                "Ctrl+Alt+M",
                "Ctrl+Alt+P",
                "Ctrl+Alt+K",
                "Ctrl+Alt+E",
            ]
        );
    }
}
