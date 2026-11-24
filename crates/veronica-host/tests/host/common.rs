use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::json;
use tar::{EntryType, Header};
use veronica_host::archive::{pack, sha256_hex};
use veronica_protocol::catalog::{FileEntry, FileIndex};

pub(crate) struct Raw<'a> {
    pub(crate) path: &'a [u8],
    pub(crate) kind: EntryType,
    pub(crate) mode: u32,
    pub(crate) data: &'a [u8],
}

impl<'a> Raw<'a> {
    pub(crate) fn file(path: &'a str, data: &'a [u8]) -> Self {
        Self {
            path: path.as_bytes(),
            kind: EntryType::Regular,
            mode: 0o644,
            data,
        }
    }
}

pub(crate) fn raw_archive(entries: &[Raw<'_>]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for entry in entries {
        let mut header = Header::new_gnu();
        header.as_old_mut().name[..entry.path.len()].copy_from_slice(entry.path);
        header.set_entry_type(entry.kind);
        header.set_mode(entry.mode);
        header.set_size(entry.data.len() as u64);
        header.set_cksum();
        builder.append(&header, entry.data).unwrap();
    }
    zstd::encode_all(builder.into_inner().unwrap().as_slice(), 3).unwrap()
}

pub(crate) fn index_of(files: &[(&str, &[u8], u32)]) -> FileIndex {
    files
        .iter()
        .map(|(path, data, mode)| {
            let entry = FileEntry {
                sha256: sha256_hex(*data).unwrap(),
                size: data.len() as u64,
                mode: *mode,
            };
            ((*path).to_string(), entry)
        })
        .collect()
}

pub(crate) fn index_json(index: &FileIndex) -> Vec<u8> {
    serde_json::to_vec(index).unwrap()
}

pub(crate) fn write_tree(root: &Path, files: &[(&str, &[u8], u32)]) {
    for (path, data, mode) in files {
        let target = root.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, data).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(*mode)).unwrap();
    }
}

pub(crate) fn manifest(id: &str, version: &str) -> Vec<u8> {
    manifest_with(id, version, ">=0.1.0", 1, "linux-x86_64")
}

pub(crate) fn manifest_with(
    id: &str,
    version: &str,
    host: &str,
    api: u32,
    target: &str,
) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema": 1,
        "id": id,
        "version": version,
        "title": "Probe",
        "summary": "A test extension.",
        "category": "labs",
        "engines": { "host": host, "hostApi": api },
        "targets": [target]
    }))
    .unwrap()
}

pub(crate) fn package_tree(root: &Path, manifest: &[u8], backend: &[u8]) {
    write_tree(
        root,
        &[
            ("manifest.json", manifest, 0o644),
            ("bin/backend", backend, 0o755),
            ("ui/index.html", b"<!doctype html>", 0o644),
        ],
    );
}

pub(crate) fn package(manifest: &[u8], backend: &[u8]) -> Vec<u8> {
    let source = tempfile::tempdir().unwrap();
    package_tree(source.path(), manifest, backend);
    let mut bytes = Vec::new();
    pack(source.path(), &mut bytes).unwrap();
    bytes
}
