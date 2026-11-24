use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Family {
    Cache,
    Project,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: &'static str,
    pub title: &'static str,
    pub cost: &'static str,
    pub family: Family,
    pub on_by_default: bool,
    pub paths: &'static [&'static str],
    pub directory_names: &'static [&'static str],
}

pub const CATEGORIES: &[Category] = &[
    Category {
        id: "npm",
        title: "npm cache",
        cost: "Tarballs are re-downloaded on the next install.",
        family: Family::Cache,
        on_by_default: true,
        paths: &[".npm/_cacache"],
        directory_names: &[],
    },
    Category {
        id: "yarn",
        title: "Yarn cache",
        cost: "Re-downloaded on the next install.",
        family: Family::Cache,
        on_by_default: true,
        paths: &[".cache/yarn"],
        directory_names: &[],
    },
    Category {
        id: "pnpm",
        title: "pnpm store",
        cost: "Re-downloaded on the next install; other projects linking the \
               store re-fetch too.",
        family: Family::Cache,
        on_by_default: false,
        paths: &[".local/share/pnpm/store"],
        directory_names: &[],
    },
    Category {
        id: "bun",
        title: "Bun cache",
        cost: "Re-downloaded on the next install.",
        family: Family::Cache,
        on_by_default: true,
        paths: &[".bun/install/cache"],
        directory_names: &[],
    },
    Category {
        id: "pip",
        title: "pip cache",
        cost: "Wheels are re-downloaded on the next install.",
        family: Family::Cache,
        on_by_default: true,
        paths: &[".cache/pip"],
        directory_names: &[],
    },
    Category {
        id: "cargo",
        title: "Cargo registry",
        cost: "Crates are fetched and unpacked again on the next build. \
               Installed binaries under ~/.cargo/bin are not touched.",
        family: Family::Cache,
        on_by_default: true,
        paths: &[
            ".cargo/registry/cache",
            ".cargo/registry/src",
            ".cargo/git/checkouts",
        ],
        directory_names: &[],
    },
    Category {
        id: "go",
        title: "Go build cache",
        cost: "The next build is a full one.",
        family: Family::Cache,
        on_by_default: true,
        paths: &[".cache/go-build"],
        directory_names: &[],
    },
    Category {
        id: "playwright",
        title: "Playwright browsers",
        cost: "The next test run downloads several hundred megabytes of \
               browsers before it can start.",
        family: Family::Cache,
        on_by_default: false,
        paths: &[".cache/ms-playwright"],
        directory_names: &[],
    },
    Category {
        id: "puppeteer",
        title: "Puppeteer browsers",
        cost: "The next run downloads Chromium again.",
        family: Family::Cache,
        on_by_default: false,
        paths: &[".cache/puppeteer"],
        directory_names: &[],
    },
    Category {
        id: "claudeCode",
        title: "Claude Code logs",
        cost: "Nothing you would miss. Transcripts, projects and settings \
               elsewhere under ~/.claude are not touched.",
        family: Family::Cache,
        on_by_default: true,
        paths: &[".claude/debug", ".claude/shell-snapshots"],
        directory_names: &[],
    },
    Category {
        id: "thumbnails",
        title: "Thumbnail cache",
        cost: "Nothing. The file manager regenerates a thumbnail the next time \
               it shows the folder.",
        family: Family::Cache,
        on_by_default: true,
        paths: &[".cache/thumbnails"],
        directory_names: &[],
    },
    Category {
        id: "nodeModules",
        title: "node_modules",
        cost: "An install restores it, network permitting. Anything patched in \
               place is gone.",
        family: Family::Project,
        on_by_default: false,
        paths: &[],
        directory_names: &["node_modules"],
    },
    Category {
        id: "pycache",
        title: "__pycache__",
        cost: "Nothing, Python regenerates them.",
        family: Family::Project,
        on_by_default: true,
        paths: &[],
        directory_names: &["__pycache__"],
    },
    Category {
        id: "pyvenv",
        title: "Python virtual environments",
        cost: "The whole environment: the interpreter links, every installed \
               package, and any script you dropped in bin. Recreated from a \
               lockfile, if you have one.",
        family: Family::Project,
        on_by_default: false,
        paths: &[],
        directory_names: &[".venv", "venv"],
    },
    Category {
        id: "rustTarget",
        title: "Cargo target directories",
        cost: "A full rebuild.",
        family: Family::Project,
        on_by_default: false,
        paths: &[],
        directory_names: &["target"],
    },
    Category {
        id: "gradle",
        title: "Gradle project caches",
        cost: "A slower next build.",
        family: Family::Project,
        on_by_default: true,
        paths: &[],
        directory_names: &[".gradle"],
    },
    Category {
        id: "nextBuild",
        title: "Next.js build output",
        cost: "A rebuild.",
        family: Family::Project,
        on_by_default: true,
        paths: &[],
        directory_names: &[".next"],
    },
    Category {
        id: "turbo",
        title: "Turborepo cache",
        cost: "Cache misses on the next run.",
        family: Family::Project,
        on_by_default: true,
        paths: &[],
        directory_names: &[".turbo"],
    },
];

