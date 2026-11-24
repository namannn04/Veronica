use std::fs;
use std::os::unix::fs::PermissionsExt;

use crate::common::{index_json, index_of, raw_archive, write_tree, Raw};
use proptest::prelude::*;
use tar::EntryType;
use veronica_host::archive::{extract, index_tree, pack, ArchiveError, Limits, FILE_INDEX};

const BODY: &[u8] = b"payload";

fn extract_bytes(bytes: &[u8], limits: &Limits) -> Result<(), ArchiveError> {
    let root = tempfile::tempdir().unwrap();
    extract(bytes, root.path(), limits).map(drop)
}

fn listed_with(entry: Raw<'_>, listed: &[(&str, &[u8], u32)]) -> Result<(), ArchiveError> {
    let index = index_json(&index_of(listed));
    let bytes = raw_archive(&[Raw::file(FILE_INDEX, &index), entry]);
    extract_bytes(&bytes, &Limits::default())
}

fn single(entry: Raw<'_>) -> Result<(), ArchiveError> {
    let path = std::str::from_utf8(entry.path).unwrap().to_string();
    listed_with(entry, &[(&path, BODY, 0o644)])
}

#[test]
fn a_packed_tree_extracts_and_verifies() {
    let source = tempfile::tempdir().unwrap();
    write_tree(
        source.path(),
        &[
            ("manifest.json", b"{}", 0o644),
            ("bin/backend", b"\x7fELF", 0o755),
            ("ui/assets/deep/app.js", b"run()", 0o600),
        ],
    );
    let mut first = Vec::new();
    let index = pack(source.path(), &mut first).unwrap();
    let mut second = Vec::new();
    pack(source.path(), &mut second).unwrap();
    assert_eq!(first, second, "packing must be deterministic");
    assert_eq!(index["ui/assets/deep/app.js"].mode, 0o644);
    assert_eq!(index["bin/backend"].mode, 0o755);

    let target = tempfile::tempdir().unwrap();
    let extracted = extract(first.as_slice(), target.path(), &Limits::default()).unwrap();
    assert_eq!(extracted, index);
    let mode = |path: &str| {
        fs::metadata(target.path().join(path))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    };
    assert_eq!(mode("bin/backend"), 0o755);
    assert_eq!(mode("manifest.json"), 0o644);
    assert_eq!(
        fs::read(target.path().join("ui/assets/deep/app.js")).unwrap(),
        b"run()"
    );
    assert!(target.path().join(FILE_INDEX).is_file());
}

#[test]
fn packed_entries_are_sorted_with_zeroed_metadata() {
    let source = tempfile::tempdir().unwrap();
    write_tree(source.path(), &[("b", b"2", 0o644), ("a/z", b"1", 0o755)]);
    let mut bytes = Vec::new();
    pack(source.path(), &mut bytes).unwrap();
    let tar = zstd::decode_all(bytes.as_slice()).unwrap();
    let mut archive = tar::Archive::new(tar.as_slice());
    let mut names = Vec::new();
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        let header = entry.header();
        assert_eq!(header.mtime().unwrap(), 0);
        assert_eq!(header.uid().unwrap(), 0);
        assert_eq!(header.gid().unwrap(), 0);
        names.push(entry.path().unwrap().to_string_lossy().into_owned());
    }
    assert_eq!(names, [FILE_INDEX, "a/z", "b"]);
}

#[test]
fn unsafe_paths_are_rejected() {
    for path in [
        "/etc/passwd",
        "../escape",
        "ui/../../escape",
        "./manifest.json",
        "ui//app.js",
        "ui/",
    ] {
        let error = listed_with(Raw::file(path, BODY), &[("payload", BODY, 0o644)]).unwrap_err();
        assert!(
            matches!(error, ArchiveError::UnsafePath(_)),
            "{path}: {error}"
        );
    }
}

