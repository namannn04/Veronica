use anyhow::{Context, Result};
use serde::Serialize;
use zbus::Connection;

pub const SCREEN_CAST_BUS: &str = "org.gnome.Mutter.ScreenCast";
pub const SCREEN_CAST_SESSIONS_PATH: &str = "/org/gnome/Mutter/ScreenCast/Session";
pub const REMOTE_DESKTOP_BUS: &str = "org.gnome.Mutter.RemoteDesktop";
pub const REMOTE_DESKTOP_SESSIONS_PATH: &str = "/org/gnome/Mutter/RemoteDesktop/Session";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenShareState {
    pub sharing: bool,
    pub screencast_sessions: usize,
    pub remote_sessions: usize,
    pub reason: Option<String>,
    pub unavailable: Option<String>,
}

impl ScreenShareState {
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

pub async fn detect() -> ScreenShareState {
    let connection = match Connection::session().await {
        Ok(connection) => connection,
        Err(error) => return ScreenShareState::cannot_tell(format!("no session bus: {error}")),
    };
    detect_on(&connection).await
}

pub async fn detect_on(connection: &Connection) -> ScreenShareState {
    let screencast = count_sessions(connection, SCREEN_CAST_BUS, SCREEN_CAST_SESSIONS_PATH).await;
    let remote = count_sessions(connection, REMOTE_DESKTOP_BUS, REMOTE_DESKTOP_SESSIONS_PATH).await;

    match (screencast, remote) {
        (Err(cast_error), Err(_)) => ScreenShareState::cannot_tell(format!(
            "the compositor does not expose screencast sessions: {cast_error:#}"
        )),
        (casts, remotes) => ScreenShareState::from_counts(casts.unwrap_or(0), remotes.unwrap_or(0)),
    }
}

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

    const IDLE: &str = r#"
        <node>
          <interface name="org.freedesktop.DBus.Introspectable">
            <method name="Introspect"><arg type="s" direction="out"/></method>
          </interface>
        </node>"#;

    const SHARING: &str = r#"
        <node>
          <interface name="org.freedesktop.DBus.Introspectable"/>
          <node name="u8"/>
          <node name="u9"/>
        </node>"#;

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
        assert_eq!(
            count_child_nodes("<node name=\"\"/>"),
            0,
            "an empty name is not a node"
        );
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
        let state = ScreenShareState::from_counts(0, 1);
        assert!(state.sharing);
        assert_eq!(state.remote_sessions, 1);
    }

    #[test]
    fn cannot_tell_is_distinct_from_nothing_is_sharing() {
        let state = ScreenShareState::cannot_tell("not GNOME".into());
        assert!(!state.sharing);
        assert_eq!(state.reason, None);
        assert!(
            state.unavailable.is_some(),
            "the interface must be able to say so"
        );
    }

    #[tokio::test]
    async fn detection_answers_on_this_machine_without_erroring() {
        let state = detect().await;
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
