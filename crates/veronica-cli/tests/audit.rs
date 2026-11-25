mod common;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

const PAGE: &str = "<!doctype html><html lang=\"en\"><head><title>A small test page for the audit</title></head><body><h1>Hello</h1><p>Just enough words to read as a page.</p></body></html>";

fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let sitemap = format!(
        "<?xml version=\"1.0\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"><url><loc>{origin}/</loc></url><url><loc>{origin}/about</loc></url></urlset>"
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = String::new();
            let mut reader = BufReader::new(&stream);
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (status, kind, body) = match path.as_str() {
                "/robots.txt" => ("404 Not Found", "text/plain", String::new()),
                "/sitemap.xml" => ("200 OK", "application/xml", sitemap.clone()),
                _ => ("200 OK", "text/html; charset=utf-8", PAGE.to_string()),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    origin
}

struct Home {
    root: TempDir,
}

impl Home {
    fn with_audit() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "audit"))
            .unwrap();
        home
    }

    fn vr(&self, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_vr"));
        for key in [
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_STATE_HOME",
            "XDG_RUNTIME_DIR",
        ] {
            command.env_remove(key);
        }
        command
            .env("HOME", self.root.path())
            .env("NO_COLOR", "1")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

fn document(output: &Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn the_audit_commands_come_from_the_installed_extension() {
    let home = Home::with_audit();
    let help = home.vr(&["audit", "--help"]);
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(
        text.contains("Site Audit: crawl a site's sitemap"),
        "{text}"
    );
    for leaf in ["site", "page", "sitemap"] {
        assert!(text.contains(leaf), "{text}");
    }
    let leaf = home.vr(&["audit", "site", "--help"]);
    assert!(String::from_utf8_lossy(&leaf.stdout).contains("Usage: vr audit site [OPTIONS] <SITE>"));
}

#[test]
fn a_page_is_audited_through_the_backend() {
    let origin = serve();
    let home = Home::with_audit();
    let page = document(&home.vr(&["--json", "audit", "page", &format!("{origin}/")]));
    assert_eq!(page["url"], format!("{origin}/"));
    assert_eq!(page["statusCode"], 200);
    assert_eq!(page["metadata"]["title"], "A small test page for the audit");

    let aliased = document(&home.vr(&["seo", "page", &format!("{origin}/"), "--json"]));
    assert_eq!(aliased["url"], page["url"]);
}

#[test]
fn a_sitemap_lists_its_pages() {
    let origin = serve();
    let home = Home::with_audit();
    let listed = document(&home.vr(&["audit", "sitemap", &origin, "--json"]));
    assert_eq!(
        listed["pages"],
        serde_json::json!([format!("{origin}/"), format!("{origin}/about")])
    );
    let text = home.vr(&["audit", "sitemap", &origin]);
    assert_eq!(
        String::from_utf8_lossy(&text.stdout),
        format!("{origin}/\n{origin}/about\n")
    );
}
