//! Sleep and lid behaviour, through systemd-logind and the GNOME session.
//!
//! Edith calls `IOPMAssertionCreateWithName` for prevent-sleep and drives a
//! privileged helper for lid-awake. On Linux both are logind inhibitor locks:
//! `idle` stops the idle timer, and `handle-lid-switch` stops the lid from
//! suspending the machine. The lock lives as long as the file descriptor logind
//! hands back, so the descriptor must be held, not dropped.
//!
//! On GNOME a logind `idle` lock is not enough on its own. gsd-power decides
//! when to blank and suspend from gnome-session's own inhibitors, and
//! gnome-session never reads logind's idle locks back, so the screen still
//! blanks and the machine still suspends on the idle timeout. Any lock that
//! covers idle therefore also takes a session inhibitor for idle and suspend,
//! held for as long as its own session-bus connection stays open.

use anyhow::{Context, Result};
use zbus::zvariant::OwnedFd;
use zbus::Connection;

pub const LOGIND_BUS: &str = "org.freedesktop.login1";
pub const LOGIND_PATH: &str = "/org/freedesktop/login1";
pub const LOGIND_MANAGER: &str = "org.freedesktop.login1.Manager";

pub const SESSION_MANAGER_BUS: &str = "org.gnome.SessionManager";
pub const SESSION_MANAGER_PATH: &str = "/org/gnome/SessionManager";
/// `GsmInhibitorFlag` values from gnome-session's D-Bus interface.
pub const SESSION_INHIBIT_SUSPEND: u32 = 4;
pub const SESSION_INHIBIT_IDLE: u32 = 8;
/// The application id gnome-session shows against the inhibitor.
pub const APP_ID: &str = "io.github.namannn04.Veronica";

/// What a lock suppresses. `Idle` is prevent-sleep; `LidSwitch` is lid-awake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InhibitWhat {
    Idle,
    LidSwitch,
    /// Both, which is what lid-awake needs to survive a closed lid.
    IdleAndLidSwitch,
    Sleep,
}

impl InhibitWhat {
    /// Whether the lock is meant to stop the idle timer.
    pub fn covers_idle(self) -> bool {
        matches!(self, InhibitWhat::Idle | InhibitWhat::IdleAndLidSwitch)
    }

    /// logind takes a colon-separated list.
    pub fn as_str(self) -> &'static str {
        match self {
            InhibitWhat::Idle => "idle",
            InhibitWhat::LidSwitch => "handle-lid-switch",
            InhibitWhat::IdleAndLidSwitch => "idle:handle-lid-switch",
            InhibitWhat::Sleep => "sleep",
        }
    }
}

/// How strongly to hold the lock. `Block` prevents the action outright; `Delay`
/// only postpones it, which is not enough for prevent-sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InhibitMode {
    Block,
    Delay,
}

impl InhibitMode {
    pub fn as_str(self) -> &'static str {
        match self {
            InhibitMode::Block => "block",
            InhibitMode::Delay => "delay",
        }
    }
}

/// A held inhibitor lock. Dropping this releases it, so the caller must keep it
/// alive for as long as the behaviour should be suppressed.
pub struct InhibitorLock {
    what: InhibitWhat,
    reason: String,
    // The lock exists precisely because logind's descriptor is still open.
    _fd: OwnedFd,
    // gnome-session drops an inhibitor when its caller leaves the bus, so the
    // dedicated connection is the lock; closing it on drop releases it.
    session: Option<Connection>,
}

impl InhibitorLock {
    pub fn what(&self) -> InhibitWhat {
        self.what
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Whether the GNOME session also honours this lock.
    pub fn holds_session_inhibitor(&self) -> bool {
        self.session.is_some()
    }
}

impl std::fmt::Debug for InhibitorLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InhibitorLock")
            .field("what", &self.what.as_str())
            .field("reason", &self.reason)
            .field("session", &self.session.is_some())
            .finish()
    }
}

/// Take an inhibitor lock from logind.
pub async fn inhibit(
    connection: &Connection,
    what: InhibitWhat,
    who: &str,
    reason: &str,
    mode: InhibitMode,
) -> Result<InhibitorLock> {
    let fd: OwnedFd = connection
        .call_method(
            Some(LOGIND_BUS),
            LOGIND_PATH,
            Some(LOGIND_MANAGER),
            "Inhibit",
            &(what.as_str(), who, reason, mode.as_str()),
        )
        .await
        .context("logind refused the inhibitor request")?
        .body()
        .deserialize()
        .context("logind returned no inhibitor descriptor")?;

    // Best effort: a desktop without gnome-session has nothing to tell, and
    // the logind lock is then the whole of the behaviour.
    let session = if what.covers_idle() {
        match inhibit_session(reason).await {
            Ok(connection) => Some(connection),
            Err(error) => {
                tracing::debug!("no session inhibitor: {error:#}");
                None
            }
        }
    } else {
        None
    };

    Ok(InhibitorLock {
        what,
        reason: reason.to_string(),
        _fd: fd,
        session,
    })
}

/// Ask gnome-session to inhibit idle and suspend, on a connection of its own.
async fn inhibit_session(reason: &str) -> Result<Connection> {
    // `Connection::session()` opens a new connection rather than sharing one,
    // which is what ties the inhibitor's lifetime to this lock alone.
    let connection = Connection::session()
        .await
        .context("cannot reach the session bus")?;
    let _cookie: u32 = connection
        .call_method(
            Some(SESSION_MANAGER_BUS),
            SESSION_MANAGER_PATH,
            Some(SESSION_MANAGER_BUS),
            "Inhibit",
            &(
                APP_ID,
                0u32,
                reason,
                SESSION_INHIBIT_IDLE | SESSION_INHIBIT_SUSPEND,
            ),
        )
        .await
        .context("gnome-session refused the inhibitor request")?
        .body()
        .deserialize()
        .context("gnome-session returned no inhibitor cookie")?;
    Ok(connection)
}

/// Whether the machine has a lid at all, so the UI can hide lid-awake on a
/// desktop rather than offering a control that cannot work.
pub fn has_lid() -> bool {
    std::fs::read_dir("/proc/acpi/button/lid")
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inhibit_targets_use_the_names_logind_expects() {
        assert_eq!(InhibitWhat::Idle.as_str(), "idle");
        assert_eq!(InhibitWhat::LidSwitch.as_str(), "handle-lid-switch");
        assert_eq!(InhibitWhat::Sleep.as_str(), "sleep");
    }

    #[test]
    fn lid_awake_blocks_both_idle_and_the_lid_switch() {
        // Blocking only the lid still lets the idle timer suspend the machine,
        // so lid-awake has to hold both in one lock.
        let combined = InhibitWhat::IdleAndLidSwitch.as_str();
        assert!(combined.contains("idle"));
        assert!(combined.contains("handle-lid-switch"));
        assert_eq!(combined, "idle:handle-lid-switch");
    }

    #[test]
    fn only_idle_locks_also_hold_the_gnome_session() {
        // gsd-power ignores logind idle locks, so these need the session too.
        assert!(InhibitWhat::Idle.covers_idle());
        assert!(InhibitWhat::IdleAndLidSwitch.covers_idle());
        assert!(!InhibitWhat::LidSwitch.covers_idle());
        assert!(!InhibitWhat::Sleep.covers_idle());
        assert_eq!(SESSION_INHIBIT_IDLE | SESSION_INHIBIT_SUSPEND, 12);
    }

    #[test]
    fn modes_use_logind_names() {
        assert_eq!(InhibitMode::Block.as_str(), "block");
        assert_eq!(InhibitMode::Delay.as_str(), "delay");
    }
}
