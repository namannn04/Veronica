//! The tray indicator.
//!
//! Edith's readout lives in the macOS menu bar. Ubuntu's equivalent is a
//! StatusNotifierItem, which GNOME exposes through the AppIndicator extension
//! that ships enabled on Ubuntu.

use anyhow::{Context, Result};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub fn install(app: &AppHandle) -> Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Veronica", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh usage", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open, &refresh, &separator, &quit])?;

    TrayIconBuilder::with_id("veronica-tray")
        .icon(app.default_window_icon().cloned().context("no app icon")?)
        .menu(&menu)
        // The left click opens the app; the menu is on right click, matching
        // how other Ubuntu indicators behave.
        .show_menu_on_left_click(false)
        .tooltip("Veronica")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "refresh" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = crate::commands::usage_refresh(handle).await {
                        tracing::warn!("tray refresh failed: {error}");
                    }
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)
        .context("cannot create the tray indicator")?;

    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
