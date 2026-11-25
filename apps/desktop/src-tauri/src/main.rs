mod commands;
mod emoji_picker;
mod extensions;
mod file_shelf;
mod image_tools;
mod logging;
mod main_window;
mod notch_tools;
mod presenter;
mod shortcuts;
mod single_instance;
mod state;
mod tray;

use anyhow::Result;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;
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

fn modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
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
        .manage(shortcuts::Shortcuts::default())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        shortcuts::fire(app, shortcut);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::agent_tracking,
            commands::agent_profiles, commands::agent_profiles_save, commands::agent_start, commands::agent_list, commands::agent_external, commands::agent_output, commands::agent_input, commands::agent_resize, commands::agent_stop,
            commands::diagnostics,
            commands::capabilities,
            commands::settings_all,
            commands::settings_set,
            commands::launch_at_login_status,
            commands::launch_at_login_set,
            commands::shell_extension_status,
            commands::shell_extension_enable,
            commands::backup_export,
            commands::backup_inspect,
            commands::backup_import,
            commands::shell_action,
            commands::power_status,
            commands::update_check,
            commands::update_install,
            commands::app_restart,
            commands::tools_readiness,
            commands::tools_install,
            commands::emoji_groups,
            commands::emoji_search,
            commands::emoji_recents,
            commands::emoji_copy,
            commands::presenter_state,
            commands::presenter_set,
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
            extensions::commands::ext_auto_update,
            extensions::commands::ext_auto_update_set,
            extensions::commands::ext_onboarding_detect,
            commands::shortcuts_list,
            extensions::commands::ext_surfaces,
            extensions::commands::ext_surface_action,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            app.manage(state);
            host.attach(std::sync::Arc::new(extensions::shell::AppShell::new(handle.clone())));
            extensions::topics::watch(&handle);
            let settings_file = directories.settings_file();
            tauri::async_runtime::spawn(async move {
                host.start().await;
                if let Ok(settings) = veronica_core::Settings::load(&settings_file) {
                    extensions::updates::install_bundled(&host, settings).await;
                }
                extensions::updates::run(host, settings_file).await;
            });

            if let Some(guard) = instance {
                guard.attach(&handle);
                app.manage(guard);
            }

            tray::install(&handle)?;

            shortcuts::sync(&handle, &handle.state::<AppState>().settings_snapshot());

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
                let installed = watch_settings
                    .state::<AppState>()
                    .extensions
                    .layout()
                    .installed_file();
                let mut installed_at = modified(&installed);
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
                    let reinstalled = modified(&installed);
                    if changed || reinstalled != installed_at {
                        installed_at = reinstalled;
                        shortcuts::sync(&watch_settings, &settings);
                    }
                    if changed {
                        let host =
                            std::sync::Arc::clone(&watch_settings.state::<AppState>().extensions);
                        let wanted = settings.clone();
                        tauri::async_runtime::spawn(async move {
                            extensions::updates::install_bundled(&host, wanted).await;
                        });
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
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested {..} | WindowEvent::Destroyed) { window.app_handle().state::<AppState>().agents.stop_all(); }
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
            RunEvent::Exit => { tauri::async_runtime::block_on(app.state::<AppState>().extensions.stop()); app.state::<AppState>().agents.stop_all(); },
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
        assert!(!handled.is_empty());
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
}
