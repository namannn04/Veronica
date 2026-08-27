//! Print whether the screen is being shared, as presenter mode sees it.
//!
//! `cargo run -p veronica-system --example detect-share`
//!
//! Useful for checking detection on a machine where presenter mode is not
//! activating: it reports the session counts and, when detection cannot run at
//! all, why.
//!
//! To exercise the positive case without actually sharing anything, create a
//! Mutter session and hold it — no stream is started, so no frames exist:
//!
//! ```text
//! gdbus call --session --dest org.gnome.Mutter.ScreenCast \
//!   --object-path /org/gnome/Mutter/ScreenCast \
//!   --method org.gnome.Mutter.ScreenCast.CreateSession "{}"
//! ```
//!
//! Mutter ties the session to the caller's bus connection, so it must be held
//! open for detection to see it.

fn main() -> anyhow::Result<()> {
    let state = tokio::runtime::Runtime::new()?.block_on(veronica_system::screencast::detect());
    println!("{}", serde_json::to_string_pretty(&state)?);
    Ok(())
}
