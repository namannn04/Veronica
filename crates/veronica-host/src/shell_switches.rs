use anyhow::Result;
use serde_json::Value;
use veronica_core::settings::Settings;
use veronica_protocol::manifest::ShellService;
use veronica_protocol::Manifest;

use crate::layout::Layout;
use crate::registry::{Origin, Registry};

pub const SWITCHES: [(ShellService, &str); 2] = [
    (ShellService::ClipboardWatch, "clipboardEnabled"),
    (ShellService::FocusDim, "focusDimEnabled"),
];

fn manifest(layout: &Layout, id: &str) -> Option<Manifest> {
    let raw = std::fs::read_to_string(layout.current_link(id).join("manifest.json")).ok()?;
    Manifest::parse(&raw).ok()
}

pub fn declared(layout: &Layout, id: &str) -> Vec<ShellService> {
    manifest(layout, id)
        .map(|manifest| manifest.permissions.shell)
        .unwrap_or_default()
}

fn write(layout: &Layout, changes: &[(&str, bool)]) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    let path = layout.settings_file();
    let mut settings = Settings::load(&path)?;
    for (key, on) in changes {
        settings.set(key, Value::Bool(*on));
    }
    settings.save(&path)
}

pub fn switch_on(layout: &Layout, services: &[ShellService]) -> Result<()> {
    let changes: Vec<(&str, bool)> = SWITCHES
        .iter()
        .filter(|(service, _)| services.contains(service))
        .map(|(_, key)| (*key, true))
        .collect();
    write(layout, &changes)
}

pub fn release(
    layout: &Layout,
    registry: &Registry,
    id: &str,
    services: &[ShellService],
) -> Result<()> {
    let others: Vec<ShellService> = registry
        .extensions
        .iter()
        .filter(|(other, entry)| {
            other.as_str() != id && entry.enabled && entry.origin != Origin::Builtin
        })
        .flat_map(|(other, _)| declared(layout, other))
        .collect();
    let changes: Vec<(&str, bool)> = SWITCHES
        .iter()
        .filter(|(service, _)| services.contains(service) && !others.contains(service))
        .map(|(_, key)| (*key, false))
        .collect();
    write(layout, &changes)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;
    use veronica_core::paths::{AppDirectories, EnvOverrides};
    use veronica_core::settings::Settings;
    use veronica_protocol::manifest::ShellService;

    use super::{declared, release, switch_on};
    use crate::layout::Layout;
    use crate::registry::{InstalledExtension, Origin, Registry};

    fn install(layout: &Layout, id: &str, shell: &[&str]) {
        let root = layout.current_link(id);
        std::fs::create_dir_all(&root).unwrap();
        let manifest = json!({
            "schema": 1, "id": id, "version": "1.0", "title": id, "summary": "test",
            "category": "utilities",
            "engines": { "host": ">=0.1.0", "hostApi": 1 },
            "targets": ["linux-x86_64"],
            "permissions": { "shell": shell },
        });
        std::fs::write(root.join("manifest.json"), manifest.to_string()).unwrap();
    }

    fn entry(enabled: bool) -> InstalledExtension {
        InstalledExtension {
            version: "1.0.0".parse().unwrap(),
            origin: Origin::Bundled,
            enabled,
            installed_at: String::new(),
            previous: None,
            quarantined: BTreeSet::new(),
        }
    }

    fn clipboard_on(layout: &Layout) -> Option<bool> {
        Settings::load(&layout.settings_file())
            .unwrap()
            .get("clipboardEnabled")
            .and_then(serde_json::Value::as_bool)
    }

    #[test]
    fn a_shell_service_stays_on_while_any_enabled_extension_declares_it() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(&AppDirectories::with_env(
            root.path(),
            EnvOverrides::default(),
        ));
        install(&layout, "clipboard", &["clipboard.watch"]);
        install(&layout, "snippets", &["clipboard.watch"]);
        install(&layout, "plain", &[]);
        assert_eq!(
            declared(&layout, "clipboard"),
            [ShellService::ClipboardWatch]
        );

        switch_on(&layout, &declared(&layout, "clipboard")).unwrap();
        assert_eq!(clipboard_on(&layout), Some(true));

        let mut registry = Registry::default();
        registry.extensions.insert("snippets".into(), entry(true));
        registry.extensions.insert("plain".into(), entry(true));
        let services = declared(&layout, "clipboard");
        release(&layout, &registry, "clipboard", &services).unwrap();
        assert_eq!(clipboard_on(&layout), Some(true));

        registry.extensions.insert("snippets".into(), entry(false));
        release(&layout, &registry, "clipboard", &services).unwrap();
        assert_eq!(clipboard_on(&layout), Some(false));
    }
}
