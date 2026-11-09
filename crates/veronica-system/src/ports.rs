//! The host's TCP listeners and unconnected UDP sockets, never sandbox PIDs.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{process::Stdio, time::Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortOwner {
    pub pid: u32,
    pub started_at: String,
    pub uid: u32,
    pub name: String,
    pub can_stop: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListeningPort {
    pub address: String,
    pub port: u16,
    pub protocol: String,
    pub inode: String,
    pub owners: Vec<PortOwner>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PortsSnapshot {
    pub ports: Vec<ListeningPort>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopRequest {
    pub pid: u32,
    pub started_at: String,
    pub inode: String,
}

async fn helper(request: serde_json::Value) -> Result<serde_json::Value> {
    let mut command = tokio::process::Command::from(veronica_core::host::command("python3"));
    command.args(["-c", include_str!("ports_helper.py"), &request.to_string()]);
    command.stdin(Stdio::null()).kill_on_drop(true);
    let result = tokio::time::timeout(Duration::from_secs(12), command.output())
        .await
        .context("Port inspection timed out")?
        .context("Port inspection requires Python 3 on this computer")?;
    if !result.status.success() {
        bail!("{}", String::from_utf8_lossy(&result.stderr).trim());
    }
    serde_json::from_slice(&result.stdout).context("Cannot read the host's port information")
}

pub async fn snapshot() -> Result<PortsSnapshot> {
    serde_json::from_value(helper(serde_json::json!({"action":"list"})).await?).map_err(Into::into)
}
pub async fn stop(request: &StopRequest) -> Result<String> {
    let mut value = serde_json::to_value(request)?;
    value["action"] = serde_json::json!("stop");
    let result = helper(value).await?;
    Ok(result["message"]
        .as_str()
        .unwrap_or("Stop requested")
        .into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_socket_and_stop_integration() {
        let test = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ports_test.py");
        let result = std::process::Command::new("python3")
            .arg(test)
            .status()
            .unwrap();
        assert!(result.success());
    }
    #[tokio::test]
    async fn host_report_deserializes() {
        let report = super::snapshot().await.unwrap();
        for port in report.ports {
            assert!(matches!(port.protocol.as_str(), "TCP" | "UDP"));
        }
    }
}
