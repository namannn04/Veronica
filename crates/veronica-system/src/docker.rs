//! Local Docker only. Pin every operation to the inspected Unix endpoint.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Stdio, time::Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerSnapshot {
    pub status: String,
    pub detail: String,
    pub endpoint: Option<String>,
    pub version: Option<String>,
    pub containers: Vec<DockerContainer>,
}
impl DockerSnapshot {
    fn unavailable(status: &str, detail: impl Into<String>, endpoint: Option<String>) -> Self {
        Self {
            status: status.into(),
            detail: detail.into(),
            endpoint,
            version: None,
            containers: Vec::new(),
        }
    }
}
async fn run(binary: &std::path::Path, args: &[&str]) -> Result<String> {
    let mut command = tokio::process::Command::from(veronica_core::host::command(binary));
    command.args(args).stdin(Stdio::null()).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .context("Docker did not respond within 15 seconds")??;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned())
}
fn binary() -> Option<PathBuf> {
    veronica_core::host::which("docker")
}
async fn endpoint(binary: &std::path::Path) -> Result<String> {
    // Read the host environment as well as the selected context. DOCKER_CONTEXT
    // overrides DOCKER_HOST; Flatpak's own environment is not the host's.
    let env = run(
        std::path::Path::new("sh"),
        &[
            "-c",
            "printf '%s\\n%s\\n' \"${DOCKER_CONTEXT-}\" \"${DOCKER_HOST-}\"",
        ],
    )
    .await?;
    let mut values = env.split('\n');
    let context = values.next().unwrap_or_default();
    let host = values.next().unwrap_or_default();
    if context.is_empty() && !host.is_empty() {
        return Ok(host.into());
    }
    let mut args = vec![
        "context",
        "inspect",
        "--format",
        "{{json .Endpoints.docker.Host}}",
    ];
    if !context.is_empty() {
        args.push(context);
    }
    let raw = run(binary, &args).await?;
    serde_json::from_str(&raw).context("Cannot read the Docker context endpoint")
}
fn local_endpoint(endpoint: &str) -> bool {
    endpoint.starts_with("unix:///") && !endpoint.contains(['\n', '\r', '\0'])
}
fn parse_containers(text: &str) -> Result<Vec<DockerContainer>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Row {
        #[serde(rename = "ID")]
        id: String,
        names: String,
        image: String,
        state: String,
        status: String,
        ports: String,
    }
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let row: Row = serde_json::from_str(line)
                .context("Docker returned an unreadable container row")?;
            Ok(DockerContainer {
                id: row.id,
                name: row.names,
                image: row.image,
                state: row.state,
                status: row.status,
                ports: row.ports,
            })
        })
        .collect()
}
async fn snapshot_with(binary: &std::path::Path) -> Result<DockerSnapshot> {
    let endpoint = endpoint(binary).await?;
    if !local_endpoint(&endpoint) {
        return Ok(DockerSnapshot::unavailable("remote", "This page supports local Unix-socket Docker endpoints. Network and SSH endpoints are not managed here; use Machines for remote hosts.", Some(endpoint)));
    }
    let version = run(
        binary,
        &[
            "--host",
            &endpoint,
            "info",
            "--format",
            "{{json .ServerVersion}}",
        ],
    )
    .await?;
    let version: String =
        serde_json::from_str(&version).context("Cannot read Docker server version")?;
    let rows = run(
        binary,
        &[
            "--host",
            &endpoint,
            "ps",
            "--all",
            "--no-trunc",
            "--format",
            "{{json .}}",
        ],
    )
    .await?;
    Ok(DockerSnapshot {
        status: "ready".into(),
        detail: "Docker is available on this computer".into(),
        endpoint: Some(endpoint),
        version: Some(version),
        containers: parse_containers(&rows)?,
    })
}
pub async fn snapshot() -> DockerSnapshot {
    let Some(binary) = binary() else {
        return DockerSnapshot::unavailable(
            "missing",
            "Docker CLI is not installed on this computer",
            None,
        );
    };
    match snapshot_with(&binary).await {
        Ok(snapshot) => snapshot,
        Err(error) => DockerSnapshot::unavailable("unavailable", format!("Docker cannot be reached: {error:#}. Check that the daemon is running and your account has access."), None),
    }
}
pub async fn action(expected_endpoint: &str, id: &str, action: &str) -> Result<()> {
    ensure!(
        local_endpoint(expected_endpoint),
        "Only a local Docker endpoint can be managed here"
    );
    ensure!(
        id.len() == 64 && id.bytes().all(|ch| ch.is_ascii_hexdigit()),
        "Invalid container identity; refresh first"
    );
    ensure!(
        matches!(action, "start" | "stop" | "restart"),
        "Unsupported Docker operation"
    );
    let binary = binary().context("Docker CLI is not installed")?;
    ensure!(
        endpoint(&binary).await? == expected_endpoint,
        "The Docker endpoint changed. Refresh before trying again"
    );
    let timeout = if action == "start" {
        vec!["--host", expected_endpoint, action, id]
    } else {
        vec!["--host", expected_endpoint, action, "--time", "8", id]
    };
    run(&binary, &timeout).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn published_and_unpublished_ports_are_kept_separate() {
        let rows = parse_containers(r#"{"ID":"abc","Names":"web","Image":"nginx","State":"running","Status":"Up 2 hours","Ports":"0.0.0.0:3000->80/tcp, [::]:3000->80/tcp, 443/tcp"}"#).unwrap();
        assert_eq!(rows[0].name, "web");
        assert!(rows[0].ports.contains("3000->80/tcp"));
        assert!(parse_containers("not json").is_err());
        assert!(parse_containers("").unwrap().is_empty());
    }
    #[test]
    fn remote_endpoints_are_not_local() {
        assert!(local_endpoint("unix:///var/run/docker.sock"));
        for endpoint in [
            "tcp://remote:2375",
            "ssh://server",
            "unix://relative",
            "unix:///tmp/a\n",
        ] {
            assert!(!local_endpoint(endpoint));
        }
    }
    #[tokio::test]
    async fn refuses_invalid_actions_before_running_docker() {
        assert!(action("tcp://remote:2375", &"a".repeat(64), "stop")
            .await
            .is_err());
        assert!(action("unix:///var/run/docker.sock", "--help", "stop")
            .await
            .is_err());
        assert!(action("unix:///var/run/docker.sock", &"a".repeat(64), "rm")
            .await
            .is_err());
    }
}
