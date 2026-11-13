//! Read-only local project checks, also used by the optional Graftcode module.
use crate::format::Output;
use anyhow::Result;
use clap::Subcommand;
use veronica_system::workspace::{self, DoctorRequest};
#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// Discover or explicitly call a local Graftcode Gateway.
    #[command(subcommand)]
    Graft(GraftCommand),
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
        WorkspaceCommand::Graft(command) => return graft(command, output).await,
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

#[derive(Subcommand)]
pub enum GraftCommand {
    Discover {
        #[arg(long)]
        endpoint: String,
    },
    Call {
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "{}")]
        arguments: String,
        #[arg(long)]
        confirm: bool,
    },
}
async fn graft(command: &GraftCommand, output: Output) -> Result<()> {
    use veronica_system::graftcode::{self, Connection};
    let endpoint = match command {
        GraftCommand::Discover { endpoint } | GraftCommand::Call { endpoint, .. } => endpoint,
    };
    let connection = Connection {
        endpoint: endpoint.clone(),
        token: std::env::var("VERONICA_GRAFT_TOKEN").unwrap_or_default(),
    };
    let result = match command {
        GraftCommand::Discover { .. } => {
            serde_json::to_value(graftcode::discover(connection).await?)?
        }
        GraftCommand::Call {
            name,
            arguments,
            confirm,
            ..
        } => {
            graftcode::call(
                connection,
                name.clone(),
                serde_json::from_str(arguments)?,
                *confirm,
            )
            .await?
        }
    };
    output.emit(&result, || {
        serde_json::to_string_pretty(&result).unwrap_or_default()
    })
}
