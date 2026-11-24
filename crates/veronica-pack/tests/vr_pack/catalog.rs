use std::fs;

use veronica_pack::catalog::{build, read_metadata, CatalogOptions};
use veronica_protocol::canonical;
use veronica_protocol::catalog::{Catalog, PackageMetadata};
use veronica_protocol::manifest::{Profile, Target};
use veronica_protocol::Manifest;

use crate::common::{fixtures, write};

const REVISION: u64 = 1_795_564_800;

fn metadata(id: &str, version: &str) -> PackageMetadata {
    let path = fixtures().join(format!("repo/extensions/{id}/manifest.json"));
    let manifest = Manifest::parse(&fs::read_to_string(path).unwrap()).unwrap();
    PackageMetadata {
        id: id.to_string(),
        version: version.parse().unwrap(),
        target: Target::LinuxX86_64,
        size: 100,
        sha256: format!("{id}-{version}-sha"),
        unpacked_size: 400,
        fingerprint: format!("sha256:{id}-{version}"),
        commit: "0123abcd".to_string(),
        engines: manifest.engines.clone(),
        manifest,
    }
}

fn releases() -> Vec<PackageMetadata> {
    let mut all: Vec<_> = (1..=5)
        .map(|patch| metadata("alpha", &format!("1.2.{patch}")))
        .collect();
    all.push(metadata("beta", "0.3.1"));
    all
}

fn options() -> CatalogOptions {
    CatalogOptions {
        revision: REVISION,
        expires_days: 30,
        commit: "feedbeef".to_string(),
    }
}

#[test]
fn the_catalog_matches_the_golden_payload() {
    let catalog = build(releases(), &options()).unwrap();
    let payload: serde_json::Value =
        serde_json::from_slice(&catalog.to_payload_bytes().unwrap()).unwrap();
    let rendered = canonical::value_to_string_pretty(&payload);
    let golden = fixtures().join("catalog.golden.json");
    if std::env::var_os("VR_PACK_BLESS").is_some() {
        fs::write(&golden, &rendered).unwrap();
    }
    let expected: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(golden).unwrap()).unwrap();
    assert_eq!(payload, expected);
}

#[test]
fn the_catalog_keeps_three_versions_and_derives_profiles() {
    let catalog = build(releases(), &options()).unwrap();
    assert_eq!(catalog.generated_at, "2026-11-25T00:00:00Z");
    assert_eq!(catalog.expires_at, "2026-12-25T00:00:00Z");
    let alpha = catalog.extension("alpha").unwrap();
    assert_eq!(alpha.latest.to_string(), "1.2.5");
    let kept: Vec<_> = alpha
        .versions
        .iter()
        .map(|v| v.version.to_string())
        .collect();
    assert_eq!(kept, ["1.2.5", "1.2.4", "1.2.3"]);
    assert!(
        alpha.latest_version().unwrap().targets[&Target::LinuxX86_64]
            .url
            .ends_with("/ext/alpha/v1.2.5/veronica-ext-alpha-1.2.5-linux-x86_64.tar.zst")
    );
    let profile = |id| {
        catalog
            .profiles
            .iter()
            .find(|profile| profile.id == id)
            .unwrap()
            .extensions
            .clone()
    };
    assert_eq!(profile(Profile::Simple), ["alpha"]);
    assert_eq!(profile(Profile::Developer), ["alpha", "beta"]);
    let categories: Vec<_> = catalog
        .categories
        .iter()
        .map(|category| (category.id.as_str(), category.hidden))
        .collect();
    assert_eq!(
        categories,
        [("agents", false), ("desk", false), ("labs", true)]
    );
}

#[test]
fn the_payload_does_not_depend_on_input_order() {
    let mut shuffled = releases();
    shuffled.reverse();
    shuffled.swap(0, 3);
    assert_eq!(
        build(releases(), &options())
            .unwrap()
            .to_payload_bytes()
            .unwrap(),
        build(shuffled, &options())
            .unwrap()
            .to_payload_bytes()
            .unwrap()
    );
}

#[test]
fn metadata_is_read_from_nested_release_folders() {
    let dir = tempfile::tempdir().unwrap();
    for (index, item) in releases().iter().enumerate() {
        write(
            dir.path(),
            &format!("release-{index}/{}.json", item.id),
            &canonical::to_string_pretty(item).unwrap(),
        );
    }
    write(dir.path(), "release-0/notes.txt", "skipped");
    let read = read_metadata(dir.path()).unwrap();
    assert_eq!(read.len(), 6);
    let catalog: Catalog = build(read, &options()).unwrap();
    assert_eq!(catalog, build(releases(), &options()).unwrap());
}

#[test]
fn a_version_cannot_have_two_builds_for_one_target() {
    let mut all = releases();
    all.push(metadata("beta", "0.3.1"));
    let error = build(all, &options()).unwrap_err();
    assert_eq!(error.to_string(), "beta 0.3.1 has two linux-x86_64 builds");
}