pub fn category(id: &str) -> Option<&'static Category> {
    CATEGORIES.iter().find(|entry| entry.id == id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub category: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub items: Vec<Item>,
    pub totals: BTreeMap<String, u64>,
    pub total_bytes: u64,
}

impl Scan {
    fn from_items(mut items: Vec<Item>) -> Self {
        items.sort_by(|left, right| {
            right
                .bytes
                .cmp(&left.bytes)
                .then(left.path.cmp(&right.path))
        });
        let mut totals: BTreeMap<String, u64> = BTreeMap::new();
        let mut total_bytes = 0_u64;
        for item in &items {
            *totals.entry(item.category.clone()).or_default() += item.bytes;
            total_bytes = total_bytes.saturating_add(item.bytes);
        }
        Self {
            items,
            totals,
            total_bytes,
        }
    }
}

pub fn directory_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
}

pub fn scan_caches(home: &Path, selected: Option<&[String]>) -> Scan {
    let mut items = Vec::new();
    for category in CATEGORIES
        .iter()
        .filter(|entry| entry.family == Family::Cache)
        .filter(|entry| selected.is_none_or(|ids| ids.iter().any(|id| id == entry.id)))
    {
        for relative in category.paths {
            let path = home.join(relative);
            if !path.is_dir() {
                continue;
            }
            let bytes = directory_size(&path);
            if bytes == 0 {
                continue;
            }
            items.push(Item {
                category: category.id.to_string(),
                path: path.display().to_string(),
                bytes,
            });
        }
    }
    Scan::from_items(items)
}

pub const MAX_PROJECT_DEPTH: usize = 8;

pub fn scan_projects(root: &Path, selected: Option<&[String]>) -> Result<Scan> {
    if !root.is_dir() {
        anyhow::bail!("not a directory: {}", root.display());
    }
    let wanted: Vec<(&'static str, &'static str)> = CATEGORIES
        .iter()
        .filter(|entry| entry.family == Family::Project)
        .filter(|entry| selected.is_none_or(|ids| ids.iter().any(|id| id == entry.id)))
        .flat_map(|entry| {
            entry
                .directory_names
                .iter()
                .map(move |name| (entry.id, *name))
        })
        .collect();

    let mut items = Vec::new();
    let mut walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(MAX_PROJECT_DEPTH)
        .into_iter();

    while let Some(entry) = walker.next() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        let Some((id, _)) = wanted.iter().find(|(_, wanted)| *wanted == name) else {
            continue;
        };
        walker.skip_current_dir();
        let bytes = directory_size(entry.path());
        if bytes == 0 {
            continue;
        }
        items.push(Item {
            category: id.to_string(),
            path: entry.path().display().to_string(),
            bytes,
        });
    }
    Ok(Scan::from_items(items))
}

pub fn trash_dir(home: &Path) -> PathBuf {
    match std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value).join("Trash"),
        None => home.join(".local/share/Trash"),
    }
}