#[test]
fn non_regular_entries_are_rejected() {
    let cases = [
        (EntryType::Symlink, "symlink"),
        (EntryType::Link, "hardlink"),
        (EntryType::Char, "device"),
        (EntryType::Block, "device"),
        (EntryType::Fifo, "fifo"),
        (EntryType::Directory, "directory"),
    ];
    for (kind, expected) in cases {
        let entry = Raw {
            path: b"bin/backend",
            kind,
            mode: 0o644,
            data: b"",
        };
        let error = single(entry).unwrap_err();
        assert!(
            matches!(error, ArchiveError::NotRegular { kind, .. } if kind == expected),
            "{expected}: {error}"
        );
    }
}

#[test]
fn unlisted_and_duplicate_entries_are_rejected() {
    let error = listed_with(Raw::file("extra", BODY), &[("other", BODY, 0o644)]).unwrap_err();
    assert!(matches!(error, ArchiveError::Unlisted(ref path) if path == "extra"));

    let index = index_json(&index_of(&[("a", BODY, 0o644)]));
    let bytes = raw_archive(&[
        Raw::file(FILE_INDEX, &index),
        Raw::file("a", BODY),
        Raw::file("a", BODY),
    ]);
    let error = extract_bytes(&bytes, &Limits::default()).unwrap_err();
    assert!(matches!(error, ArchiveError::Duplicate(_)), "{error}");
}

#[test]
fn modes_other_than_644_and_755_are_rejected() {
    for mode in [0o777, 0o600, 0o4755, 0o2755, 0o666] {
        let entry = Raw {
            mode,
            ..Raw::file("bin/backend", BODY)
        };
        let error = single(entry).unwrap_err();
        assert!(
            matches!(error, ArchiveError::Mode { .. }),
            "{mode:o}: {error}"
        );
    }
    let error = listed_with(Raw::file("a", BODY), &[("a", BODY, 0o777)]).unwrap_err();
    assert!(matches!(error, ArchiveError::Mode { .. }), "{error}");
}

#[test]
fn the_index_must_come_first_and_be_well_formed() {
    let index = index_json(&index_of(&[("a", BODY, 0o644)]));
    let bytes = raw_archive(&[Raw::file("a", BODY), Raw::file(FILE_INDEX, &index)]);
    let error = extract_bytes(&bytes, &Limits::default()).unwrap_err();
    assert!(matches!(error, ArchiveError::IndexNotFirst), "{error}");

    let bytes = raw_archive(&[Raw::file(FILE_INDEX, b"[]")]);
    let error = extract_bytes(&bytes, &Limits::default()).unwrap_err();
    assert!(matches!(error, ArchiveError::InvalidIndex(_)), "{error}");

    let bad_key = br#"{"../a":{"sha256":"00","size":1,"mode":420}}"#;
    let bytes = raw_archive(&[Raw::file(FILE_INDEX, bad_key)]);
    let error = extract_bytes(&bytes, &Limits::default()).unwrap_err();
    assert!(matches!(error, ArchiveError::UnsafePath(_)), "{error}");

    let empty = raw_archive(&[]);
    let error = extract_bytes(&empty, &Limits::default()).unwrap_err();
    assert!(matches!(error, ArchiveError::IndexNotFirst), "{error}");
}

#[test]
fn contents_must_match_the_index() {
    let mut index = index_of(&[("a", BODY, 0o644)]);
    index.get_mut("a").unwrap().sha256 = "0".repeat(64);
    let listing = index_json(&index);
    let bytes = raw_archive(&[Raw::file(FILE_INDEX, &listing), Raw::file("a", BODY)]);
    let error = extract_bytes(&bytes, &Limits::default()).unwrap_err();
    assert!(matches!(error, ArchiveError::Mismatch(_)), "{error}");

    let error = listed_with(Raw::file("a", b"longer payload"), &[("a", BODY, 0o644)]).unwrap_err();
    assert!(matches!(error, ArchiveError::Mismatch(_)), "{error}");

    let error = listed_with(
        Raw {
            mode: 0o755,
            ..Raw::file("a", BODY)
        },
        &[("a", BODY, 0o644)],
    )
    .unwrap_err();
    assert!(matches!(error, ArchiveError::Mismatch(_)), "{error}");

    let error = listed_with(
        Raw::file("a", BODY),
        &[("a", BODY, 0o644), ("b", BODY, 0o644)],
    )
    .unwrap_err();
    assert!(
        matches!(error, ArchiveError::Missing(ref path) if path == "b"),
        "{error}"
    );
}

