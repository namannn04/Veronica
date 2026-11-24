use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use tauri::http::{header, Method, Request, Response, StatusCode};
use veronica_host::registry::Registry;
use veronica_host::Layout;
use veronica_protocol::Manifest;

pub(crate) const SCHEME: &str = "veronica-ext";
const DEVELOPMENT_ROOT: &str = "VERONICA_EXTENSIONS_DEV";
const RANGE_CHUNK: u64 = 4 * 1024 * 1024;

type Body = Cow<'static, [u8]>;

#[derive(Debug, Default)]
pub(crate) struct Bundles {
    development: BTreeMap<String, PathBuf>,
    installed: Option<Layout>,
    interfaces: Mutex<HashMap<PathBuf, Option<PathBuf>>>,
}

struct Content {
    status: StatusCode,
    bytes: Vec<u8>,
    range: Option<String>,
}

impl Bundles {
    pub(crate) fn discover(layout: Layout) -> Self {
        let development = if cfg!(debug_assertions) {
            std::env::var_os(DEVELOPMENT_ROOT)
                .map(PathBuf::from)
                .map(|directory| Self::from_directory(&directory).development)
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        Self {
            development,
            installed: Some(layout),
            interfaces: Mutex::default(),
        }
    }

    fn from_directory(directory: &Path) -> Self {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Self::default();
        };
        let development = entries
            .flatten()
            .filter_map(|entry| {
                let id = entry.file_name().into_string().ok()?;
                let root = entry.path().canonicalize().ok()?;
                (is_extension_id(&id) && root.is_dir()).then_some((id, root))
            })
            .collect();
        Self {
            development,
            ..Self::default()
        }
    }

    pub(crate) fn respond<T>(&self, request: &Request<T>) -> Response<Body> {
        if !matches!(*request.method(), Method::GET | Method::HEAD) {
            return empty(StatusCode::METHOD_NOT_ALLOWED);
        }
        let Some(id) = request.uri().host() else {
            return empty(StatusCode::NOT_FOUND);
        };
        let Some(file) = self.resolve(id, request.uri().path()) else {
            return empty(StatusCode::NOT_FOUND);
        };
        let range = request
            .headers()
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok());
        let Ok(content) = read(&file, range) else {
            return empty(StatusCode::NOT_FOUND);
        };
        let body = if request.method() == Method::HEAD {
            Vec::new()
        } else {
            content.bytes
        };
        let mut response = Response::builder()
            .status(content.status)
            .header(header::CONTENT_TYPE, mime_type(&file))
            .header(header::CONTENT_SECURITY_POLICY, content_security_policy(id))
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "null")
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CACHE_CONTROL, "no-store");
        if let Some(range) = content.range {
            response = response.header(header::CONTENT_RANGE, range);
        }
        response
            .body(Cow::Owned(body))
            .unwrap_or_else(|_| empty(StatusCode::INTERNAL_SERVER_ERROR))
    }

    fn resolve(&self, id: &str, path: &str) -> Option<PathBuf> {
        let root = match self.development.get(id) {
            Some(root) => root.clone(),
            None => self.installed_interface(id)?,
        };
        let relative = relative_path(path)?;
        let file = root.join(relative).canonicalize().ok()?;
        (file.starts_with(&root) && file.is_file()).then_some(file)
    }

    fn installed_interface(&self, id: &str) -> Option<PathBuf> {
        let layout = self.installed.as_ref()?;
        if !is_extension_id(id) {
            return None;
        }
        let registry = Registry::load(&layout.installed_file()).ok()?;
        if !registry.get(id)?.enabled {
            return None;
        }
        let install = layout.current_link(id).canonicalize().ok()?;
        let mut interfaces = self
            .interfaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(known) = interfaces.get(&install) {
            return known.clone();
        }
        let interface = declared_interface(&install);
        interfaces.insert(install, interface.clone());
        interface
    }
}

fn declared_interface(install: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(install.join("manifest.json")).ok()?;
    let declared = Manifest::parse(&raw).ok()?.ui?.root;
    let relative = relative_path(&format!("/{declared}"))?;
    let root = install.join(relative).canonicalize().ok()?;
    (root.starts_with(install) && root.is_dir()).then_some(root)
}

