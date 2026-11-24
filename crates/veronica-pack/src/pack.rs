use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use veronica_protocol::canonical;
use veronica_protocol::catalog::{
    package_asset_name, FileEntry, FileIndex, PackageMetadata, FILE_MODE_EXECUTABLE,
    FILE_MODE_REGULAR,
};
use veronica_protocol::manifest::Target;

use crate::fingerprint::Stamp;
use crate::repo::{extension_dir, read_manifest, MANIFEST_FILE};
use crate::sha256_hex;

pub const FILES_INDEX: &str = "files.json";
const ZSTD_LEVEL: i32 = 19;
const DEFAULT_GUIDE: &str = "guide.md";
const LEGAL_FILES: [&str; 2] = ["LICENSE", "NOTICE"];

pub struct PackInputs<'a> {
    pub root: &'a Path,
    pub backend: Option<&'a Path>,
    pub ui: Option<&'a Path>,
    pub out: &'a Path,
    pub target: Target,
}

#[derive(Debug)]
pub struct Packed {
    pub archive: PathBuf,
    pub metadata: PackageMetadata,
}

struct Entry {
    path: String,
    mode: u32,
    bytes: Vec<u8>,
}

fn file_entry(path: String, source: &Path, keep_executable: bool) -> Result<Entry> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.is_file() {
        bail!("{} is not a regular file", source.display());
    }
    let executable = keep_executable && metadata.permissions().mode() & 0o111 != 0;
    Ok(Entry {
        path,
        mode: if executable {
            FILE_MODE_EXECUTABLE
        } else {
            FILE_MODE_REGULAR
        },
        bytes: fs::read(source).with_context(|| format!("reading {}", source.display()))?,
    })
}

fn collect_tree(
    dir: &Path,
    prefix: &str,
    keep_executable: bool,
    entries: &mut Vec<Entry>,
) -> Result<()> {
    for item in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let item = item?;
        let name = item.file_name();
        let Some(name) = name.to_str() else {
            bail!("{} has a name that is not UTF-8", item.path().display());
        };
        let path = format!("{prefix}/{name}");
        if item.file_type()?.is_dir() {
            collect_tree(&item.path(), &path, keep_executable, entries)?;
        } else {
            entries.push(file_entry(path, &item.path(), keep_executable)?);
        }
    }
    Ok(())
}

fn tar_zst(entries: &[Entry]) -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(Vec::new());
    builder.mode(tar::HeaderMode::Deterministic);
    for entry in entries {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(entry.bytes.len() as u64);
        header.set_mode(entry.mode);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        builder.append_data(&mut header, &entry.path, entry.bytes.as_slice())?;
    }
    let tar = builder.into_inner()?;
    Ok(zstd::stream::encode_all(tar.as_slice(), ZSTD_LEVEL)?)
}

pub fn pack(inputs: &PackInputs<'_>, stamp: &Stamp) -> Result<Packed> {
    let source = extension_dir(inputs.root, &stamp.id);
    let manifest = read_manifest(&source)?;
    if !manifest.targets.contains(&inputs.target) {
        bail!(
            "{} does not list target {}",
            manifest.id,
            inputs.target.as_str()
        );
    }
    let mut entries = Vec::new();

    let mut stamped = serde_json::to_value(&manifest)?;
    stamped["version"] = stamp.version.to_string().into();
    entries.push(Entry {
        path: MANIFEST_FILE.to_string(),
        mode: FILE_MODE_REGULAR,
        bytes: canonical::value_to_string_pretty(&stamped).into_bytes(),
    });

    match (&manifest.backend, inputs.backend) {
        (Some(backend), Some(binary)) => {
            let mut entry = file_entry(backend.entry.clone(), binary, false)?;
            entry.mode = FILE_MODE_EXECUTABLE;
            entries.push(entry);
        }
        (Some(_), None) => bail!("{} declares a backend, pass --backend", manifest.id),
        (None, Some(_)) => bail!("{} declares no backend", manifest.id),
        (None, None) => {}
    }

    match (&manifest.ui, inputs.ui) {
        (Some(ui), Some(dir)) => {
            let start = entries.len();
            collect_tree(dir, &ui.root, false, &mut entries)?;
            for page in &ui.pages {
                let entry = format!("{}/{}", ui.root, page.entry);
                if !entries[start..].iter().any(|item| item.path == entry) {
                    bail!("page {} needs {entry}, which the UI build lacks", page.id);
                }
            }
        }
        (Some(_), None) => bail!("{} declares a UI, pass --ui", manifest.id),
        (None, Some(_)) => bail!("{} declares no UI", manifest.id),
        (None, None) => {}
    }

    for (dir, keep_executable) in [("assets", false), ("resources", true)] {
        if source.join(dir).is_dir() {
            collect_tree(&source.join(dir), dir, keep_executable, &mut entries)?;
        }
    }

    let guide = manifest.guide.as_deref().unwrap_or(DEFAULT_GUIDE);
    if source.join(guide).is_file() {
        entries.push(file_entry(guide.to_string(), &source.join(guide), false)?);
    } else if manifest.guide.is_some() {
        bail!("{} names guide {guide}, which is missing", manifest.id);
    }

    for name in LEGAL_FILES {
        let path = [source.join(name), inputs.root.join(name)]
            .into_iter()
            .find(|candidate| candidate.is_file())
            .with_context(|| format!("no {name} in the extension or the repository root"))?;
        entries.push(file_entry(name.to_string(), &path, false)?);
    }

    if let Some(icon) = manifest.icon.as_deref().filter(|icon| icon.contains('/')) {
        if !entries.iter().any(|entry| entry.path == icon) {
            bail!("icon {icon} is not part of the package");
        }
    }

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    if let Some(pair) = entries.windows(2).find(|pair| pair[0].path == pair[1].path) {
        bail!("{} would be packed twice", pair[0].path);
    }

    let index: FileIndex = entries
        .iter()
        .map(|entry| {
            (
                entry.path.clone(),
                FileEntry {
                    sha256: sha256_hex(&entry.bytes),
                    size: entry.bytes.len() as u64,
                    mode: entry.mode,
                },
            )
        })
        .collect();
    entries.push(Entry {
        path: FILES_INDEX.to_string(),
        mode: FILE_MODE_REGULAR,
        bytes: canonical::to_string_pretty(&index)?.into_bytes(),
    });
    entries.sort_by(|left, right| left.path.cmp(&right.path));

    let archive = tar_zst(&entries)?;
    let name = package_asset_name(&manifest.id, stamp.version, inputs.target);
    let sha256 = sha256_hex(&archive);
    let metadata = PackageMetadata {
        id: manifest.id.clone(),
        version: stamp.version,
        target: inputs.target,
        size: archive.len() as u64,
        sha256: sha256.clone(),
        unpacked_size: entries.iter().map(|entry| entry.bytes.len() as u64).sum(),
        fingerprint: stamp.fingerprint.clone(),
        commit: stamp.commit.clone(),
        engines: manifest.engines.clone(),
        manifest,
    };

    fs::create_dir_all(inputs.out)?;
    let path = inputs.out.join(&name);
    fs::write(&path, &archive)?;
    fs::write(
        inputs.out.join(format!("{name}.sha256")),
        format!("{sha256}  {name}\n"),
    )?;
    fs::write(
        inputs.out.join(format!("{name}.json")),
        canonical::to_string_pretty(&metadata)?,
    )?;
    Ok(Packed {
        archive: path,
        metadata,
    })
}
