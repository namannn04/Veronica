use serde_json::{json, Value};
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_core::settings::Settings;
use veronica_host::registry::Origin;
use veronica_host::shortcuts::{current, State, OVERRIDES_KEY};
use veronica_host::{HostIdentity, Installer, Layout};
use veronica_protocol::manifest::Target;

use crate::common::{manifest, package_tree};

fn installer(home: &std::path::Path) -> Installer {
    let layout = Layout::new(&AppDirectories::with_env(home, EnvOverrides::default()));
    Installer::new(
        layout,
        HostIdentity {
            version: semver::Version::new(0, 5, 0),
            api: 1,
            target: Target::LinuxX86_64,
        },
    )
}

fn sideload(installer: &Installer, id: &str, shortcuts: &Value) {
    let mut document: Value = serde_json::from_slice(&manifest(id, "1.0")).unwrap();
    document["shortcuts"] = shortcuts.clone();
    let source = tempfile::tempdir().unwrap();
    package_tree(
        source.path(),
        &serde_json::to_vec(&document).unwrap(),
        b"backend",
    );
    installer.install_sideload(source.path()).unwrap();
}

fn active(installer: &Installer, settings: &Settings) -> Vec<(String, Option<String>, State)> {
    current(installer.layout(), settings)
        .unwrap()
        .into_iter()
        .map(|planned| (planned.declared.key(), planned.binding, planned.state))
        .collect()
}

fn row(key: &str, binding: Option<&str>, state: State) -> (String, Option<String>, State) {
    (key.to_string(), binding.map(str::to_string), state)
}

#[test]
fn enabled_extensions_contribute_their_manifest_shortcuts() {
    let home = tempfile::tempdir().unwrap();
    let installer = installer(home.path());
    sideload(
        &installer,
        "probe",
        &json!([
            { "id": "ping", "title": "Ping", "default": "ctrl+alt+j", "run": ["probe", "ping"] },
            { "id": "steal", "title": "Steal", "default": "Ctrl+Alt+V", "run": ["probe", "steal"] },
            { "id": "quiet", "title": "Quiet", "run": ["probe", "quiet"] }
        ]),
    );
    let mut settings = Settings::default();
    settings.set("clipboardEnabled", Value::Bool(false));

    let planned = current(installer.layout(), &settings).unwrap();
    let ping = planned.iter().find(|p| p.declared.id == "ping").unwrap();
    assert_eq!(ping.declared.origin, Some(Origin::Sideload));
    assert_eq!(ping.declared.run, ["probe", "ping"]);
    assert_eq!(
        active(&installer, &settings),
        [
            row("veronica/show", Some("Ctrl+Alt+V"), State::Active),
            row("veronica/cleanKeys", Some("Ctrl+Alt+K"), State::Active),
            row("probe/ping", Some("Ctrl+Alt+J"), State::Active),
            row("probe/steal", Some("Ctrl+Alt+V"), State::Conflict),
            row("probe/quiet", None, State::Unbound),
        ]
    );

    settings.set(
        OVERRIDES_KEY,
        json!({ "probe/quiet": "Super+Q", "probe/ping": null }),
    );
    let keys = active(&installer, &settings);
    assert!(keys.contains(&row("probe/quiet", Some("Super+Q"), State::Active)));
    assert!(keys.contains(&row("probe/ping", None, State::Unbound)));

    installer.set_enabled("probe", false).unwrap();
    assert!(active(&installer, &settings)
        .iter()
        .all(|(key, _, _)| !key.starts_with("probe/")));
}

#[test]
fn built_in_features_keep_todays_default_bindings() {
    let home = tempfile::tempdir().unwrap();
    let installer = installer(home.path());
    let mut settings = Settings::default();
    let enabled = |settings: &Settings| -> Vec<String> {
        active(&installer, settings)
            .into_iter()
            .filter(|(_, _, state)| *state == State::Active)
            .filter_map(|(_, binding, _)| binding)
            .collect()
    };
    assert_eq!(
        enabled(&settings),
        ["Ctrl+Alt+V", "Ctrl+Alt+K", "Ctrl+Alt+B"]
    );

    settings.set("clipboardEnabled", Value::Bool(false));
    settings.set("micMuteEnabled", Value::Bool(true));
    settings.set("colorPickerEnabled", Value::Bool(true));
    settings.set("emojiEnabled", Value::Bool(true));
    assert_eq!(
        enabled(&settings),
        ["Ctrl+Alt+V", "Ctrl+Alt+K", "Ctrl+Alt+M"]
    );
}
