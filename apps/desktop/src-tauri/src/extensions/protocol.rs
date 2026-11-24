use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tauri::http::{header, Method, Request, Response, StatusCode};

pub(crate) const SCHEME: &str = "veronica-ext";
const DEVELOPMENT_ROOT: &str = "VERONICA_EXTENSIONS_DEV";

type Body = Cow<'static, [u8]>;

#[derive(Debug, Default)]
pub(crate) struct Bundles {
    roots: BTreeMap<String, PathBuf>,
}

impl Bundles {
    pub(crate) fn discover() -> Self {
        if !cfg!(debug_assertions) {
            return Self::default();
        }
        std::env::var_os(DEVELOPMENT_ROOT)
            .map(PathBuf::from)
            .map(|directory| Self::from_directory(&directory))
            .unwrap_or_default()
    }

    fn from_directory(directory: &Path) -> Self {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Self::default();
        };
        let roots = entries
            .flatten()
            .filter_map(|entry| {
                let id = entry.file_name().into_string().ok()?;
                let root = entry.path().canonicalize().ok()?;
                (is_extension_id(&id) && root.is_dir()).then_some((id, root))
            })
            .collect();
        Self { roots }
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
        let Ok(bytes) = std::fs::read(&file) else {
            return empty(StatusCode::NOT_FOUND);
        };
        let body = if request.method() == Method::HEAD {
            Vec::new()
        } else {
            bytes
        };
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_type(&file))
            .header(header::CONTENT_SECURITY_POLICY, content_security_policy(id))
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "null")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Cow::Owned(body))
            .unwrap_or_else(|_| empty(StatusCode::INTERNAL_SERVER_ERROR))
    }

    fn resolve(&self, id: &str, path: &str) -> Option<PathBuf> {
        let root = self.roots.get(id)?;
        let relative = relative_path(path)?;
        let file = root.join(relative).canonicalize().ok()?;
        (file.starts_with(root) && file.is_file()).then_some(file)
    }
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
