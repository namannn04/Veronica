//! Bounded, certificate-verified release downloads. Partial bodies never become packages.
use anyhow::{bail, Context, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

pub fn download_https(url: &str, destination: &Path) -> Result<PathBuf> {
    if !url.starts_with("https://") {
        bail!("Release downloads require HTTPS");
    }
    download_with(
        url,
        destination,
        curl_command,
        5,
        std::time::Duration::from_secs(1),
    )
}

fn curl_command(url: &str, partial: &Path) -> Command {
    let mut command = Command::new("curl");
    command
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "15",
            "--max-time",
            "300",
            "--continue-at",
            "-",
            "--write-out",
            "%{http_code}",
            "--output",
        ])
        .arg(partial)
        .arg(url);
    command
}

fn download_with(
    url: &str,
    destination: &Path,
    command: impl Fn(&str, &Path) -> Command,
    attempts: usize,
    delay: std::time::Duration,
) -> Result<PathBuf> {
    let name = destination
        .file_name()
        .context("Download has no filename")?
        .to_string_lossy();
    let partial = destination.with_file_name(format!(
        ".{name}.{}-{}.part",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        // Create exclusively before handing the private path to curl.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)?;
        let started = std::time::Instant::now();
        let budget = std::time::Duration::from_secs(900);
        let mut detail = String::new();
        for attempt in 0..attempts {
            let remaining = budget.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            let output = command(url, &partial)
                .args(["--max-time", &remaining.as_secs().clamp(1, 300).to_string()])
                .output()
                .context("cannot run curl")?;
            if output.status.success() {
                std::fs::rename(&partial, destination)
                    .context("cannot finish the release download")?;
                return Ok(destination.to_path_buf());
            }
            detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            // Keep received bytes and ask for the remainder on the next call.
            // If a server cannot resume, restart safely instead of appending
            // a complete response to the partial body.
            if matches!(output.status.code(), Some(33 | 36)) || output.stdout == b"416" {
                std::fs::write(&partial, [])?;
            }
            if attempt + 1 < attempts {
                std::thread::sleep(delay.min(budget.saturating_sub(started.elapsed())));
            }
        }
        bail!("Download failed after bounded retries: {detail}. Try again or download from the release page.")
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(partial);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    fn serve(responses: Vec<&'static [u8]>) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/package", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for response in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut request = [0u8; 2048];
                let _ = socket.read(&mut request);
                socket.write_all(response).unwrap();
            }
        });
        (url, server)
    }
    fn local_command(url: &str, partial: &Path) -> Command {
        let mut command = curl_command(url, partial);
        // The test fixture is local HTTP; production permits HTTPS only.
        command.args([
            "--proto",
            "=http",
            "--proto-redir",
            "=http",
            "--retry",
            "0",
            "--retry-delay",
            "1",
            "--retry-max-time",
            "5",
            "--max-time",
            "3",
        ]);
        command
    }
    #[test]
    fn interrupted_transfer_is_resumed_without_duplicating_partial_bytes() {
        let (url, server) = serve(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ncom",
            b"HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\nContent-Range: bytes 3-7/8\r\nConnection: close\r\n\r\nplete",
        ]);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("package.deb");
        download_with(&url, &path, local_command, 2, std::time::Duration::ZERO).unwrap();
        server.join().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"complete");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[test]
    fn a_server_ignoring_range_requests_falls_back_to_a_clean_download() {
        let (url, server) = serve(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ncom",
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ncomplete",
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ncomplete",
        ]);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("package.deb");
        download_with(&url, &path, local_command, 3, std::time::Duration::ZERO).unwrap();
        server.join().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"complete");
    }

    #[test]
    fn exhausted_retries_leave_no_installable_partial_package() {
        let (url, server) = serve(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nbad",
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nbad",
        ]);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("package.deb");
        assert!(download_with(&url, &path, local_command, 2, std::time::Duration::ZERO).is_err());
        server.join().unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
    #[test]
    fn server_failure_is_retried_before_publishing_the_file() {
        let (url, server) = serve(vec![
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        ]);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("SHA256SUMS");
        download_with(&url, &path, local_command, 2, std::time::Duration::ZERO).unwrap();
        server.join().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"ok");
    }
    #[test]
    fn terminal_installer_resumes_an_interrupted_transfer() {
        let (url, server) = serve(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ncom",
            b"HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\nContent-Range: bytes 3-7/8\r\nConnection: close\r\n\r\nplete",
        ]);
        let source = include_str!("../../../install.sh");
        let helpers = source
            .split("fetch() {")
            .nth(1)
            .unwrap()
            .split("\nversion=")
            .next()
            .unwrap();
        let helpers = format!("fetch() {{{helpers}").replace(
            "--proto '=https' --proto-redir '=https'",
            "--proto '=http' --proto-redir '=http'",
        );
        let script = format!("set -euo pipefail\n{helpers}\ndownload_file \"$1\" \"$2\"");
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("package.deb");
        let output = Command::new("bash")
            .args(["-c", &script, "download-test"])
            .arg(&path)
            .arg(url)
            .output()
            .unwrap();
        server.join().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read(path).unwrap(), b"complete");
    }

    #[test]
    fn insecure_release_urls_are_refused() {
        let root = tempfile::tempdir().unwrap();
        assert!(download_https("http://example.invalid/file", &root.path().join("file")).is_err());
    }
}
