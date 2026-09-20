//! Lifecycle for the main webview.
//!
//! Veronica spends most of its idle memory in WebKit, not in the tray process.
//! The configured window is therefore a template: create it only when somebody
//! opens the app, and destroy it on close so WebKit can return that memory.

use anyhow::{Context, Result};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "main";

/// Show the main window, creating its webview on demand.
///
/// A route is put in the initial URL when a new webview is needed. That avoids
/// losing a navigation event while React is still installing its listeners.
pub(crate) fn show(app: &tauri::AppHandle, route: Option<&str>) -> Result<()> {
    if let Some(window) = app.get_webview_window(LABEL) {
        window.show().context("cannot show the main window")?;
        window
            .unminimize()
            .context("cannot restore the main window")?;
        window.set_focus().context("cannot focus the main window")?;
        if let Some(route) = route {
            window
                .emit("navigate", route)
                .context("cannot navigate the main window")?;
        }
        return Ok(());
    }

    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == LABEL)
        .cloned()
        .context("main window configuration is missing")?;
    if let Some(route) = route {
        config.url = WebviewUrl::App(format!("index.html?route={route}").into());
    }

    let window = WebviewWindowBuilder::from_config(app, &config)
        .context("cannot prepare the main window")?
        .build()
        .context("cannot create the main window")?;
    window.show().context("cannot show the main window")?;
    window.set_focus().context("cannot focus the main window")?;
    Ok(())
}
