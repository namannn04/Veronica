use std::fs;

use serde_json::{json, Value};
use veronica_pack::repo::extension_dirs;
use veronica_pack::validate::{cross_check, validate_dirs};
use veronica_protocol::Manifest;

use crate::common::{fixtures, write};

fn fixture_manifest(id: &str) -> Value {
    let path = fixtures().join(format!("repo/extensions/{id}/manifest.json"));
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn gamma(commands: Value, contracts: Value) -> Manifest {
    let mut value = fixture_manifest("beta");
    value["id"] = json!("gamma");
    value["commands"] = commands;
    value["contracts"] = contracts;
    serde_json::from_value(value).unwrap()
}

fn manifest(id: &str) -> Manifest {
    serde_json::from_value(fixture_manifest(id)).unwrap()
}

fn messages(manifests: &[Manifest]) -> Vec<String> {
    cross_check(manifests)
        .into_iter()
        .map(|problem| format!("{}: {}", problem.extension, problem.message))
        .collect()
}

fn leaf(name: &str) -> Value {
    json!({ "name": name, "effect": "read", "json": true })
}

#[test]
fn the_fixture_extensions_are_consistent() {
    let dirs = extension_dirs(&fixtures().join("repo")).unwrap();
    let report = validate_dirs(&dirs);
    assert!(report.is_ok(), "{:?}", report.problems);
    assert_eq!(report.extensions, ["alpha", "beta"]);
}

#[test]
fn roots_and_aliases_are_unique_across_extensions() {
    let clash = gamma(
        json!([{ "name": "gamma", "aliases": ["al"], "subcommands": [leaf("show")] }]),
        json!({}),
    );
    assert_eq!(
        messages(&[manifest("alpha"), clash]),
        [r#"gamma: command root "al" is already claimed by "alpha""#]
    );
}

#[test]
fn mcp_tool_names_are_unique_across_extensions() {
    let mut renamed = leaf("show");
    renamed["mcp"] = json!({ "name": "alpha_status" });
    let clash = gamma(
        json!([{ "name": "gamma", "subcommands": [renamed] }]),
        json!({}),
    );
    assert_eq!(
        messages(&[manifest("alpha"), clash]),
        [r#"gamma: MCP tool "veronica_alpha_status" is already claimed by "alpha""#]
    );
}

#[test]
fn every_used_contract_needs_a_provider() {
    let lonely = gamma(
        json!([]),
        json!({ "uses": ["alpha.status@1", "host.privacy@1"] }),
    );
    assert!(messages(&[manifest("alpha"), lonely.clone()]).is_empty());
    assert_eq!(
        messages(&[manifest("beta"), lonely]),
        [
            "beta: uses contract alpha.status@1, which nothing provides",
            "gamma: uses contract alpha.status@1, which nothing provides",
        ]
    );
}

#[test]
fn duplicate_extensions_conflict_on_every_claim() {
    let found = messages(&[manifest("alpha"), manifest("alpha")]);
    let claimed =
        |what: &str, name: &str| format!("alpha: {what} {name:?} is already claimed by \"alpha\"");
    assert_eq!(
        found,
        [
            claimed("extension id", "alpha"),
            claimed("command root", "alpha"),
            claimed("command root", "al"),
            claimed("MCP tool", "veronica_alpha_status"),
            claimed("MCP tool", "veronica_alpha_run"),
            claimed("contract", "alpha.status@1"),
        ]
    );
}

#[test]
fn reserved_roots_are_rejected_across_extensions() {
    let reserved = gamma(
        json!([{ "name": "gamma", "aliases": ["mcp"], "subcommands": [leaf("show")] }]),
        json!({}),
    );
    assert_eq!(
        messages(&[reserved]),
        [r#"gamma: command root "mcp" is reserved by the host"#]
    );
}

#[test]
fn directories_must_match_ids_and_bad_manifests_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut renamed = fixture_manifest("beta");
    renamed["id"] = json!("delta");
    write(root, "extensions/beta/manifest.json", &renamed.to_string());
    write(root, "extensions/broken/manifest.json", "{\"schema\": 1}");
    let report = validate_dirs(&extension_dirs(root).unwrap());
    let found: Vec<_> = report
        .problems
        .iter()
        .map(|problem| problem.extension.as_str())
        .collect();
    assert_eq!(found, ["beta", "broken", "delta"]);
    assert!(report.problems[0]
        .message
        .contains("holds manifest id \"delta\""));
}
