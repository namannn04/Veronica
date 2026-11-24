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
    .title("Veronica Emoji Picker")
    .inner_size(440.0, 500.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
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
