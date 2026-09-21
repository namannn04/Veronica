//! Compact, shortcut-owned emoji picker.
//!
//! The full Emoji page remains useful for settings and browsing. Ctrl+Alt+E
//! should feel like a picker, though, not like opening an application, so it
//! gets a small disposable webview of its own.

use anyhow::{Context, Result};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

pub(crate) const LABEL: &str = "emoji-picker";

pub(crate) fn show(app: &tauri::AppHandle) -> Result<()> {
    if let Some(window) = app.get_webview_window(LABEL) {
        window.show().context("cannot show the emoji picker")?;
        window
            .set_focus()
            .context("cannot focus the emoji picker")?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App("index.html?surface=emoji-picker".into()),
    )
    .title("Emoji")
    .inner_size(440.0, 500.0)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .center()
    .focused(true)
    .build()
    .context("cannot create the emoji picker")?;

    window.show().context("cannot show the emoji picker")?;
    window
        .set_focus()
        .context("cannot focus the emoji picker")?;
    Ok(())
}
