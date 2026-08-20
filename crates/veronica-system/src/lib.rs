//! Linux system integration for Veronica.
//!
//! Each module wraps one platform service that a macOS framework provides on
//! Edith: metrics from procfs, sleep control from logind, audio from PipeWire,
//! notifications and portal access from D-Bus.

pub mod audio;
pub mod metrics;
pub mod notifications;
pub mod notify;
pub mod portal;
pub mod power;

pub use metrics::{MetricsSampler, SystemSnapshot};
pub use notifications::Notification;
pub use portal::PortalSupport;

use anyhow::Result;
use veronica_core::DesktopSession;

/// Detect the session and refine it with facts that need a D-Bus round trip.
///
/// `DesktopSession::detect` is synchronous and cannot probe the portal, so the
/// capability map would otherwise report global shortcuts as unavailable even
/// where the portal implements them.
pub async fn detect_session() -> DesktopSession {
    let mut session = DesktopSession::detect();
    if !session.is_graphical() {
        return session;
    }
    match probe_portal().await {
        Ok(support) => session.has_global_shortcuts_portal = support.global_shortcuts,
        Err(error) => {
            tracing::debug!("portal probe failed: {error:#}");
        }
    }
    session
}

async fn probe_portal() -> Result<PortalSupport> {
    let connection = zbus::Connection::session().await?;
    portal::probe(&connection).await
}
