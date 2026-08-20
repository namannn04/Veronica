//! Sleep and lid behaviour, through systemd-logind.
//!
//! Edith calls `IOPMAssertionCreateWithName` for prevent-sleep and drives a
//! privileged helper for lid-awake. On Linux both are logind inhibitor locks:
//! `idle` stops the idle timer, and `handle-lid-switch` stops the lid from
//! suspending the machine. The lock lives as long as the file descriptor logind
//! hands back, so the descriptor must be held, not dropped.

use anyhow::{Context, Result};
use zbus::zvariant::OwnedFd;
use zbus::Connection;

pub const LOGIND_BUS: &str = "org.freedesktop.login1";
pub const LOGIND_PATH: &str = "/org/freedesktop/login1";
pub const LOGIND_MANAGER: &str = "org.freedesktop.login1.Manager";

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
}

impl InhibitorLock {
    pub fn what(&self) -> InhibitWhat {
        self.what
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl std::fmt::Debug for InhibitorLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InhibitorLock")
            .field("what", &self.what.as_str())
            .field("reason", &self.reason)
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

    Ok(InhibitorLock {
        what,
        reason: reason.to_string(),
        _fd: fd,
    })
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
    fn modes_use_logind_names() {
        assert_eq!(InhibitMode::Block.as_str(), "block");
        assert_eq!(InhibitMode::Delay.as_str(), "delay");
    }
}
