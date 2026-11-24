use anyhow::Result;
use serde_json::json;
use veronica_core::AppDirectories;
use veronica_system::notify::{self, Notification, Urgency};
use veronica_usage::alerts::NotifierState;

use crate::format::Output;

#[derive(clap::Subcommand)]
pub enum AlertsCommand {
    #[command(about = "Print what the notifier is comparing against")]
    State,
    #[command(
        about = "Post one sample banner, to confirm notifications arrive at all",
        long_about = "Post one sample banner, to confirm notifications arrive at all.\n\nDeliberately does not touch the notifier state: consuming a real edge to prove delivery works would suppress the alert being tested."
    )]
    Test,
    #[command(
        about = "Forget the remembered levels and zones",
        long_about = "Forget the remembered levels and zones.\n\nWorth doing after changing a threshold, since the stored comparison is then against the old scale."
    )]
    Clear,
}

pub async fn run(
    directories: &AppDirectories,
    command: &AlertsCommand,
    output: Output,
) -> Result<()> {
    let path = directories.alerts_state_file();

    match command {
        AlertsCommand::State => {
            let state = NotifierState::load(&path);
            output.emit(&state, || {
                use std::fmt::Write;
                let mut out = String::new();
                let _ = writeln!(out, "session level  {:?}", state.session_level);
                let _ = writeln!(out, "weekly level   {:?}", state.weekly_level);
                let _ = writeln!(out, "session pace   {:?}", state.session_pacing);
                let _ = writeln!(out, "weekly pace    {:?}", state.weekly_pacing);
                for (label, value) in [
                    ("session reminder", state.session_reminder_for),
                    ("weekly reminder ", state.weekly_reminder_for),
                    ("token expired   ", state.token_expired_at),
                ] {
                    let _ = writeln!(
                        out,
                        "{label}  {}",
                        value
                            .map(|at| at.to_rfc3339())
                            .unwrap_or_else(|| "never".into())
                    );
                }
                let _ = write!(out, "open banners   {}", state.banners.len());
                out
            })
        }

        AlertsCommand::Test => {
            let connection = zbus::Connection::session().await?;
            let id = notify::post(
                &connection,
                &Notification::new(
                    "Alerts are working",
                    "This is what a Veronica rate-limit alert looks like.",
                )
                .urgency(Urgency::Normal),
            )
            .await?;
            output.emit(&json!({ "posted": id }), || {
                format!("posted notification {id}")
            })
        }

        AlertsCommand::Clear => {
            let mut state = NotifierState::load(&path);
            state.reset_tracking();
            state.save(&path)?;
            output.emit(&json!({ "cleared": true }), || {
                "forgot the remembered levels and zones".to_string()
            })
        }
    }
}
