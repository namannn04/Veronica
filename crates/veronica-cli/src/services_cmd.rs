//! Local developer services, shared with the Ports & Docker page.
use crate::format::Output;
use anyhow::{ensure, Result};
use veronica_system::{docker, ports};

#[derive(clap::Subcommand)]
pub enum PortsCommand {
    /// TCP listeners and bound, unconnected UDP sockets, IPv4 and IPv6.
    List {
        #[arg(long)]
        port: Option<u16>,
    },
    /// Send SIGTERM after verifying the process and socket identity.
    Stop {
        #[arg(long)]
        pid: u32,
        #[arg(long)]
        started_at: String,
        #[arg(long)]
        inode: String,
        #[arg(long)]
        confirm: bool,
    },
}
#[derive(clap::Subcommand)]
pub enum DockerCommand {
    /// Local daemon availability, containers and published ports.
    Status,
    /// Start, stop or restart a container from a status reading.
    Action {
        #[arg(value_enum)]
        action: ContainerAction,
        id: String,
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        confirm: bool,
    },
}
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
}
impl ContainerAction {
    fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}
pub async fn ports(command: &PortsCommand, output: Output) -> Result<()> {
    match command {
        PortsCommand::List { port } => {
            let mut report = ports::snapshot().await?;
            report
                .ports
                .retain(|row| port.is_none_or(|port| port == row.port));
            output.emit(&report, || {
                let mut lines: Vec<String> = report
                    .ports
                    .iter()
                    .map(|row| {
                        let owners = row
                            .owners
                            .iter()
                            .map(|owner| {
                                format!(
                                    "{} (PID {}, started {})",
                                    owner.name, owner.pid, owner.started_at
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!(
                            "{} {}:{}  {}  [inode {}]",
                            row.protocol,
                            row.address,
                            row.port,
                            if owners.is_empty() {
                                "owner unavailable"
                            } else {
                                &owners
                            },
                            row.inode
                        )
                    })
                    .collect();
                if lines.is_empty() {
                    lines.push("No matching listening ports".into());
                }
                lines.extend(report.warnings.clone());
                lines.join("\n")
            })
        }
        PortsCommand::Stop {
            pid,
            started_at,
            inode,
            confirm,
        } => {
            ensure!(*confirm, "Pass --confirm to stop this process");
            let message = ports::stop(&ports::StopRequest {
                pid: *pid,
                started_at: started_at.clone(),
                inode: inode.clone(),
            })
            .await?;
            output.emit(&serde_json::json!({"message":message}), || message.clone())
        }
    }
}
pub async fn docker(command: &DockerCommand, output: Output) -> Result<()> {
    match command {
        DockerCommand::Status => {
            let report = docker::snapshot().await;
            output.emit(&report, || {
                let mut lines = vec![report.detail.clone()];
                for container in &report.containers {
                    lines.push(format!(
                        "{}  {}  {}  {}  {}",
                        container.id,
                        container.name,
                        container.state,
                        container.image,
                        container.ports
                    ));
                }
                lines.join("\n")
            })
        }
        DockerCommand::Action {
            action,
            id,
            endpoint,
            confirm,
        } => {
            ensure!(*confirm, "Pass --confirm to change this container");
            docker::action(endpoint, id, action.name()).await?;
            output.emit(&serde_json::json!({"ok":true}), || {
                "Container updated".into()
            })
        }
    }
}
