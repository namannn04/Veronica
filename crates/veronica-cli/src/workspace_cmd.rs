//! Read-only local project checks, also used by the optional Graftcode module.
use crate::format::Output;
use anyhow::Result;
use clap::Subcommand;
use veronica_system::workspace::{self, DoctorRequest};
#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// Inspect a project without executing its code or reading .env files.
    Doctor {
        #[arg(long)]
        root: String,
        /// Expected IPv4 loopback TCP listeners (not application health checks).
        #[arg(long, value_delimiter = ',')]
        ports: Vec<u16>,
        /// Exact local Docker container names expected to be running.
        #[arg(long, value_delimiter = ',')]
        containers: Vec<String>,
    },
}
pub async fn run(command: &WorkspaceCommand, output: Output) -> Result<()> {
    match command {
        WorkspaceCommand::Doctor {
            root,
            ports,
            containers,
        } => {
            let report = workspace::doctor(DoctorRequest {
                root: root.clone(),
                ports: ports.clone(),
                containers: containers.clone(),
            })
            .await?;
            output.emit(&report, || {
                report
                    .checks
                    .iter()
                    .map(|c| format!("{}: {} — {}", c.status, c.name, c.detail))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
    }
}