fn byte_range(requested: &str, length: u64) -> Option<(u64, u64)> {
    let (first, last) = requested.strip_prefix("bytes=")?.split_once('-')?;
    let final_byte = length.checked_sub(1)?;
    let (start, end) = if first.is_empty() {
        (length.saturating_sub(last.parse().ok()?), final_byte)
    } else if last.is_empty() {
        (first.parse().ok()?, final_byte)
    } else {
        (
            first.parse().ok()?,
            last.parse::<u64>().ok()?.min(final_byte),
        )
    };
    (start <= end).then(|| (start, end.min(start.saturating_add(RANGE_CHUNK - 1))))
}

fn read(file: &Path, range: Option<&str>) -> std::io::Result<Content> {
    let mut handle = File::open(file)?;
    let length = handle.metadata()?.len();
    let mut bytes = Vec::new();
    let Some(requested) = range else {
        handle.read_to_end(&mut bytes)?;
        return Ok(Content {
            status: StatusCode::OK,
            bytes,
            range: None,
        });
    };
    let Some((start, end)) = byte_range(requested, length) else {
        return Ok(Content {
            status: StatusCode::RANGE_NOT_SATISFIABLE,
            bytes,
            range: Some(format!("bytes */{length}")),
        });
    };
    handle.seek(SeekFrom::Start(start))?;
    handle.take(end - start + 1).read_to_end(&mut bytes)?;
    Ok(Content {
        status: StatusCode::PARTIAL_CONTENT,
        bytes,
        range: Some(format!("bytes {start}-{end}/{length}")),
    })
}

fn empty(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Cow::Borrowed(&[][..]));
    *response.status_mut() = status;
    response
}

