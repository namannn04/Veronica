use anyhow::{Context, Result};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "main";

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

    open(app, route.map(|route| format!("index.html?route={route}")))
}

pub(crate) fn open(app: &tauri::AppHandle, url: Option<String>) -> Result<()> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == LABEL)
        .cloned()
        .context("main window configuration is missing")?;
    if let Some(url) = url {
        config.url = WebviewUrl::App(url.into());
    }

    app.state::<crate::state::AppState>().agents.open();
    app.state::<crate::state::AppState>().chat.open_window();
    let window = WebviewWindowBuilder::from_config(app, &config)
        .context("cannot prepare the main window")?
        .build()
        .context("cannot create the main window")?;
    window.show().context("cannot show the main window")?;
    window.set_focus().context("cannot focus the main window")?;
    Ok(())
}
