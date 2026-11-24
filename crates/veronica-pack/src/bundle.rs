use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use veronica_protocol::bundled::{BundledIndex, BundledPackage, BUNDLED_INDEX, NEVER_BUNDLED};
use veronica_protocol::canonical;
use veronica_protocol::catalog::package_asset_name;

use crate::catalog::read_metadata;
use crate::sha256_hex;

pub fn bundle(packages: &Path, out: &Path) -> Result<BundledIndex> {
    let metadata = if packages.exists() {
        read_metadata(packages)?
    } else {
        Vec::new()
    };
    if out.exists() {
        fs::remove_dir_all(out).with_context(|| format!("clearing {}", out.display()))?;
    }
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let mut archives = BTreeMap::new();
    if packages.exists() {
        collect_archives(packages, &mut archives)?;
    }
    let mut index = BundledIndex::default();
    for item in metadata {
        if NEVER_BUNDLED.contains(&item.id.as_str()) {
            bail!("{} is a test extension and is never bundled", item.id);
        }
        ensure!(
            index.get(&item.id).is_none(),
            "{} appears more than once in {}",
            item.id,
            packages.display()
        );
        let name = package_asset_name(&item.id, item.version, item.target);
        let archive = archives
            .get(&name)
            .with_context(|| format!("{name} is missing from {}", packages.display()))?;
        let bytes = fs::read(archive).with_context(|| format!("reading {}", archive.display()))?;
        ensure!(
            bytes.len() as u64 == item.size && sha256_hex(&bytes) == item.sha256,
            "{} does not match its metadata",
            archive.display()
        );
        let package = BundledPackage {
            id: item.id,
            version: item.version,
            sha256: item.sha256,
            size: item.size,
            profiles: item.manifest.profiles,
        };
        fs::write(out.join(package.file_name()), &bytes)?;
        index.extensions.push(package);
    }
    index
        .extensions
        .sort_by(|left, right| left.id.cmp(&right.id));
    fs::write(
        out.join(BUNDLED_INDEX),
        canonical::to_string_pretty(&index)?,
    )?;
    Ok(index)
}

fn collect_archives(dir: &Path, found: &mut BTreeMap<String, PathBuf>) -> Result<()> {
    for item in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = item?.path();
        if path.is_dir() {
            collect_archives(&path, found)?;
        } else if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            if name.ends_with(".tar.zst") {
                found.insert(name.to_string(), path);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::json;
    use veronica_protocol::bundled::{BundledIndex, BUNDLED_INDEX};
    use veronica_protocol::canonical;
    use veronica_protocol::catalog::{package_asset_name, PackageMetadata};
    use veronica_protocol::manifest::{Profile, Target};
    use veronica_protocol::Manifest;

    use super::bundle;
    use crate::sha256_hex;

    fn package(dir: &Path, id: &str, version: &str, bytes: &[u8]) {
        let manifest = Manifest::parse(
            &json!({
                "schema": 1, "id": id, "version": "1.0", "title": "Probe",
                "summary": "A test extension.", "category": "labs",
                "engines": { "host": ">=0.1.0", "hostApi": 1 },
                "targets": ["linux-x86_64"], "profiles": ["simple"]
            })
            .to_string(),
        )
        .unwrap();
        let version = version.parse().unwrap();
        let name = package_asset_name(id, version, Target::LinuxX86_64);
        let metadata = PackageMetadata {
            id: id.to_string(),
            version,
            target: Target::LinuxX86_64,
            size: bytes.len() as u64,
            sha256: sha256_hex(bytes),
            unpacked_size: 1,
            fingerprint: "sha256:00".to_string(),
            commit: "0123abcd".to_string(),
            published_at: "2026-11-25T00:00:00Z".to_string(),
            engines: manifest.engines.clone(),
            manifest,
        };
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(&name), bytes).unwrap();
        fs::write(
            dir.join(format!("{name}.json")),
            canonical::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn no_packages_still_produce_an_empty_index() {
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("bundled");
        let index = bundle(&root.path().join("missing"), &out).unwrap();
        assert_eq!(index, BundledIndex::default());
        let written: BundledIndex =
            serde_json::from_str(&fs::read_to_string(out.join(BUNDLED_INDEX)).unwrap()).unwrap();
        assert_eq!(written, index);
    }

    #[test]
    fn packages_are_copied_by_id_and_indexed_with_their_hash() {
        let root = tempfile::tempdir().unwrap();
        let packages = root.path().join("packages");
        package(&packages.join("ext-zeta"), "zeta", "1.2.3", b"zeta");
        package(&packages.join("ext-alpha"), "alpha", "2.0.7", b"alpha");
        let out = root.path().join("bundled");
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("stale.tar.zst"), b"old").unwrap();
        let index = bundle(&packages, &out).unwrap();
        let ids: Vec<_> = index.extensions.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["alpha", "zeta"]);
        assert_eq!(index.extensions[0].sha256, sha256_hex(b"alpha"));
        assert_eq!(index.extensions[0].version.to_string(), "2.0.7");
        assert_eq!(index.extensions[0].profiles, [Profile::Simple]);
        assert_eq!(fs::read(out.join("zeta.tar.zst")).unwrap(), b"zeta");
        assert!(!out.join("stale.tar.zst").exists());
    }

    #[test]
    fn the_test_fixture_and_tampered_archives_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let packages = root.path().join("packages");
        package(&packages, "fixture", "0.1.4", b"fixture");
        let error = bundle(&packages, &root.path().join("out")).unwrap_err();
        assert!(error.to_string().contains("never bundled"), "{error}");

        let other = root.path().join("other");
        package(&other, "probe", "1.0.1", b"probe");
        let name = package_asset_name("probe", "1.0.1".parse().unwrap(), Target::LinuxX86_64);
        fs::write(other.join(name), b"tampered").unwrap();
        assert!(bundle(&other, &root.path().join("out")).is_err());
    }
}
