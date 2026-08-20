//! xdg-desktop-portal.
//!
//! The portal is how a Wayland app reaches things the compositor owns. Which
//! interfaces exist varies by desktop and portal version, so Veronica probes
//! rather than assumes, and reports the result on the diagnostics page.

use anyhow::{Context, Result};
use zbus::Connection;

pub const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
pub const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

/// Which portal interfaces this session offers.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalSupport {
    pub global_shortcuts: bool,
    pub screenshot: bool,
    pub screen_cast: bool,
    pub camera: bool,
    pub inhibit: bool,
    pub notification: bool,
    pub remote_desktop: bool,
    pub clipboard: bool,
    pub settings: bool,
    /// Every interface name found, so diagnostics can show the full list.
    pub interfaces: Vec<String>,
}

impl PortalSupport {
    /// Build from a list of interface names.
    pub fn from_interfaces(interfaces: Vec<String>) -> Self {
        let has = |name: &str| {
            interfaces
                .iter()
                .any(|found| found == &format!("org.freedesktop.portal.{name}"))
        };
        Self {
            global_shortcuts: has("GlobalShortcuts"),
            screenshot: has("Screenshot"),
            screen_cast: has("ScreenCast"),
            camera: has("Camera"),
            inhibit: has("Inhibit"),
            notification: has("Notification"),
            remote_desktop: has("RemoteDesktop"),
            clipboard: has("Clipboard"),
            settings: has("Settings"),
            interfaces,
        }
    }
}

/// Ask the portal what it implements, by introspecting its object.
pub async fn probe(connection: &Connection) -> Result<PortalSupport> {
    let proxy = zbus::fdo::IntrospectableProxy::builder(connection)
        .destination(PORTAL_BUS)?
        .path(PORTAL_PATH)?
        .build()
        .await
        .context("cannot reach the desktop portal")?;

    let xml = proxy
        .introspect()
        .await
        .context("the desktop portal did not answer introspection")?;

    Ok(PortalSupport::from_interfaces(parse_interfaces(&xml)))
}

/// Pull `<interface name="...">` values out of introspection XML.
///
/// A tiny scanner rather than an XML dependency: the document is machine
/// generated and this only needs one attribute from it.
pub fn parse_interfaces(xml: &str) -> Vec<String> {
    const NEEDLE: &str = "<interface name=\"";
    let mut names = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(NEEDLE) {
        rest = &rest[start + NEEDLE.len()..];
        if let Some(end) = rest.find('"') {
            let name = &rest[..end];
            if !name.is_empty() {
                names.push(name.to_string());
            }
            rest = &rest[end..];
        } else {
            break;
        }
    }
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        <node>
          <interface name="org.freedesktop.DBus.Introspectable">
            <method name="Introspect"><arg type="s" direction="out"/></method>
          </interface>
          <interface name="org.freedesktop.portal.GlobalShortcuts">
            <property name="version" type="u" access="read"/>
          </interface>
          <interface name="org.freedesktop.portal.Screenshot"/>
          <interface name="org.freedesktop.portal.Camera"/>
        </node>"#;

    #[test]
    fn extracts_every_interface_name() {
        let names = parse_interfaces(SAMPLE);
        assert!(names.contains(&"org.freedesktop.portal.GlobalShortcuts".to_string()));
        assert!(names.contains(&"org.freedesktop.portal.Screenshot".to_string()));
        assert!(names.contains(&"org.freedesktop.DBus.Introspectable".to_string()));
        assert_eq!(names.len(), 4);
    }

    #[test]
    fn maps_interfaces_onto_the_features_that_need_them() {
        let support = PortalSupport::from_interfaces(parse_interfaces(SAMPLE));
        assert!(support.global_shortcuts);
        assert!(support.screenshot);
        assert!(support.camera);
        // Not in the sample, so it must not be claimed.
        assert!(!support.screen_cast);
        assert!(!support.remote_desktop);
    }

    #[test]
    fn an_empty_or_malformed_document_claims_nothing() {
        assert!(parse_interfaces("").is_empty());
        assert!(parse_interfaces("<node><interface name=\"unterminated").is_empty());
        let support = PortalSupport::from_interfaces(Vec::new());
        assert_eq!(support, PortalSupport::default());
    }

    #[test]
    fn a_prefix_match_does_not_count_as_the_interface() {
        // "GlobalShortcutsExtra" must not satisfy "GlobalShortcuts".
        let support = PortalSupport::from_interfaces(vec![
            "org.freedesktop.portal.GlobalShortcutsExtra".to_string(),
        ]);
        assert!(!support.global_shortcuts);
    }
}
