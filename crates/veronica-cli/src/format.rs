use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use veronica_protocol::{canonical, ExitCode};

pub(crate) use veronica_sdk::format::{countdown, table};

static DOCUMENT_WRITTEN: AtomicBool = AtomicBool::new(false);

pub(crate) fn document(value: &Value) {
    DOCUMENT_WRITTEN.store(true, Ordering::Relaxed);
    print!("{}", canonical::value_to_string_pretty(value));
}

pub(crate) fn error_document(code: ExitCode, message: &str) -> Value {
    json!({ "error": { "code": code.code(), "message": message } })
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) code: ExitCode,
    pub(crate) message: String,
}

impl Failure {
    pub(crate) fn new(code: ExitCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

pub(crate) fn confirmation<T: Serialize>(
    output: Output,
    preview: &T,
    text: impl FnOnce() -> String,
) -> anyhow::Error {
    let rerun = crate::routes::rerun(&crate::routes::arguments());
    match output {
        Output::Json => match serde_json::to_value(preview) {
            Ok(preview) => document(&json!({
                "confirmationRequired": true,
                "preview": preview,
                "rerun": rerun,
            })),
            Err(error) => return error.into(),
        },
        Output::Text => {
            println!(
                "{}\n\nNothing was changed. To go ahead, run:\n  {rerun}",
                text()
            );
        }
    }
    Failure::new(ExitCode::ConfirmationRequired, "confirmation required").into()
}

pub(crate) fn fail(json: bool, code: ExitCode, message: &str) -> ExitCode {
    eprintln!("vr: {message}");
    if json && !DOCUMENT_WRITTEN.load(Ordering::Relaxed) {
        document(&error_document(code, message));
    }
    code
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Output {
    Text,
    Json,
}

impl Output {
    pub(crate) fn new(json: bool) -> Self {
        if json {
            Self::Json
        } else {
            Self::Text
        }
    }

    pub(crate) fn emit<T: Serialize>(self, value: &T, text: impl FnOnce() -> String) -> Result<()> {
        match self {
            Self::Json => document(&serde_json::to_value(value)?),
            Self::Text => {
                let rendered = text();
                if !rendered.is_empty() {
                    println!("{rendered}");
                }
            }
        }
        Ok(())
    }
}
