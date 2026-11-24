use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path};

use sha2::{Digest, Sha256};
use tar::EntryType;
use veronica_protocol::canonical;
use veronica_protocol::catalog::{FileEntry, FileIndex, FILE_MODE_EXECUTABLE, FILE_MODE_REGULAR};

pub const FILE_INDEX: &str = "files.json";
const COMPRESSION_LEVEL: i32 = 19;
const HEADER_ALLOWANCE: u64 = 4096;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_entries: usize,
    pub max_unpacked_bytes: u64,
    pub max_index_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            max_unpacked_bytes: 512 * 1024 * 1024,
            max_index_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive cannot be read: {0}")]
    Io(#[from] io::Error),
    #[error("{FILE_INDEX} must be the first entry of the archive")]
    IndexNotFirst,
    #[error("{FILE_INDEX} is invalid: {0}")]
    InvalidIndex(String),
    #[error("entry {0:?} has an unsafe path")]
    UnsafePath(String),
    #[error("entry {path:?} is a {kind}, only regular files are allowed")]
    NotRegular { path: String, kind: &'static str },
    #[error("entry {0:?} is not listed in {FILE_INDEX}")]
    Unlisted(String),
    #[error("entry {0:?} appears more than once")]
    Duplicate(String),
    #[error("entry {path:?} has mode {mode:o}, only 644 and 755 are allowed")]
    Mode { path: String, mode: u32 },
    #[error("entry {0:?} does not match {FILE_INDEX}")]
    Mismatch(String),
    #[error("{FILE_INDEX} lists {0:?} but the archive does not contain it")]
    Missing(String),
    #[error("package exceeds the limit of {0} entries")]
    TooManyEntries(usize),
    #[error("package exceeds the unpacked size limit of {0} bytes")]
    TooLarge(u64),
}

fn safe_path(raw: &str) -> Result<&Path, ArchiveError> {
    let path = Path::new(raw);
    let normal = !raw.is_empty()
        && !raw.ends_with('/')
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && path.components().count() == raw.split('/').count();
    if normal {
        Ok(path)
    } else {
        Err(ArchiveError::UnsafePath(raw.to_string()))
    }
}

fn kind_name(kind: EntryType) -> &'static str {
    match kind {
        EntryType::Symlink => "symlink",
        EntryType::Link => "hardlink",
        EntryType::Char | EntryType::Block => "device",
        EntryType::Fifo => "fifo",
        EntryType::Directory => "directory",
        _ => "special entry",
    }
}

const fn allowed_mode(mode: u32) -> bool {
    matches!(mode, FILE_MODE_REGULAR | FILE_MODE_EXECUTABLE)
}

pub(crate) fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn validate_index(index: &FileIndex, limits: &Limits) -> Result<(), ArchiveError> {
    if index.len() >= limits.max_entries {
        return Err(ArchiveError::TooManyEntries(limits.max_entries));
    }
    let mut total: u64 = 0;
    for (path, entry) in index {
        safe_path(path)?;
        if path == FILE_INDEX {
            return Err(ArchiveError::InvalidIndex(format!("it lists {FILE_INDEX}")));
        }
        if !allowed_mode(entry.mode) {
            return Err(ArchiveError::Mode {
                path: path.clone(),
                mode: entry.mode,
            });
        }
        if !is_sha256(&entry.sha256) {
            return Err(ArchiveError::InvalidIndex(format!(
                "{path:?} has a malformed sha256"
            )));
        }
        total = total.saturating_add(entry.size);
        if total > limits.max_unpacked_bytes {
            return Err(ArchiveError::TooLarge(limits.max_unpacked_bytes));
        }
    }
    Ok(())
}

pub fn sha256_hex(mut reader: impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut reader, &mut hasher)?;
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        hex.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(HEX_DIGITS[usize::from(byte & 0xf)]));
    }
    Ok(hex)
}

pub(crate) fn write_file(
    root: &Path,
    relative: &Path,
    mode: u32,
    mut content: impl Read,
) -> io::Result<()> {
    let target = root.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&target)?;
    io::copy(&mut content, &mut file)?;
    file.set_permissions(Permissions::from_mode(mode))?;
    file.sync_all()
}

fn entry_path<R: Read>(entry: &tar::Entry<'_, R>) -> Result<String, ArchiveError> {
    let bytes = entry.path_bytes();
    String::from_utf8(bytes.into_owned())
        .map_err(|error| ArchiveError::UnsafePath(String::from_utf8_lossy(error.as_bytes()).into()))
}

fn regular_entry<R: Read>(entry: &tar::Entry<'_, R>, path: &str) -> Result<u32, ArchiveError> {
    safe_path(path)?;
    let kind = entry.header().entry_type();
    if kind != EntryType::Regular {
        return Err(ArchiveError::NotRegular {
            path: path.to_string(),
            kind: kind_name(kind),
        });
    }
    let mode = entry.header().mode()? & 0o7777;
    if allowed_mode(mode) {
        Ok(mode)
    } else {
        Err(ArchiveError::Mode {
            path: path.to_string(),
            mode,
        })
    }
}

