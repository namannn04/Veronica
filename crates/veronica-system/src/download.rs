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
    download_with(url, destination, curl_command)
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
            "60",
            "--retry",
            "4",
            "--retry-all-errors",
            "--retry-delay",
            "1",
            "--retry-max-time",
            "120",
            "--output",
        ])
        .arg(partial)
        .arg(url);
    command
}

fn download_with(
    url: &str,
    destination: &Path,
    command: impl FnOnce(&str, &Path) -> Command,
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
        let output = command(url, &partial).output().context("cannot run curl")?;
        if !output.status.success() {
            bail!("Download failed after bounded retries: {}. Try again or download from the release page.",
                String::from_utf8_lossy(&output.stderr).trim());
        }
        std::fs::rename(&partial, destination).context("cannot finish the release download")?;
        Ok(destination.to_path_buf())
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
            "1",
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
    fn interrupted_transfer_is_restarted_without_duplicating_partial_bytes() {
        let (url, server) = serve(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nbad",
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\ncomplete",
        ]);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("package.deb");
        download_with(&url, &path, local_command).unwrap();
        server.join().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"complete");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[test]
    fn exhausted_retries_leave_no_installable_partial_package() {
        let (url, server) = serve(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nbad",
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nbad",
        ]);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("package.deb");
        assert!(download_with(&url, &path, local_command).is_err());
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
        download_with(&url, &path, local_command).unwrap();
        server.join().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"ok");
    }
    #[test]
    fn insecure_release_urls_are_refused() {
        let root = tempfile::tempdir().unwrap();
        assert!(download_https("http://example.invalid/file", &root.path().join("file")).is_err());
    }
}