fn is_extension_id(id: &str) -> bool {
    let mut characters = id.chars();
    (2..=32).contains(&id.len())
        && characters
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn is_plain_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.starts_with('.')
        && segment.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

fn relative_path(path: &str) -> Option<PathBuf> {
    let segments: Vec<&str> = path.strip_prefix('/')?.split('/').collect();
    segments
        .iter()
        .all(|segment| is_plain_segment(segment))
        .then(|| segments.iter().collect())
}

fn content_security_policy(id: &str) -> String {
    let origin = format!("{SCHEME}://{id}");
    format!(
        "default-src 'none'; script-src {origin}; style-src {origin} 'unsafe-inline'; \
         img-src {origin} data: blob:; media-src {origin} blob:; font-src {origin} data:; \
         connect-src 'none'; form-action 'none'; base-uri 'none'"
    )
}

fn mime_type(file: &Path) -> &'static str {
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("mp3") => "audio/mpeg",
        Some("ogg") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/extensions");

    fn fixtures() -> Bundles {
        Bundles::from_directory(Path::new(FIXTURES))
    }

    fn get(bundles: &Bundles, uri: &str) -> Response<Body> {
        bundles.respond(&Request::get(uri).body(()).expect("request"))
    }

    fn header_value(response: &Response<Body>, name: header::HeaderName) -> &str {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
    }

    #[test]
    fn bundle_files_are_served_with_their_types() {
        let bundles = fixtures();
        for (file, mime) in [
            ("index.html", "text/html; charset=utf-8"),
            ("app.js", "text/javascript; charset=utf-8"),
            ("app.css", "text/css; charset=utf-8"),
            ("probe.woff2", "font/woff2"),
        ] {
            let response = get(&bundles, &format!("veronica-ext://hostile/{file}"));
            assert_eq!(response.status(), StatusCode::OK, "{file}");
            assert_eq!(header_value(&response, header::CONTENT_TYPE), mime);
            assert_eq!(
                header_value(&response, header::X_CONTENT_TYPE_OPTIONS),
                "nosniff"
            );
            assert!(!response.body().is_empty(), "{file}");
        }
    }

    #[test]
    fn every_response_confines_the_bundle_to_its_own_origin() {
        let response = get(&fixtures(), "veronica-ext://hostile/index.html");
        assert_eq!(
            header_value(&response, header::CONTENT_SECURITY_POLICY),
            "default-src 'none'; script-src veronica-ext://hostile; \
             style-src veronica-ext://hostile 'unsafe-inline'; \
             img-src veronica-ext://hostile data: blob:; \
             media-src veronica-ext://hostile blob:; \
             font-src veronica-ext://hostile data:; \
             connect-src 'none'; form-action 'none'; base-uri 'none'"
        );
        assert_eq!(
            header_value(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN),
            "null"
        );
    }

    #[test]
    fn unknown_extensions_and_files_are_not_found() {
        let bundles = fixtures();
        for uri in [
            "veronica-ext://absent/index.html",
            "veronica-ext://hostile/absent.js",
            "veronica-ext://hostile/",
            "veronica-ext://hostile",
            "veronica-ext://Hostile/index.html",
        ] {
            assert_eq!(get(&bundles, uri).status(), StatusCode::NOT_FOUND, "{uri}");
        }
        assert_eq!(
            get(&Bundles::default(), "veronica-ext://hostile/index.html").status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn traversal_never_leaves_the_bundle() {
        let bundles = fixtures();
        for path in [
            "/../neighbour/secret.js",
            "/%2e%2e/neighbour/secret.js",
            "/..%2fneighbour/secret.js",
            "/./index.html",
            "//index.html",
            "/assets/../../neighbour/secret.js",
            "/index.html%00.js",
            "/.hidden",
            "/assets\\..\\index.html",
        ] {
            assert_eq!(bundles.resolve("hostile", path), None, "{path}");
        }
        assert!(bundles.resolve("hostile", "/index.html").is_some());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_the_bundle() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let outside = directory.path().join("outside");
        let root = directory.path().join("extensions").join("linked");
        std::fs::create_dir_all(&outside).expect("outside directory");
        std::fs::create_dir_all(&root).expect("bundle directory");
        std::fs::write(outside.join("secret.txt"), "secret").expect("outside file");
        std::fs::write(root.join("inside.txt"), "inside").expect("bundle file");
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("file.txt"))
            .expect("file link");
        std::os::unix::fs::symlink(&outside, root.join("folder")).expect("folder link");
        std::os::unix::fs::symlink(root.join("inside.txt"), root.join("alias.txt"))
            .expect("inner link");

        let bundles = Bundles::from_directory(&directory.path().join("extensions"));
        assert_eq!(bundles.resolve("linked", "/file.txt"), None);
        assert_eq!(bundles.resolve("linked", "/folder/secret.txt"), None);
        assert!(bundles.resolve("linked", "/alias.txt").is_some());
        assert!(bundles.resolve("linked", "/inside.txt").is_some());
    }

    fn installed(root: &Path, enabled: bool, declared: &str) -> Bundles {
        let directories = veronica_core::AppDirectories {
            configuration: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
            runtime: root.join("runtime"),
            state: root.join("state"),
        };
        let layout = Layout::new(&directories);
        let version = "1.0.0".parse().expect("version");
        let tree = layout.version_dir("probe", version);
        std::fs::create_dir_all(tree.join("ui/assets")).expect("interface");
        std::fs::create_dir_all(tree.join("bin")).expect("backend directory");
        std::fs::write(tree.join("ui/index.html"), "<!doctype html>").expect("page");
        std::fs::write(tree.join("ui/assets/tone.mp3"), b"0123456789").expect("media");
        std::fs::write(tree.join("bin/backend"), "secret").expect("backend");
        let manifest = serde_json::json!({
            "schema": 1, "id": "probe", "version": "1.0", "title": "Probe",
            "summary": "A test extension.", "category": "labs",
            "engines": { "host": ">=0.1.0", "hostApi": 1 },
            "targets": ["linux-x86_64"],
            "ui": { "root": declared, "pages": [{ "id": "main", "title": "Probe", "entry": "index.html" }] }
        });
        std::fs::write(tree.join("manifest.json"), manifest.to_string()).expect("manifest");
        std::os::unix::fs::symlink(&tree, layout.current_link("probe")).expect("current link");
        let mut registry = Registry::default();
        registry.extensions.insert(
            "probe".to_string(),
            veronica_host::registry::InstalledExtension {
                version,
                origin: veronica_host::registry::Origin::Catalog,
                enabled,
                installed_at: "2026-11-24T12:00:00Z".to_string(),
                previous: None,
                quarantined: std::collections::BTreeSet::new(),
            },
        );
        registry.save(&layout.installed_file()).expect("registry");
        Bundles {
            installed: Some(layout),
            ..Bundles::default()
        }
    }

    #[test]
    fn installed_extensions_serve_only_their_declared_interface() {
        let root = tempfile::tempdir().expect("temporary directory");
        let bundles = installed(root.path(), true, "ui");
        let page = get(&bundles, "veronica-ext://probe/index.html");
        assert_eq!(page.status(), StatusCode::OK);
        assert!(header_value(&page, header::CONTENT_SECURITY_POLICY)
            .contains("script-src veronica-ext://probe;"));
        for uri in [
            "veronica-ext://probe/manifest.json",
            "veronica-ext://probe/bin/backend",
            "veronica-ext://probe/../bin/backend",
            "veronica-ext://absent/index.html",
        ] {
            assert_eq!(get(&bundles, uri).status(), StatusCode::NOT_FOUND, "{uri}");
        }
    }

    #[test]
    fn disabled_extensions_and_escaping_roots_serve_nothing() {
        let root = tempfile::tempdir().expect("temporary directory");
        let disabled = installed(root.path(), false, "ui");
        assert_eq!(
            get(&disabled, "veronica-ext://probe/index.html").status(),
            StatusCode::NOT_FOUND
        );
        for declared in ["..", "/", "ui/../bin", "absent"] {
            let root = tempfile::tempdir().expect("temporary directory");
            let bundles = installed(root.path(), true, declared);
            for file in ["index.html", "backend", "manifest.json"] {
                let uri = format!("veronica-ext://probe/{file}");
                assert_eq!(
                    get(&bundles, &uri).status(),
                    StatusCode::NOT_FOUND,
                    "{declared} {file}"
                );
            }
        }
    }

    #[test]
    fn media_is_served_in_byte_ranges() {
        let root = tempfile::tempdir().expect("temporary directory");
        let bundles = installed(root.path(), true, "ui");
        let ranged = |range: &str| {
            let request = Request::get("veronica-ext://probe/assets/tone.mp3")
                .header(header::RANGE, range)
                .body(())
                .expect("request");
            bundles.respond(&request)
        };
        for (range, body, described) in [
            ("bytes=2-5", "2345", "bytes 2-5/10"),
            ("bytes=7-", "789", "bytes 7-9/10"),
            ("bytes=-3", "789", "bytes 7-9/10"),
            ("bytes=8-99", "89", "bytes 8-9/10"),
        ] {
            let response = ranged(range);
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT, "{range}");
            assert_eq!(response.body().as_ref(), body.as_bytes(), "{range}");
            assert_eq!(header_value(&response, header::CONTENT_RANGE), described);
        }
        for range in [
            "bytes=10-",
            "bytes=5-2",
            "bytes=0-1,4-5",
            "items=0-1",
            "bytes=-0",
        ] {
            let response = ranged(range);
            assert_eq!(
                response.status(),
                StatusCode::RANGE_NOT_SATISFIABLE,
                "{range}"
            );
            assert_eq!(header_value(&response, header::CONTENT_RANGE), "bytes */10");
        }
        assert_eq!(
            byte_range("bytes=0-", RANGE_CHUNK * 3),
            Some((0, RANGE_CHUNK - 1))
        );
    }

    #[test]
    fn only_reads_are_answered() {
        let bundles = fixtures();
        let post = Request::post("veronica-ext://hostile/index.html")
            .body(())
            .expect("request");
        assert_eq!(
            bundles.respond(&post).status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        let head = Request::head("veronica-ext://hostile/index.html")
            .body(())
            .expect("request");
        let response = bundles.respond(&head);
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.body().is_empty());
    }

    #[test]
    fn extension_ids_follow_the_manifest_rule() {
        for id in ["media", "seo-audit", "a1"] {
            assert!(is_extension_id(id), "{id}");
        }
        for id in ["", "a", "1media", "Media", "me_dia", "-media", "me.dia"] {
            assert!(!is_extension_id(id), "{id}");
        }
        assert!(!is_extension_id(&"a".repeat(33)));
    }
}
