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

pub(crate) fn money(value: f64) -> String {
    format!("${value:.2}")
}

pub(crate) fn tokens(value: u64) -> String {
    veronica_usage::cards::tokens(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_always_has_two_decimals() {
        assert_eq!(money(179.7819), "$179.78");
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(5.0), "$5.00");
    }

    #[test]
    fn tokens_abbreviate_with_three_significant_figures() {
        assert_eq!(tokens(999), "999");
        assert_eq!(tokens(1_000), "1.00K");
        assert_eq!(tokens(1_050), "1.05K");
        assert_eq!(tokens(15_000), "15.0K");
        assert_eq!(tokens(502_000_260), "502M");
        assert_eq!(tokens(2_500_000_000), "2.50B");
    }

    #[test]
    fn rounding_up_to_the_next_unit_or_precision_reads_naturally() {
        assert_eq!(tokens(9_999), "10.0K");
        assert_eq!(tokens(99_999), "100K");
        assert_eq!(tokens(999_999), "1.00M");
        assert_eq!(tokens(999_499), "999K");
        assert_eq!(tokens(999_999_999), "1.00B");
    }
}