#[test]
fn entry_count_and_unpacked_size_are_capped() {
    let files: Vec<(String, &[u8], u32)> = (0..4).map(|n| (format!("f{n}"), BODY, 0o644)).collect();
    let borrowed: Vec<(&str, &[u8], u32)> =
        files.iter().map(|(p, d, m)| (p.as_str(), *d, *m)).collect();
    let index = index_json(&index_of(&borrowed));
    let mut entries = vec![Raw::file(FILE_INDEX, &index)];
    entries.extend(borrowed.iter().map(|(path, data, _)| Raw::file(path, data)));
    let bytes = raw_archive(&entries);
    assert!(extract_bytes(&bytes, &Limits::default()).is_ok());

    let few = Limits {
        max_entries: 4,
        ..Limits::default()
    };
    let error = extract_bytes(&bytes, &few).unwrap_err();
    assert!(matches!(error, ArchiveError::TooManyEntries(4)), "{error}");

    let small = Limits {
        max_unpacked_bytes: 20,
        ..Limits::default()
    };
    let error = extract_bytes(&bytes, &small).unwrap_err();
    assert!(matches!(error, ArchiveError::TooLarge(20)), "{error}");

    let tiny_index = Limits {
        max_index_bytes: 8,
        ..Limits::default()
    };
    let error = extract_bytes(&bytes, &tiny_index).unwrap_err();
    assert!(matches!(error, ArchiveError::InvalidIndex(_)), "{error}");
}

#[test]
fn packing_rejects_symlinks() {
    let source = tempfile::tempdir().unwrap();
    write_tree(source.path(), &[("manifest.json", b"{}", 0o644)]);
    std::os::unix::fs::symlink("/etc/passwd", source.path().join("leak")).unwrap();
    let error = index_tree(source.path(), &Limits::default()).unwrap_err();
    assert!(
        matches!(
            error,
            ArchiveError::NotRegular {
                kind: "symlink",
                ..
            }
        ),
        "{error}"
    );
}

fn segment() -> impl Strategy<Value = String> {
    "[a-z0-9_.-]{1,12}".prop_filter("normal segment", |s| s != "." && s != "..")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn paths_with_traversal_are_always_rejected(
        before in proptest::collection::vec(segment(), 0..4),
        after in proptest::collection::vec(segment(), 1..4),
        absolute in any::<bool>(),
    ) {
        let mut parts = before;
        if !absolute {
            parts.push("..".to_string());
        }
        parts.extend(after);
        let joined = parts.join("/");
        let path = if absolute { format!("/{joined}") } else { joined };
        prop_assume!(path.len() < 100);
        let error = listed_with(Raw::file(&path, BODY), &[("payload", BODY, 0o644)]).unwrap_err();
        prop_assert!(matches!(error, ArchiveError::UnsafePath(_)), "{}: {}", path, error);
    }

    #[test]
    fn any_tree_round_trips(
        files in proptest::collection::btree_map(
            proptest::collection::vec(segment(), 1..4).prop_map(|parts| parts.join("/")),
            (proptest::collection::vec(any::<u8>(), 0..256), any::<bool>()),
            1..12,
        )
    ) {
        let names: Vec<&String> = files.keys().collect();
        prop_assume!(!names.iter().any(|name| name.as_str() == FILE_INDEX
            || names.iter().any(|other| other.starts_with(&format!("{name}/")))));
        let source = tempfile::tempdir().unwrap();
        let tree: Vec<(&str, &[u8], u32)> = files
            .iter()
            .map(|(path, (data, executable))| {
                (path.as_str(), data.as_slice(), if *executable { 0o755 } else { 0o644 })
            })
            .collect();
        write_tree(source.path(), &tree);
        let mut bytes = Vec::new();
        let index = pack(source.path(), &mut bytes).unwrap();
        prop_assert_eq!(&index, &index_of(&tree));
        let target = tempfile::tempdir().unwrap();
        let extracted = extract(bytes.as_slice(), target.path(), &Limits::default()).unwrap();
        prop_assert_eq!(extracted, index);
    }
}
