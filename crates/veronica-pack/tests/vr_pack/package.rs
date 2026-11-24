use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use veronica_pack::audit::audit;
use veronica_pack::fingerprint::{stamp, Stamp};
use veronica_pack::pack::{pack, PackInputs, Packed};
use veronica_pack::sha256_hex;
use veronica_protocol::catalog::FileIndex;
use veronica_protocol::manifest::Target;

use crate::common::{commit_all, fixtures, repo, write};

pub(crate) fn pack_into(root: &Path, stamp: &Stamp, backend: &Path, out: &Path) -> Packed {
    pack(
        &PackInputs {
            root,
            backend: Some(backend),
            ui: Some(&fixtures().join("ui")),
            out,
            target: Target::LinuxX86_64,
        },
        stamp,
    )
    .unwrap()
}

fn backend(root: &Path, contents: &str) -> PathBuf {
    let path = root.join("build/backend");
    write(root, "build/backend", contents);
    path
}

struct Member {
    mode: u32,
    mtime: u64,
    uid: u64,
    bytes: Vec<u8>,
}

fn members(archive: &Path) -> Vec<(String, Member)> {
    let decoder = zstd::stream::Decoder::new(File::open(archive).unwrap()).unwrap();
    let mut tar = tar::Archive::new(decoder);
    tar.entries()
        .unwrap()
        .map(|entry| {
            let mut entry = entry.unwrap();
            let header = entry.header().clone();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (
                entry.path().unwrap().to_string_lossy().into_owned(),
                Member {
                    mode: header.mode().unwrap(),
                    mtime: header.mtime().unwrap(),
                    uid: header.uid().unwrap() + header.gid().unwrap(),
                    bytes,
                },
            )
        })
        .collect()
}

#[test]
fn packing_is_byte_for_byte_deterministic() {
    let dir = repo();
    let root = dir.path();
    let binary = backend(root, "binary using org.freedesktop.DBus");
    let stamped = stamp(root, "alpha").unwrap();
    let first = pack_into(root, &stamped, &binary, &root.join("out-a"));
    let second = pack_into(root, &stamped, &binary, &root.join("out-b"));
    let name = "veronica-ext-alpha-1.2.1-linux-x86_64.tar.zst";
    assert!(first.archive.ends_with(name));
    for suffix in ["", ".sha256", ".json"] {
        let read = |out: &str| fs::read(root.join(out).join(format!("{name}{suffix}"))).unwrap();
        assert_eq!(read("out-a"), read("out-b"), "{suffix}");
    }
    assert_eq!(first.metadata, second.metadata);
    let archive = fs::read(&first.archive).unwrap();
    assert_eq!(first.metadata.sha256, sha256_hex(&archive));
    assert_eq!(first.metadata.size, archive.len() as u64);
    assert_eq!(
        fs::read_to_string(root.join("out-a").join(format!("{name}.sha256"))).unwrap(),
        format!("{}  {name}\n", first.metadata.sha256)
    );
}

#[test]
fn the_package_tree_is_sorted_stamped_and_indexed() {
    let dir = repo();
    let root = dir.path();
    let binary = backend(root, "binary");
    let packed = pack_into(
        root,
        &stamp(root, "alpha").unwrap(),
        &binary,
        &root.join("out"),
    );
    let members = members(&packed.archive);
    let names: Vec<_> = members.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "LICENSE",
            "NOTICE",
            "assets/icon.svg",
            "bin/backend",
            "files.json",
            "guide.md",
            "manifest.json",
            "resources/refresh",
            "ui/app.js",
            "ui/assets/app.css",
            "ui/index.html",
        ]
    );
    let by_name: BTreeMap<_, _> = members.into_iter().collect();
    for (name, member) in &by_name {
        assert_eq!((member.mtime, member.uid), (0, 0), "{name}");
        let executable = matches!(name.as_str(), "bin/backend" | "resources/refresh");
        assert_eq!(
            member.mode,
            if executable { 0o755 } else { 0o644 },
            "{name}"
        );
    }
    let index: FileIndex = serde_json::from_slice(&by_name["files.json"].bytes).unwrap();
    assert_eq!(index.len(), by_name.len() - 1);
    for (name, entry) in &index {
        assert_eq!(entry.sha256, sha256_hex(&by_name[name].bytes), "{name}");
        assert_eq!(entry.mode, by_name[name].mode);
    }
    let manifest: Value = serde_json::from_slice(&by_name["manifest.json"].bytes).unwrap();
    assert_eq!(manifest["version"], json!("1.2.1"));
    let unpacked: u64 = by_name
        .values()
        .map(|member| member.bytes.len() as u64)
        .sum();
    assert_eq!(packed.metadata.unpacked_size, unpacked);
    assert_eq!(packed.metadata.manifest.version.to_string(), "1.2");
}

#[test]
fn packing_requires_the_declared_parts() {
    let dir = repo();
    let root = dir.path();
    let stamped = stamp(root, "alpha").unwrap();
    let missing = pack(
        &PackInputs {
            root,
            backend: None,
            ui: Some(&fixtures().join("ui")),
            out: &root.join("out"),
            target: Target::LinuxX86_64,
        },
        &stamped,
    );
    assert!(format!("{:#}", missing.unwrap_err()).contains("pass --backend"));
    let wrong = pack(
        &PackInputs {
            root,
            backend: Some(&backend(root, "binary")),
            ui: None,
            out: &root.join("out"),
            target: Target::LinuxX86_64,
        },
        &stamp(root, "beta").unwrap(),
    );
    assert!(format!("{:#}", wrong.unwrap_err()).contains("declares no backend"));
}

#[test]
fn audit_flags_undeclared_network_use() {
    let dir = repo();
    let root = dir.path();
    let stamped = stamp(root, "alpha").unwrap();
    let quiet = backend(root, "uses zbus to reach org.freedesktop.DBus");
    let report = audit(&pack_into(root, &stamped, &quiet, &root.join("quiet")).archive).unwrap();
    assert!(report.is_ok(), "{:?}", report.problems);
    assert_eq!(report.dbus.detected, ["org.freedesktop.DBus", "zbus"]);

    let chatty = backend(root, "links reqwest and rustls");
    let report = audit(&pack_into(root, &stamped, &chatty, &root.join("chatty")).archive).unwrap();
    assert_eq!(report.network.detected, ["reqwest", "rustls"]);
    assert_eq!(
        report.problems,
        ["backend shows network use (reqwest, rustls) that the manifest does not declare"]
    );

    let manifest = root.join("extensions/alpha/manifest.json");
    let mut value: Value = serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    value["permissions"]["network"] = json!(["api.example.com"]);
    fs::write(&manifest, value.to_string()).unwrap();
    commit_all(root, "declare network");
    let declared = stamp(root, "alpha").unwrap();
    let report =
        audit(&pack_into(root, &declared, &chatty, &root.join("declared")).archive).unwrap();
    assert!(report.is_ok());
}
