use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use tar::{EntryType, Header};
use veronica_host::archive::{pack, sha256_hex};
use veronica_host::fetch::{Response, Transport};
use veronica_host::trust::DEVELOPMENT_KEY;
use veronica_pack::catalog::{build, CatalogOptions};
use veronica_pack::signing;
use veronica_protocol::catalog::{
    Catalog, EnvelopeSignature, FileEntry, FileIndex, PackageMetadata, SignatureAlgorithm,
    SignedEnvelope,
};
use veronica_protocol::manifest::Target;
use veronica_protocol::{Manifest, PackageVersion};

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

pub(crate) const NOW: &str = "2026-11-24T12:00:00Z";
pub(crate) const REVISION: u64 = 1_795_600_000;

pub(crate) fn now() -> DateTime<Utc> {
    NOW.parse().unwrap()
}

pub(crate) fn development_key() -> SigningKey {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/development-catalog.key");
    signing::load_private(path.to_str().unwrap()).unwrap()
}

pub(crate) fn metadata(id: &str, version: &str, sha256: &str, size: u64) -> PackageMetadata {
    let version: PackageVersion = version.parse().unwrap();
    let manifest = Manifest::parse(
        std::str::from_utf8(&manifest(id, &version.authored().to_string())).unwrap(),
    )
    .unwrap();
    PackageMetadata {
        id: id.to_string(),
        version,
        target: Target::LinuxX86_64,
        size,
        sha256: sha256.to_string(),
        unpacked_size: size,
        fingerprint: format!("sha256:{id}-{version}"),
        commit: "0123abcd".to_string(),
        published_at: "2026-11-20T08:30:00Z".to_string(),
        engines: manifest.engines.clone(),
        manifest,
    }
}

pub(crate) fn catalog_of(metadata: Vec<PackageMetadata>, revision: u64) -> Catalog {
    build(
        metadata,
        &CatalogOptions {
            revision,
            expires_days: 30,
            commit: "0123abcd".to_string(),
        },
    )
    .unwrap()
}

pub(crate) fn sign_payload(payload: &[u8], key: &SigningKey, key_id: &str) -> Vec<u8> {
    let envelope = SignedEnvelope {
        schema: 1,
        payload: STANDARD.encode(payload),
        signatures: vec![EnvelopeSignature {
            key_id: key_id.to_string(),
            algorithm: SignatureAlgorithm::Ed25519,
            signature: STANDARD.encode(key.sign(payload).to_bytes()),
        }],
    };
    envelope.to_canonical_string().unwrap().into_bytes()
}

pub(crate) fn signed(catalog: &Catalog) -> Vec<u8> {
    let payload = catalog.to_payload_bytes().unwrap();
    signing::sign(&payload, &development_key(), DEVELOPMENT_KEY.id)
        .unwrap()
        .to_canonical_string()
        .unwrap()
        .into_bytes()
}

type Served = BTreeMap<String, (Vec<u8>, Option<String>)>;

#[derive(Default)]
pub(crate) struct FakeTransport {
    served: Mutex<Served>,
    pub(crate) requests: Mutex<Vec<(String, Option<String>)>>,
}

impl FakeTransport {
    pub(crate) fn serve(&self, url: &str, body: Vec<u8>, etag: Option<&str>) {
        self.served
            .lock()
            .unwrap()
            .insert(url.to_string(), (body, etag.map(str::to_string)));
    }

    pub(crate) fn last_etag(&self) -> Option<String> {
        self.requests.lock().unwrap().last().unwrap().1.clone()
    }
}

impl Transport for FakeTransport {
    fn get(&self, url: &str, etag: Option<&str>) -> anyhow::Result<Response> {
        self.requests
            .lock()
            .unwrap()
            .push((url.to_string(), etag.map(str::to_string)));
        let served = self.served.lock().unwrap();
        let Some((body, current)) = served.get(url) else {
            anyhow::bail!("offline");
        };
        if etag.is_some() && etag == current.as_deref() {
            return Ok(Response::NotModified);
        }
        Ok(Response::Body {
            etag: current.clone(),
            body: Box::new(Cursor::new(body.clone())),
        })
    }
}
