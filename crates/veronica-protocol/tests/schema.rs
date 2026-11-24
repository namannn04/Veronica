use std::path::PathBuf;

use serde_json::Value;
use veronica_protocol::{canonical, manifest};

const UPDATE_VARIABLE: &str = "VERONICA_UPDATE_SCHEMA";

fn committed_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/schemas/extension-manifest.schema.json")
}

#[test]
fn committed_manifest_schema_matches_the_types() {
    let generated = manifest::json_schema();
    let path = committed_path();
    if std::env::var_os(UPDATE_VARIABLE).is_some() {
        std::fs::write(&path, canonical::value_to_string_pretty(&generated)).unwrap();
    }
    let committed: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        committed == generated,
        "{} is out of date; run `{UPDATE_VARIABLE}=1 cargo test -p veronica-protocol --test schema` and format it with Biome",
        path.display()
    );
}

#[test]
fn schema_carries_the_closed_sets_and_patterns() {
    let schema = manifest::json_schema();
    let text = schema.to_string();
    assert_eq!(schema["properties"]["id"]["pattern"], manifest::ID_PATTERN);
    for needle in [
        "nowPlaying",
        "destructive",
        "settings:read",
        "clipboard.watch",
        "linux-x86_64",
        "desktopEntry",
        veronica_protocol::contract_id::CONTRACT_ID_PATTERN,
    ] {
        assert!(
            text.contains(&serde_json::to_string(needle).unwrap()),
            "{needle}"
        );
    }
}
