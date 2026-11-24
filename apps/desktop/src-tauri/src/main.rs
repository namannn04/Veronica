mod alerts;
mod art;
mod commands;
mod emoji_picker;
mod extensions;
mod file_shelf;
mod image_tools;
mod logging;
mod main_window;
mod notch_tools;
mod presenter;
mod single_instance;
mod state;
mod tray;
mod usage_schedule;

use anyhow::Result;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use veronica_core::AppDirectories;

use state::AppState;

fn main() {
    logging::init();
    force_x11_backend();

    let shelf = std::env::args().any(|arg| arg == "--file-shelf");
    let tools = std::env::args().any(|arg| arg == "--notch-tools");
    let services = std::env::args().any(|arg| arg == "--services");
    let background = services || tools || shelf || is_background_launch(std::env::args_os());

    if let Err(error) = run(background, shelf, tools, services) {
        eprintln!("veronica: {error:#}");
        std::process::exit(1);
    }
}

fn is_background_launch(args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>) -> bool {
    args.into_iter()
        .any(|argument| argument.as_ref() == std::ffi::OsStr::new("--background"))
}

fn force_x11_backend() {
    if let Ok(override_backend) = std::env::var("VERONICA_GDK_BACKEND") {
        std::env::set_var("GDK_BACKEND", override_backend);
        return;
    }
    if std::env::var_os("GDK_BACKEND").is_some() {
        return;
    }
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
    if let Err(error) = main_window::show(app, Some(route)) {
        tracing::warn!(target: "veronica", "cannot open {route}: {error:#}");
    }
}

fn handle_shortcut(app: &tauri::AppHandle, shortcut: &Shortcut) {
    let settings = app.state::<AppState>().settings_snapshot();
    if matches(shortcut, Code::KeyV) {
        if let Err(error) = main_window::show(app, None) {
            tracing::warn!(target: "veronica", "cannot open Veronica: {error:#}");
        }
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
        if let Err(error) = emoji_picker::show(app) {
            tracing::warn!(target: "veronica", "cannot open the emoji picker: {error:#}");
        }
    } else if matches(shortcut, Code::KeyK) {
        tauri::async_runtime::spawn(async {
            if let Err(error) = commands::call_shell_method("CleanKeys").await {
                tracing::warn!(target: "veronica", "clean-keys shortcut failed: {error}");
            }
        });
    }
}