pub fn extract(reader: impl Read, root: &Path, limits: &Limits) -> Result<FileIndex, ArchiveError> {
    let budget = limits.max_unpacked_bytes
        + limits.max_index_bytes
        + (limits.max_entries as u64 + 2) * HEADER_ALLOWANCE;
    let decoder = zstd::stream::read::Decoder::new(reader)?;
    let mut archive = tar::Archive::new(decoder.take(budget));
    let mut entries = archive.entries()?;
    let mut first = entries.next().ok_or(ArchiveError::IndexNotFirst)??;
    if entry_path(&first)? != FILE_INDEX {
        return Err(ArchiveError::IndexNotFirst);
    }
    regular_entry(&first, FILE_INDEX)?;
    if first.size() > limits.max_index_bytes {
        return Err(ArchiveError::InvalidIndex("it is too large".into()));
    }
    let mut raw = Vec::new();
    first.read_to_end(&mut raw)?;
    let index: FileIndex = serde_json::from_slice(&raw)
        .map_err(|error| ArchiveError::InvalidIndex(error.to_string()))?;
    validate_index(&index, limits)?;
    write_file(
        root,
        Path::new(FILE_INDEX),
        FILE_MODE_REGULAR,
        raw.as_slice(),
    )?;
    let mut seen = BTreeSet::new();
    for entry in entries {
        let entry = entry?;
        let path = entry_path(&entry)?;
        let mode = regular_entry(&entry, &path)?;
        let expected = index
            .get(&path)
            .ok_or_else(|| ArchiveError::Unlisted(path.clone()))?;
        if !seen.insert(path.clone()) {
            return Err(ArchiveError::Duplicate(path));
        }
        if mode != expected.mode || entry.size() != expected.size {
            return Err(ArchiveError::Mismatch(path));
        }
        write_file(root, Path::new(&path), mode, entry)?;
    }
    if let Some(missing) = index.keys().find(|path| !seen.contains(*path)) {
        return Err(ArchiveError::Missing(missing.clone()));
    }
    verify_tree(root, &index)?;
    Ok(index)
}

fn relative_name(root: &Path, path: &Path) -> Result<String, ArchiveError> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| ArchiveError::UnsafePath(relative.to_string_lossy().into_owned()))
}

fn walk(root: &Path) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> {
    walkdir::WalkDir::new(root)
        .min_depth(1)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
}

fn non_regular_kind(file_type: fs::FileType) -> &'static str {
    if file_type.is_symlink() {
        "symlink"
    } else {
        "special entry"
    }
}

pub fn verify_tree(root: &Path, index: &FileIndex) -> Result<(), ArchiveError> {
    let mut verified = 0;
    for entry in walk(root) {
        let entry = entry.map_err(io::Error::from)?;
        let file_type = entry.file_type();
        if file_type.is_dir() {
            continue;
        }
        let path = relative_name(root, entry.path())?;
        if !file_type.is_file() {
            return Err(ArchiveError::NotRegular {
                path,
                kind: non_regular_kind(file_type),
            });
        }
        if path == FILE_INDEX {
            continue;
        }
        let expected = index
            .get(&path)
            .ok_or_else(|| ArchiveError::Unlisted(path.clone()))?;
        let metadata = entry.metadata().map_err(io::Error::from)?;
        let matches = metadata.permissions().mode() & 0o7777 == expected.mode
            && metadata.len() == expected.size
            && sha256_hex(File::open(entry.path())?)? == expected.sha256;
        if !matches {
            return Err(ArchiveError::Mismatch(path));
        }
        verified += 1;
    }
    if verified == index.len() {
        Ok(())
    } else {
        let missing = index
            .keys()
            .find(|path| !root.join(path).is_file())
            .cloned()
            .unwrap_or_default();
        Err(ArchiveError::Missing(missing))
    }
}

pub fn index_tree(root: &Path, limits: &Limits) -> Result<FileIndex, ArchiveError> {
    let mut index = FileIndex::new();
    for entry in walk(root) {
        let entry = entry.map_err(io::Error::from)?;
        let file_type = entry.file_type();
        if file_type.is_dir() {
            continue;
        }
        let path = relative_name(root, entry.path())?;
        if !file_type.is_file() {
            return Err(ArchiveError::NotRegular {
                path,
                kind: non_regular_kind(file_type),
            });
        }
        if path == FILE_INDEX {
            continue;
        }
        let metadata = entry.metadata().map_err(io::Error::from)?;
        let mode = if metadata.permissions().mode() & 0o111 == 0 {
            FILE_MODE_REGULAR
        } else {
            FILE_MODE_EXECUTABLE
        };
        let sha256 = sha256_hex(File::open(entry.path())?)?;
        index.insert(
            path,
            FileEntry {
                sha256,
                size: metadata.len(),
                mode,
            },
        );
        if index.len() >= limits.max_entries {
            return Err(ArchiveError::TooManyEntries(limits.max_entries));
        }
    }
    validate_index(&index, limits)?;
    Ok(index)
}

fn append<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    mode: u32,
    size: u64,
    content: impl Read,
) -> io::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_size(size);
    header.set_mode(mode);
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    builder.append_data(&mut header, path, content)
}

pub fn pack(source: &Path, writer: impl Write) -> Result<FileIndex, ArchiveError> {
    let index = index_tree(source, &Limits::default())?;
    let listing = canonical::to_string_pretty(&index)
        .map_err(|error| ArchiveError::InvalidIndex(error.to_string()))?;
    let encoder = zstd::stream::write::Encoder::new(writer, COMPRESSION_LEVEL)?;
    let mut builder = tar::Builder::new(encoder);
    append(
        &mut builder,
        FILE_INDEX,
        FILE_MODE_REGULAR,
        listing.len() as u64,
        listing.as_bytes(),
    )?;
    for (path, entry) in &index {
        append(
            &mut builder,
            path,
            entry.mode,
            entry.size,
            File::open(source.join(path))?.take(entry.size),
        )?;
    }
    builder.into_inner()?.finish()?;
    Ok(index)
}
