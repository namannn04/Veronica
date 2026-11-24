pub mod audio;
pub mod bluetooth;
pub mod color;
pub mod docker;
pub mod download;
pub mod herdr;
pub mod image_tools;
pub mod metrics;
pub mod notifications;
pub mod notify;
pub mod packages;
pub mod portal;
pub mod ports;
pub mod power;
pub mod quinjet;
pub mod screencast;
pub mod selection;
pub mod tools;

pub use bluetooth::BluetoothState;
pub use metrics::{MetricsSampler, SystemSnapshot};
pub use notifications::Notification;
pub use portal::PortalSupport;

use anyhow::Result;
use veronica_core::DesktopSession;

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

pub mod workspace;

pub mod graftcode;

pub mod agents;

pub mod private_store;

pub mod agent_tracking;

pub mod companion_ai;

mod owned_processes;
