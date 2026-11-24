use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use tauri::{AppHandle, Listener, Manager};

const REPORT_FILE: &str = "VERONICA_FRAME_PROBE";
const TARGET: &str = "VERONICA_FRAME_PROBE_TARGET";
const DEFAULT_TARGET: &str = "hostile/index.html";
const REPORT_EVENT: &str = "extension-frame-probe";
const ESCAPE_EVENT: &str = "extension-frame-escape";

pub(crate) struct Probe {
    report: PathBuf,
    extension: String,
    entry: String,
}

pub(crate) fn requested() -> Option<Probe> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let report = PathBuf::from(std::env::var_os(REPORT_FILE)?);
    let target = std::env::var(TARGET).unwrap_or_else(|_| DEFAULT_TARGET.to_string());
    let (extension, entry) = target.split_once('/')?;
    Some(Probe {
        report,
        extension: extension.to_string(),
        entry: entry.to_string(),
    })
}

impl Probe {
    pub(crate) fn start(self, app: &AppHandle) -> Result<()> {
        let escapes = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&escapes);
        app.listen_any(ESCAPE_EVENT, move |_event| {
            counted.fetch_add(1, Ordering::SeqCst);
        });

        let handle = app.clone();
        let report = self.report.clone();
        app.listen_any(REPORT_EVENT, move |event| {
            let outcome = serde_json::json!({
                "frontend": serde_json::from_str::<serde_json::Value>(event.payload())
                    .unwrap_or(serde_json::Value::Null),
                "escapeEvents": escapes.load(Ordering::SeqCst),
                "windows": window_urls(&handle),
            });
            if let Err(error) = veronica_core::fsutil::write_atomic(&report, outcome.to_string().as_bytes()) {
                tracing::error!(target: "veronica", "cannot write the frame probe report: {error:#}");
            }
            handle.exit(0);
        });

        let url = format!(
            "index.html?surface=frame-probe&extension={}&entry={}",
            self.extension, self.entry
        );
        crate::main_window::open(app, Some(url)).context("cannot open the frame probe")
    }
}

fn window_urls(app: &AppHandle) -> serde_json::Value {
    app.webview_windows()
        .into_iter()
        .map(|(label, window)| {
            let url = window.url().map(|url| url.to_string()).unwrap_or_default();
            (label, serde_json::Value::String(url))
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}
