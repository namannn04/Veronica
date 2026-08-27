//! Detecting that the screen is being shared.
//!
//! Edith watches macOS for a display being captured or mirrored. The Ubuntu
//! equivalent is an active compositor screencast session: every well-behaved
//! screen-sharing path on Wayland — a browser tab, Zoom, OBS, GNOME's own
//! recorder — goes through `xdg-desktop-portal`, which asks Mutter for a
//! session. Mutter exports one object per live session under a `Session`
//! collection — `/org/gnome/Mutter/ScreenCast/Session/u8` — and removes it when
//! the session ends, so counting the children of that collection answers "is the
//! screen being shared" without any privileged access and without watching what
//! is on screen. (Introspecting the *root* path is not enough: it only gains a
//! single `Session` child, whatever the number of sessions beneath it.)
//!
//! RemoteDesktop is included because a remote-control session is also someone
//! else looking at the screen, which is the thing presenter mode is for.
//!
//! Detection is read-only: it can tell that a session exists and cannot see the
//! frames, the peer, or what is being shared.

use anyhow::{Context, Result};
use serde::Serialize;
use zbus::Connection;

pub const SCREEN_CAST_BUS: &str = "org.gnome.Mutter.ScreenCast";
/// The collection Mutter exports live screencast sessions beneath.
pub const SCREEN_CAST_SESSIONS_PATH: &str = "/org/gnome/Mutter/ScreenCast/Session";
pub const REMOTE_DESKTOP_BUS: &str = "org.gnome.Mutter.RemoteDesktop";
pub const REMOTE_DESKTOP_SESSIONS_PATH: &str = "/org/gnome/Mutter/RemoteDesktop/Session";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenShareState {
    /// Whether anything is capturing the screen right now.
    pub sharing: bool,
    /// Active screencast sessions: a browser tab sharing, a recorder, and so on.
    pub screencast_sessions: usize,
    /// Active remote-control sessions.
    pub remote_sessions: usize,
    /// One line naming what was found, for the interface to show. `None` when
    /// nothing is sharing.
    pub reason: Option<String>,
    /// Set when detection itself could not run, e.g. on a desktop that is not
    /// GNOME. Distinguishes "nothing is sharing" from "cannot tell".
    pub unavailable: Option<String>,
}

impl ScreenShareState {
    /// The wording Edith's `presenterAutoReason` carries, adapted to what Linux
    /// can actually distinguish.
    fn describe(screencast: usize, remote: usize) -> Option<String> {
        match (screencast, remote) {
            (0, 0) => None,
            (casts, 0) => Some(if casts == 1 {
                "The screen is being shared".to_string()
            } else {
                format!("The screen is being shared by {casts} apps")
            }),
            (0, _) => Some("Someone is controlling this computer remotely".to_string()),
            (casts, _) => Some(format!(
                "The screen is being shared ({casts}) and controlled remotely"
            )),
        }
    }

    fn from_counts(screencast: usize, remote: usize) -> Self {
        Self {
            sharing: screencast + remote > 0,
            screencast_sessions: screencast,
            remote_sessions: remote,
            reason: Self::describe(screencast, remote),
            unavailable: None,
        }
    }

    fn cannot_tell(reason: String) -> Self {
        Self {
            unavailable: Some(reason),
            ..Self::default()
        }
    }
}

/// Read the current state. Never fails: a desktop with no Mutter reports
/// `unavailable` rather than erroring, because presenter mode must keep working
/// manually wherever detection does not.
pub async fn detect() -> ScreenShareState {
    let connection = match Connection::session().await {
        Ok(connection) => connection,
        Err(error) => return ScreenShareState::cannot_tell(format!("no session bus: {error}")),
    };
    detect_on(&connection).await
}

pub async fn detect_on(connection: &Connection) -> ScreenShareState {
    let screencast =
        count_sessions(connection, SCREEN_CAST_BUS, SCREEN_CAST_SESSIONS_PATH).await;
    let remote =
        count_sessions(connection, REMOTE_DESKTOP_BUS, REMOTE_DESKTOP_SESSIONS_PATH).await;

    match (screencast, remote) {
        // Neither service answered: this is not a Mutter desktop, so detection
        // is unavailable rather than reporting a quiet screen.
        (Err(cast_error), Err(_)) => ScreenShareState::cannot_tell(format!(
            "the compositor does not expose screencast sessions: {cast_error:#}"
        )),
        // One answering is enough to report on; the other simply contributes
        // nothing, which is what a desktop without remote desktop looks like.
        (casts, remotes) => {
            ScreenShareState::from_counts(casts.unwrap_or(0), remotes.unwrap_or(0))
        }
    }
}

/// How many session objects exist beneath a Mutter service's session collection.
///
/// Mutter creates one child object per live session and removes it when the
/// session ends, so the child count *is* the session count. With no sessions the
/// collection path itself has no children — D-Bus answers introspection for a
/// path with no object, so that is an empty answer rather than an error. A
/// desktop with no Mutter at all fails to reach the bus name instead, which is
/// what separates "nothing is sharing" from "cannot tell".
async fn count_sessions(connection: &Connection, bus: &str, path: &str) -> Result<usize> {
    let proxy = zbus::fdo::IntrospectableProxy::builder(connection)
        .destination(bus.to_string())?
        .path(path.to_string())?
        .build()
        .await
        .with_context(|| format!("cannot reach {bus}"))?;
    let xml = proxy
        .introspect()
        .await
        .with_context(|| format!("{bus} did not answer introspection"))?;
    Ok(count_child_nodes(&xml))
}