pub fn trash(home: &Path, path: &Path) -> Result<String> {
    let root = trash_dir(home);
    let files = root.join("files");
    let info = root.join("info");
    std::fs::create_dir_all(&files)
        .with_context(|| format!("cannot create {}", files.display()))?;
    std::fs::create_dir_all(&info).with_context(|| format!("cannot create {}", info.display()))?;

    let original = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "item".to_string());

    let mut name = original.clone();
    let mut suffix = 1;
    while files.join(&name).exists() || info.join(format!("{name}.trashinfo")).exists() {
        name = format!("{original}.{suffix}");
        suffix += 1;
        if suffix > 10_000 {
            anyhow::bail!("cannot find a free name in the Trash for {original}");
        }
    }

    let deletion_date = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    std::fs::write(
        info.join(format!("{name}.trashinfo")),
        format!(
            "[Trash Info]\nPath={}\nDeletionDate={deletion_date}\n",
            encode_path(path)
        ),
    )
    .context("cannot record the trashed item's original location")?;

    match std::fs::rename(path, files.join(&name)) {
        Ok(()) => Ok(name),
        Err(error) => {
            let _ = std::fs::remove_file(info.join(format!("{name}.trashinfo")));
            Err(error).with_context(|| {
                format!(
                    "cannot move {} to the Trash. The Trash has to be on the same \
                     filesystem as the item for a move to work.",
                    path.display()
                )
            })
        }
    }
}

fn encode_path(path: &Path) -> String {
    let mut out = String::new();
    for byte in path.display().to_string().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    pub trashed: Vec<Item>,
    pub failed: Vec<(String, String)>,
    pub bytes_reclaimed: u64,
}

