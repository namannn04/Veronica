//! On-demand, read-only workspace checks. Never executes project code or reads .env.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::OpenOptions,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_FILE: u64 = 256 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DoctorRequest {
    pub root: String,
    #[serde(default)]
    pub ports: Vec<u16>,
    #[serde(default)]
    pub containers: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub status: String,
    pub detail: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub project: String,
    pub root: String,
    pub checks: Vec<Check>,
}
fn check(name: impl Into<String>, ok: bool, detail: impl Into<String>) -> Check {
    Check {
        name: name.into(),
        status: if ok { "pass" } else { "warning" }.into(),
        detail: detail.into(),
    }
}
fn read_file(path: &Path) -> Result<Vec<u8>> {
    // Refuse symlinks/devices/FIFOs, including a replacement racing metadata checks.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(meta.is_file(), "not a regular file");
    ensure!(meta.len() <= MAX_FILE, "file exceeds 256 KiB");
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_FILE, "file exceeds 256 KiB");
    Ok(bytes)
}
fn inspect(root: &str) -> Result<DoctorReport> {
    ensure!(
        !root.is_empty() && root.len() <= 4096,
        "Choose a project directory"
    );
    let root = PathBuf::from(root)
        .canonicalize()
        .context("Cannot open project directory")?;
    ensure!(root.is_dir(), "Project must be a directory");
    let mut report = DoctorReport {
        project: root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        root: root.to_string_lossy().into_owned(),
        checks: Vec::new(),
    };
    for (name, label) in [
        ("package.json", "Node.js"),
        ("Cargo.toml", "Rust"),
        ("pyproject.toml", "Python"),
        ("requirements.txt", "Python requirements"),
        ("go.mod", "Go"),
        ("pom.xml", "Java"),
        ("composer.json", "PHP"),
        ("Gemfile", "Ruby"),
        ("compose.yaml", "Docker Compose"),
        ("docker-compose.yml", "Docker Compose"),
    ] {
        let path = root.join(name);
        if std::fs::symlink_metadata(&path).is_err() {
            continue;
        }
        match read_file(&path) {
            Ok(bytes) => {
                let detail = if name == "package.json" {
                    match serde_json::from_slice::<serde_json::Value>(&bytes) {
                        Ok(value) if value.is_object() => {
                            let scripts = value
                                .get("scripts")
                                .and_then(|v| v.as_object())
                                .map(|s| {
                                    s.keys()
                                        .take(32)
                                        .map(|s| s.chars().take(80).collect::<String>())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default();
                            format!("Manifest found. Available script names: {}. No scripts were executed.", if scripts.is_empty() { "none" } else { &scripts })
                        }
                        _ => {
                            report.checks.push(check(
                                label,
                                false,
                                "package.json is not a valid JSON object",
                            ));
                            continue;
                        }
                    }
                } else {
                    format!("{name} found; contents were not executed. Dependency installation is not verified.")
                };
                report.checks.push(check(label, true, detail));
            }
            Err(error) => report
                .checks
                .push(check(label, false, format!("{name}: {error}"))),
        }
    }
    if report.checks.is_empty() {
        report.checks.push(check(
            "Project manifests",
            false,
            "No supported manifest found in this directory. Choose the project root.",
        ));
    }
    Ok(report)
}
pub async fn doctor(request: DoctorRequest) -> Result<DoctorReport> {
    ensure!(
        request.ports.len() <= 16 && request.ports.iter().all(|p| *p != 0),
        "Choose at most 16 non-zero ports"
    );
    ensure!(
        request.containers.len() <= 16
            && request.containers.iter().all(|s| !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))),
        "Choose at most 16 exact container names (letters, numbers, _, . and -)"
    );
    let root = request.root;
    let mut report = tokio::task::spawn_blocking(move || inspect(&root)).await??;
    for port in request.ports.into_iter().collect::<BTreeSet<_>>() {
        let open = matches!(
            tokio::time::timeout(
                Duration::from_millis(250),
                tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            )
            .await,
            Ok(Ok(_))
        );
        report.checks.push(check(format!("Port {port}"), open, if open { "TCP connection accepted on 127.0.0.1; application health is not verified." } else { "No TCP connection accepted on 127.0.0.1. Start the expected service or check its bind address." }));
    }
    if !request.containers.is_empty() {
        match tokio::time::timeout(Duration::from_secs(6), crate::docker::snapshot()).await {
            Ok(snapshot) if snapshot.status == "ready" => {
                for name in request.containers.into_iter().collect::<BTreeSet<_>>() {
                    let state = snapshot
                        .containers
                        .iter()
                        .find(|c| c.name.trim_start_matches('/') == name)
                        .map(|c| c.state.as_str());
                    report.checks.push(check(
                        format!("Container {name}"),
                        state == Some("running"),
                        format!(
                            "State: {}. No container was changed.",
                            state.unwrap_or("not found")
                        ),
                    ));
                }
            }
            Ok(snapshot) => report.checks.push(check(
                "Docker checks",
                false,
                snapshot.detail.chars().take(512).collect::<String>(),
            )),
            Err(_) => report.checks.push(check(
                "Docker checks",
                false,
                "Local Docker did not answer within 6 seconds.",
            )),
        }
    }
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn inspects_without_executing_scripts_or_reading_secrets() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("package.json"),
            r#"{"scripts":{"dev":"touch SHOULD_NOT_EXIST"}}"#,
        )
        .unwrap();
        std::fs::write(temp.path().join(".env"), "SECRET=never-export").unwrap();
        let report = doctor(DoctorRequest {
            root: temp.path().display().to_string(),
            ports: vec![],
            containers: vec![],
        })
        .await
        .unwrap();
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(encoded.contains("dev"));
        assert!(!encoded.contains("never-export"));
        assert!(!temp.path().join("SHOULD_NOT_EXIST").exists());
    }
    #[test]
    fn rejects_symlinks_fifos_and_large_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("package.json");
        std::os::unix::fs::symlink("/etc/passwd", &path).unwrap();
        assert!(read_file(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, vec![0; MAX_FILE as usize + 1]).unwrap();
        assert!(read_file(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        unsafe {
            libc::mkfifo(name.as_ptr(), 0o600);
        }
        assert!(read_file(&path).is_err());
    }
    #[tokio::test]
    async fn validates_limits_and_checks_real_listener() {
        let temp = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut request = DoctorRequest {
            root: temp.path().display().to_string(),
            ports: vec![listener.local_addr().unwrap().port()],
            containers: vec![],
        };
        let report = doctor(request.clone()).await.unwrap();
        assert_eq!(report.checks.last().unwrap().status, "pass");
        request.ports = vec![1; 17];
        assert!(doctor(request).await.is_err());
    }
}
