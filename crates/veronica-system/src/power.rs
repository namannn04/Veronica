use anyhow::{Context, Result};
use zbus::{Connection, OwnedFd};

pub const LOGIND_BUS: &str = "org.freedesktop.login1";
pub const LOGIND_PATH: &str = "/org/freedesktop/login1";
pub const LOGIND_MANAGER: &str = "org.freedesktop.login1.Manager";

pub const SESSION_MANAGER_BUS: &str = "org.gnome.SessionManager";
pub const SESSION_MANAGER_PATH: &str = "/org/gnome/SessionManager";
pub const SESSION_INHIBIT_SUSPEND: u32 = 4;
pub const SESSION_INHIBIT_IDLE: u32 = 8;
pub const APP_ID: &str = "io.github.namannn04.Veronica";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InhibitWhat {
    Idle,
    LidSwitch,
    IdleAndLidSwitch,
    Sleep,
}

impl InhibitWhat {
    pub fn covers_idle(self) -> bool {
        matches!(self, Self::Idle | Self::IdleAndLidSwitch)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::LidSwitch => "handle-lid-switch",
            Self::IdleAndLidSwitch => "idle:handle-lid-switch",
            Self::Sleep => "sleep",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InhibitMode {
    Block,
    Delay,
}

impl InhibitMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Delay => "delay",
        }
    }
}

pub struct InhibitorLock {
    what: InhibitWhat,
    reason: String,
    _logind_lock_fd: OwnedFd,
    gnome_session_lock: Option<Connection>,
}

impl InhibitorLock {
    pub fn what(&self) -> InhibitWhat {
        self.what
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn holds_session_inhibitor(&self) -> bool {
        self.gnome_session_lock.is_some()
    }
}

impl std::fmt::Debug for InhibitorLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InhibitorLock")
            .field("what", &self.what.as_str())
            .field("reason", &self.reason)
            .field("session", &self.gnome_session_lock.is_some())
            .finish_non_exhaustive()
    }
}

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
        _logind_lock_fd: fd,
        gnome_session_lock: session,
    })
}

async fn inhibit_session(reason: &str) -> Result<Connection> {
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

pub fn has_lid() -> bool {
    std::fs::read_dir("/proc/acpi/button/lid").is_ok_and(|mut entries| entries.next().is_some())
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
        let combined = InhibitWhat::IdleAndLidSwitch.as_str();
        assert!(combined.contains("idle"));
        assert!(combined.contains("handle-lid-switch"));
        assert_eq!(combined, "idle:handle-lid-switch");
    }

    #[test]
    fn only_idle_locks_also_hold_the_gnome_session() {
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