pub fn clean(home: &Path, scan: &Scan) -> CleanReport {
    let mut report = CleanReport::default();
    for item in &scan.items {
        match trash(home, Path::new(&item.path)) {
            Ok(_) => {
                report.bytes_reclaimed = report.bytes_reclaimed.saturating_add(item.bytes);
                report.trashed.push(item.clone());
            }
            Err(error) => report
                .failed
                .push((item.path.clone(), format!("{error:#}"))),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "veronica-cleaner-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn every_category_has_a_unique_id_and_belongs_to_exactly_one_family() {
        let mut ids: Vec<&str> = CATEGORIES.iter().map(|entry| entry.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate category id");

        for entry in CATEGORIES {
            match entry.family {
                Family::Cache => {
                    assert!(!entry.paths.is_empty(), "{} has no paths", entry.id);
                    assert!(entry.directory_names.is_empty(), "{}", entry.id);
                }
                Family::Project => {
                    assert!(entry.paths.is_empty(), "{}", entry.id);
                    assert!(
                        !entry.directory_names.is_empty(),
                        "{} matches no directory name",
                        entry.id
                    );
                }
            }
        }
    }

    #[test]
    fn every_category_says_what_removing_it_costs() {
        for entry in CATEGORIES {
            assert!(!entry.cost.is_empty(), "{} has no cost", entry.id);
            assert!(!entry.title.is_empty(), "{} has no title", entry.id);
        }
    }

    #[test]
    fn no_cache_path_escapes_the_home_directory() {
        for entry in CATEGORIES {
            for path in entry.paths {
                assert!(!path.starts_with('/'), "{} is absolute: {path}", entry.id);
                assert!(!path.contains(".."), "{} escapes home: {path}", entry.id);
            }
        }
    }

    #[test]
    fn a_scan_measures_the_caches_that_exist_and_skips_the_ones_that_do_not() {
        let home = scratch("caches");
        write(&home.join(".npm/_cacache/a"), 1_000);
        write(&home.join(".cache/pip/b"), 500);

        let scan = scan_caches(&home, None);
        assert_eq!(scan.total_bytes, 1_500);
        assert_eq!(scan.totals.get("npm"), Some(&1_000));
        assert_eq!(scan.totals.get("pip"), Some(&500));
        assert_eq!(
            scan.totals.get("go"),
            None,
            "an absent cache is not a zero row"
        );
        assert_eq!(scan.items[0].category, "npm");
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_scan_can_be_narrowed_to_named_categories() {
        let home = scratch("selected");
        write(&home.join(".npm/_cacache/a"), 1_000);
        write(&home.join(".cache/pip/b"), 500);

        let scan = scan_caches(&home, Some(&["pip".to_string()]));
        assert_eq!(scan.total_bytes, 500);
        assert_eq!(scan.items.len(), 1);
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_project_sweep_finds_directories_by_name() {
        let root = scratch("projects");
        write(&root.join("app/node_modules/left-pad/index.js"), 800);
        write(&root.join("app/src/main.rs"), 100);
        write(&root.join("service/__pycache__/x.pyc"), 50);

        let scan = scan_projects(&root, None).unwrap();
        let categories: Vec<&str> = scan.items.iter().map(|i| i.category.as_str()).collect();
        assert!(categories.contains(&"nodeModules"));
        assert!(categories.contains(&"pycache"));
        assert_eq!(scan.total_bytes, 850, "source files are not swept");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_nested_match_is_not_counted_twice() {
        let root = scratch("nested");
        write(
            &root.join("node_modules/pkg/node_modules/dep/index.js"),
            400,
        );

        let scan = scan_projects(&root, None).unwrap();
        assert_eq!(scan.items.len(), 1);
        assert_eq!(scan.total_bytes, 400);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_root_that_is_not_a_directory_is_an_error_rather_than_an_empty_scan() {
        assert!(scan_projects(Path::new("/nonexistent/veronica/root"), None).is_err());
    }

    #[test]
    fn trashing_moves_the_item_and_records_where_it_came_from() {
        let home = scratch("trash");
        let victim = home.join("project/node_modules");
        write(&victim.join("index.js"), 10);

        let name = trash(&home, &victim).unwrap();
        assert!(!victim.exists(), "the original is gone");
        assert!(trash_dir(&home).join("files").join(&name).exists());

        let info = std::fs::read_to_string(
            trash_dir(&home)
                .join("info")
                .join(format!("{name}.trashinfo")),
        )
        .unwrap();
        assert!(info.starts_with("[Trash Info]"), "got {info}");
        assert!(info.contains("Path="), "got {info}");
        assert!(info.contains("DeletionDate="), "got {info}");
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_second_item_of_the_same_name_does_not_overwrite_the_first() {
        let home = scratch("collide");
        write(&home.join("a/target/x"), 10);
        write(&home.join("b/target/y"), 10);

        let first = trash(&home, &home.join("a/target")).unwrap();
        let second = trash(&home, &home.join("b/target")).unwrap();
        assert_ne!(first, second);
        assert!(trash_dir(&home)
            .join("files")
            .join(&first)
            .join("x")
            .exists());
        assert!(trash_dir(&home)
            .join("files")
            .join(&second)
            .join("y")
            .exists());
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_path_with_spaces_is_encoded_the_way_the_spec_requires() {
        assert_eq!(
            encode_path(Path::new("/home/me/My Files/a+b")),
            "/home/me/My%20Files/a%2Bb"
        );
    }

    #[test]
    fn cleaning_reports_what_it_moved_and_keeps_going_past_a_failure() {
        let home = scratch("clean");
        write(&home.join("app/node_modules/a"), 100);
        let scan = Scan::from_items(vec![
            Item {
                category: "nodeModules".into(),
                path: home.join("app/node_modules").display().to_string(),
                bytes: 100,
            },
            Item {
                category: "nodeModules".into(),
                path: home.join("gone/node_modules").display().to_string(),
                bytes: 50,
            },
        ]);

        let report = clean(&home, &scan);
        assert_eq!(report.trashed.len(), 1);
        assert_eq!(
            report.bytes_reclaimed, 100,
            "only what actually moved counts"
        );
        assert_eq!(report.failed.len(), 1, "and the failure is reported");
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_failed_move_leaves_no_orphan_info_file_behind() {
        let home = scratch("orphan");
        let missing = home.join("never/existed");
        assert!(trash(&home, &missing).is_err());
        let info = trash_dir(&home).join("info");
        let orphans = std::fs::read_dir(&info)
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(orphans, 0);
        std::fs::remove_dir_all(&home).ok();
    }
}