#[expect(
    clippy::items_after_statements,
    reason = "with tracing on, the command handler macro expands each command after its span statements"
)]
fn run(background: bool, shelf: bool, tools: bool, services: bool) -> Result<()> {
    let instance = match single_instance::claim(background, shelf, tools, services) {
        Some(single_instance::Claim::AlreadyRunning) => return Ok(()),
        Some(single_instance::Claim::First(guard)) => Some(guard),
        None => None,
    };

    let directories = AppDirectories::current()?;
    directories.prepare()?;
    if let Err(error) = commands::install_bundled_shell_extension(&directories) {
        tracing::warn!("cannot install the bundled shell extension: {error:#}");
    }

    let state = AppState::new(directories.clone(), veronica_core::DesktopSession::detect())?;
    let host = std::sync::Arc::clone(&state.extensions);
    let bundles = extensions::protocol::Bundles::discover(
        host.layout().clone(),
        std::sync::Arc::clone(host.files()),
    );

    tauri::Builder::default()
        .register_uri_scheme_protocol(extensions::protocol::SCHEME, move |_context, request| {
            bundles.respond(&request)
        })
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
            commands::workspace_doctor,
            commands::chat_journal,
            commands::chat_view,commands::chat_configure,commands::chat_enable,commands::chat_send,commands::chat_preview,commands::chat_models,commands::chat_pull,commands::chat_clear,commands::chat_export,
            commands::agent_herdr_attach,
            commands::agent_tracking,
            commands::agent_profiles, commands::agent_profiles_save, commands::agent_start, commands::agent_list, commands::agent_external, commands::agent_output, commands::agent_input, commands::agent_resize, commands::agent_stop,
            commands::graft_discover,
            commands::graft_call,
            commands::diagnostics,
            commands::capabilities,
            commands::usage_view,
            commands::usage_refresh,
            commands::usage_export,
            commands::usage_limits,
            commands::settings_all,
            commands::settings_set,
            commands::launch_at_login_status,
            commands::launch_at_login_set,
            commands::shell_extension_status,
            commands::shell_extension_enable,
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
            commands::ports_snapshot,
            commands::ports_stop,
            commands::docker_snapshot,
            commands::docker_action,
            commands::power_status,
            commands::update_check,
            commands::update_install,
            commands::app_restart,
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
            commands::tools_install,
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
            extensions::commands::ext_list,
            extensions::commands::ext_status,
            extensions::commands::ext_restart,
            extensions::commands::ext_catalog_refresh,
            extensions::commands::ext_catalog_search,
            extensions::commands::ext_catalog_info,
            extensions::commands::ext_install,
            extensions::commands::ext_sideload,
            extensions::commands::ext_update,
            extensions::commands::ext_enable,
            extensions::commands::ext_disable,
            extensions::commands::ext_remove,
            extensions::commands::ext_rollback,
            extensions::commands::ext_jobs,
            extensions::commands::ext_page_opened,
            extensions::commands::ext_page_closed,
            extensions::commands::ext_confirm_answer,
            extensions::commands::ext_bridge,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            let recordings =
                veronica_core::CompanionRepository::new(directories.companion_dir()).recordings_dir();
            if let Err(error) = std::fs::create_dir_all(&recordings)
                .map_err(anyhow::Error::from)
                .and_then(|()| {
                    app.asset_protocol_scope()
                        .allow_directory(&recordings, true)
                        .map_err(anyhow::Error::from)
                })
            {
                tracing::warn!("voice memos cannot be played in the app: {error:#}");
            }

            app.manage(state);
            host.attach(std::sync::Arc::new(extensions::shell::AppShell::new(handle.clone())));
            tauri::async_runtime::spawn(async move { host.start().await });

            if let Some(guard) = instance {
                guard.attach(&handle);
                app.manage(guard);
            }

            tray::install(&handle)?;

            sync_global_shortcuts(&handle, &handle.state::<AppState>().settings_snapshot());

            if let Some(probe) = extensions::probe::requested() {
                probe.start(&handle)?;
            } else if tools {
                notch_tools::show()?;
            } else if shelf {
                file_shelf::show(&handle)?;
            } else if services {
                main_window::show(&handle, Some("services"))?;
            } else if !background {
                main_window::show(&handle, Some("home"))?;
            }

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

            let watcher = handle.clone();
            tauri::async_runtime::spawn(async move {
                let emitter = watcher.clone();
                let result = veronica_system::notifications::watch(move |notification| {
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

            let collecting = handle.clone();
            tauri::async_runtime::spawn(async move {
                usage_schedule::run(collecting).await;
            });

            let alerting = handle.clone();
            tauri::async_runtime::spawn(async move {
                alerts::run(alerting).await;
            });

            let presenting = handle.clone();
            tauri::async_runtime::spawn(async move {
                presenter::run(presenting).await;
            });

            let refine = handle;
            tauri::async_runtime::spawn(async move {
                let resolved = veronica_system::detect_session().await;
                let state = refine.state::<AppState>();
                *state.session.lock().expect("session lock") = resolved;
                let _ = refine.emit("session-resolved", ());
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested {..} | WindowEvent::Destroyed) { window.app_handle().state::<AppState>().agents.stop_all();
                window.app_handle().state::<AppState>().chat.close_window(); }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    if let Err(error) = window.destroy() {
                        tracing::warn!(target: "veronica", "cannot release the main window: {error}");
                    }
                }
            }
        })
        .build(tauri::generate_context!())?
        .run(|app, event| match event {
            RunEvent::ExitRequested {
                code: None, api, ..
            } => api.prevent_exit(),
            RunEvent::Exit => { tauri::async_runtime::block_on(app.state::<AppState>().extensions.stop()); app.state::<AppState>().chat.close_window(); app.state::<AppState>().agents.stop_all(); commands::finish_companion_recording_on_exit(app); },
            _ => {}
        });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_explicit_background_flag_hides_the_startup_window() {
        assert!(is_background_launch(["veronica", "--background"]));
        assert!(!is_background_launch(["veronica"]));
        assert!(!is_background_launch(["veronica", "--not-background"]));
    }

    fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let from = source.find(start).expect("list start") + start.len();
        let length = source[from..].find(end).expect("list end");
        &source[from..from + length]
    }

    fn handled_commands() -> Vec<String> {
        between(include_str!("main.rs"), "generate_handler![", "])")
            .split("commands::")
            .skip(1)
            .map(|rest| {
                rest.chars()
                    .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                    .collect()
            })
            .collect()
    }

    fn quoted(source: &str) -> Vec<String> {
        source
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    }

    fn granted(capability: &str) -> Vec<String> {
        let capability: serde_json::Value = serde_json::from_str(capability).expect("capability");
        capability["permissions"]
            .as_array()
            .expect("permissions")
            .iter()
            .filter_map(|permission| permission.as_str())
            .filter_map(|permission| permission.strip_prefix("allow-"))
            .map(|command| command.replace('-', "_"))
            .collect()
    }

    #[test]
    fn every_command_is_gated_and_granted_to_the_main_window() {
        let handled = handled_commands();
        assert!(handled.len() > 100);
        assert_eq!(
            quoted(between(include_str!("../build.rs"), ".commands(&[", "])")),
            handled
        );
        assert_eq!(
            granted(include_str!("../capabilities/main-commands.json")),
            handled
        );
    }

    #[test]
    fn only_the_main_window_holds_the_full_command_grant() {
        let main: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/main-commands.json"))
                .expect("capability");
        assert_eq!(main["windows"], serde_json::json!(["main"]));
        assert!(main.get("webviews").is_none());
        assert!(main.get("remote").is_none());

        let picker = granted(include_str!("../capabilities/emoji-picker.json"));
        assert_eq!(
            picker,
            [
                "emoji_groups",
                "emoji_search",
                "emoji_recents",
                "emoji_copy",
                "settings_all"
            ]
        );
        assert!(granted(include_str!("../capabilities/default.json")).is_empty());
    }

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
