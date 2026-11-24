use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{ETAG, IF_NONE_MATCH};
use reqwest::{StatusCode, Url};
use sha2::{Digest, Sha256};
use veronica_protocol::catalog::{TargetArtifact, PACKAGE_URL_PREFIX};

use crate::archive::is_sha256;
use crate::layout::Layout;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_mins(10);
const CHUNK: usize = 64 * 1024;

pub enum Response {
    NotModified,
    Body {
        etag: Option<String>,
        body: Box<dyn Read + Send>,
    },
}

pub trait Transport {
    fn get(&self, url: &str, etag: Option<&str>) -> Result<Response>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error("{0} is outside the allowed package location")]
    UrlNotAllowed(String),
    #[error("{0:?} is not a sha256 digest")]
    BadDigest(String),
    #[error("the server answered not modified for {0}, which was never cached")]
    UnexpectedNotModified(String),
    #[error("download is larger than the expected {expected} bytes")]
    TooLarge { expected: u64 },
    #[error("download is {actual} bytes, expected {expected}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("download has sha256 {actual}, expected {expected}")]
    HashMismatch { expected: String, actual: String },
}

#[derive(Debug, Clone)]
pub struct HttpTransport {
    client: Client,
}

impl HttpTransport {
    pub fn new() -> Result<Self> {
        Self::build(Client::builder().https_only(true))
    }

    fn build(builder: ClientBuilder) -> Result<Self> {
        let client = builder
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("veronica/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("cannot create the HTTP client")?;
        Ok(Self { client })
    }
}

impl Transport for HttpTransport {
    fn get(&self, url: &str, etag: Option<&str>) -> Result<Response> {
        let mut request = self.client.get(url);
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        let response = request
            .send()
            .with_context(|| format!("cannot reach {url}"))?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(Response::NotModified);
        }
        let response = response
            .error_for_status()
            .with_context(|| format!("cannot download {url}"))?;
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        Ok(Response::Body {
            etag,
            body: Box::new(response),
        })
    }
}

pub fn is_allowed_package_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|parsed| {
        parsed.query().is_none()
            && parsed.fragment().is_none()
            && parsed.as_str() == url
            && url.starts_with(PACKAGE_URL_PREFIX)
    })
}

#[derive(Debug)]
pub struct Download {
    path: PathBuf,
}

impl Download {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn open(&self) -> Result<File> {
        File::open(&self.path).with_context(|| format!("cannot open {}", self.path.display()))
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn stream(mut body: impl Read, file: &mut File, expected: u64) -> Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK];
    let mut total: u64 = 0;
    loop {
        let read = body.read(&mut buffer).context("download was interrupted")?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > expected {
            return Err(FetchError::TooLarge { expected }.into());
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])?;
    }
    file.sync_all()?;
    Ok((total, hex(&hasher.finalize())))
}

pub fn download(
    layout: &Layout,
    transport: &dyn Transport,
    artifact: &TargetArtifact,
) -> Result<Download> {
    if !is_allowed_package_url(&artifact.url) {
        return Err(FetchError::UrlNotAllowed(artifact.url.clone()).into());
    }
    if !is_sha256(&artifact.sha256) {
        return Err(FetchError::BadDigest(artifact.sha256.clone()).into());
    }
    let Response::Body { body, .. } = transport.get(&artifact.url, None)? else {
        return Err(FetchError::UnexpectedNotModified(artifact.url.clone()).into());
    };
    let directory = layout.downloads_dir();
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let download = Download {
        path: layout.download_part(&artifact.sha256),
    };
    let mut file = File::create(&download.path)
        .with_context(|| format!("cannot create {}", download.path.display()))?;
    let (size, sha256) = stream(body, &mut file, artifact.size)?;
    if size != artifact.size {
        return Err(FetchError::SizeMismatch {
            expected: artifact.size,
            actual: size,
        }
        .into());
    }
    if sha256 != artifact.sha256 {
        return Err(FetchError::HashMismatch {
            expected: artifact.sha256.clone(),
            actual: sha256,
        }
        .into());
    }
    Ok(download)
}

pub fn read_limited(body: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    body.take(limit + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= limit,
        "response is larger than {limit} bytes"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    use super::{is_allowed_package_url, read_limited, Client, HttpTransport, Response, Transport};

    fn serve(listener: &TcpListener, answer: &str) -> Vec<String> {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut lines = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line.trim().is_empty() {
                break;
            }
            lines.push(line.trim().to_ascii_lowercase());
        }
        stream.write_all(answer.as_bytes()).unwrap();
        lines
    }

    #[test]
    fn the_http_transport_sends_and_honours_etags() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/catalog", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let first = serve(
                &listener,
                "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
            );
            let second = serve(
                &listener,
                "HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n",
            );
            (first, second)
        });
        let transport = HttpTransport::build(Client::builder().no_proxy()).unwrap();
        let Response::Body { etag, body } = transport.get(&url, None).unwrap() else {
            panic!("expected a body");
        };
        assert_eq!(etag.as_deref(), Some("\"v1\""));
        assert_eq!(read_limited(body, 5).unwrap(), b"hello");
        let again = transport.get(&url, Some("\"v1\"")).unwrap();
        assert!(matches!(again, Response::NotModified));
        let (first, second) = server.join().unwrap();
        assert!(!first.iter().any(|line| line.starts_with("if-none-match")));
        assert!(second.contains(&"if-none-match: \"v1\"".to_string()));
    }

    #[test]
    fn the_https_transport_refuses_plain_http() {
        let transport = HttpTransport::new().unwrap();
        assert!(transport.get("http://127.0.0.1:9/x", None).is_err());
    }

    #[test]
    fn package_urls_must_stay_under_the_release_prefix() {
        let base = "https://github.com/namannn04/veronica/releases/download/ext/";
        assert!(is_allowed_package_url(&format!(
            "{base}media/v1.0.7/a.tar.zst"
        )));
        for url in [
            "http://github.com/namannn04/veronica/releases/download/ext/media/a.tar.zst",
            "https://github.com/namannn04/veronica/releases/download/v1.0/a.tar.zst",
            "https://github.com/other/veronica/releases/download/ext/a.tar.zst",
            "https://github.com.example.org/namannn04/veronica/releases/download/ext/a",
            &format!("{base}../../../../../other/repo/a.tar.zst"),
            &format!("{base}media/a.tar.zst?x=1"),
            &format!("{base}media/a.tar.zst#x"),
            "not a url",
        ] {
            assert!(!is_allowed_package_url(url), "{url}");
        }
        let mut body: &[u8] = b"123456";
        assert!(read_limited(&mut body, 5).is_err());
        let mut exact: &[u8] = b"12345";
        assert_eq!(read_limited(&mut exact, 5).unwrap().len(), 5);
        assert!(read_limited(std::io::empty(), 0).unwrap().is_empty());
    }
}