/// Count `<node name="..."/>` children in introspection XML.
///
/// The root element is also a `<node>` but carries no `name`, so keying on the
/// attribute counts children only. A tiny scanner rather than an XML dependency,
/// matching how `portal.rs` reads the same kind of document.
pub fn count_child_nodes(xml: &str) -> usize {
    const NEEDLE: &str = "<node name=\"";
    let mut count = 0;
    let mut rest = xml;
    while let Some(start) = rest.find(NEEDLE) {
        rest = &rest[start + NEEDLE.len()..];
        match rest.find('"') {
            Some(end) => {
                if end > 0 {
                    count += 1;
                }
                rest = &rest[end..];
            }
            None => break,
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What introspecting the session collection returns with nothing sharing:
    /// the standard interfaces and no children. Verified against Mutter 50.
    const IDLE: &str = r#"
        <node>
          <interface name="org.freedesktop.DBus.Introspectable">
            <method name="Introspect"><arg type="s" direction="out"/></method>
          </interface>
        </node>"#;

    /// Two live sessions. Mutter names them `u<n>`, one child each. Verified by
    /// creating real sessions against Mutter 50.
    const SHARING: &str = r#"
        <node>
          <interface name="org.freedesktop.DBus.Introspectable"/>
          <node name="u8"/>
          <node name="u9"/>
        </node>"#;

    /// The service *root*, while one session is live. It gains exactly one
    /// `Session` child however many sessions exist beneath it, which is why the
    /// count is taken from the collection and not from here.
    const ROOT_WHILE_SHARING: &str = r#"
        <node>
          <interface name="org.gnome.Mutter.ScreenCast">
            <method name="CreateSession"><arg type="a{sv}" direction="in"/></method>
          </interface>
          <node name="Session"/>
        </node>"#;

    #[test]
    fn an_idle_compositor_has_no_session_children() {
        assert_eq!(count_child_nodes(IDLE), 0);
    }

    #[test]
    fn each_live_session_is_one_child_of_the_collection() {
        assert_eq!(count_child_nodes(SHARING), 2);
    }

    #[test]
    fn the_service_root_cannot_be_used_to_count_sessions() {
        // It reports a single `Session` child whether one session is live or
        // ten, so counting there would under-report every time.
        assert_eq!(
            count_child_nodes(ROOT_WHILE_SHARING),
            1,
            "the root is why the count is taken from the collection path"
        );
        assert!(SCREEN_CAST_SESSIONS_PATH.ends_with("/Session"));
        assert!(REMOTE_DESKTOP_SESSIONS_PATH.ends_with("/Session"));
    }

    #[test]
    fn interfaces_and_methods_are_not_mistaken_for_sessions() {
        // `<interface name=` and `<method name=` both carry a name attribute, so
        // a looser scan would count them and report a permanent share.
        assert_eq!(count_child_nodes(IDLE), 0);
        assert_eq!(
            count_child_nodes(r#"<node><interface name="a"><method name="b"/></interface></node>"#),
            0
        );
    }

    #[test]
    fn a_malformed_or_empty_document_counts_nothing() {
        assert_eq!(count_child_nodes(""), 0);
        assert_eq!(count_child_nodes("<node name=\"unterminated"), 0);
        assert_eq!(count_child_nodes("<node name=\"\"/>"), 0, "an empty name is not a node");
    }

    #[test]
    fn nothing_sharing_produces_no_reason() {
        let state = ScreenShareState::from_counts(0, 0);
        assert!(!state.sharing);
        assert_eq!(state.reason, None);
        assert_eq!(state.unavailable, None);
    }

    #[test]
    fn the_reason_names_what_was_found() {
        assert_eq!(
            ScreenShareState::from_counts(1, 0).reason.unwrap(),
            "The screen is being shared"
        );
        assert_eq!(
            ScreenShareState::from_counts(3, 0).reason.unwrap(),
            "The screen is being shared by 3 apps"
        );
        assert_eq!(
            ScreenShareState::from_counts(0, 1).reason.unwrap(),
            "Someone is controlling this computer remotely"
        );
        assert_eq!(
            ScreenShareState::from_counts(2, 1).reason.unwrap(),
            "The screen is being shared (2) and controlled remotely"
        );
    }

    #[test]
    fn a_remote_session_alone_still_counts_as_sharing() {
        // Someone driving the machine remotely is looking at the screen, which
        // is exactly what presenter mode is for.
        let state = ScreenShareState::from_counts(0, 1);
        assert!(state.sharing);
        assert_eq!(state.remote_sessions, 1);
    }

    #[test]
    fn cannot_tell_is_distinct_from_nothing_is_sharing() {
        let state = ScreenShareState::cannot_tell("not GNOME".into());
        assert!(!state.sharing);
        assert_eq!(state.reason, None);
        assert!(state.unavailable.is_some(), "the interface must be able to say so");
    }

    /// Runs against whatever compositor is actually present.
    #[tokio::test]
    async fn detection_answers_on_this_machine_without_erroring() {
        let state = detect().await;
        // Either it read the sessions, or it said why it could not. Never both,
        // and never a panic.
        if state.unavailable.is_some() {
            assert!(!state.sharing);
            assert_eq!(state.screencast_sessions, 0);
        } else {
            assert_eq!(
                state.sharing,
                state.screencast_sessions + state.remote_sessions > 0
            );
            assert_eq!(state.sharing, state.reason.is_some());
        }
    }
}
