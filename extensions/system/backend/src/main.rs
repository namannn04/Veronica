mod rpc;
mod sampler;
mod service;
mod services_cli;
mod system_cli;

use clap::{Parser, Subcommand};
use serde_json::Value;
use veronica_sdk::protocol::rpc::{
    CliRunParams, CliRunResult, ErrorObject, RpcCallParams, RunMode,
};
use veronica_sdk::{Capture, Context, Extension};

#[derive(Parser)]
#[command(name = "vr", disable_help_subcommand = true)]
pub(crate) struct Program {
    #[arg(
        long,
        global = true,
        help = "Emit JSON on stdout instead of human-readable text"
    )]
    json: bool,
    #[command(subcommand)]
    root: Root,
}

#[derive(Subcommand)]
enum Root {
    #[command(
        subcommand,
        about = "Inspect local TCP/UDP ports and gracefully stop owned processes"
    )]
    Ports(services_cli::PortsCommand),
    #[command(
        subcommand,
        about = "Local Docker status, containers and published ports"
    )]
    Docker(services_cli::DockerCommand),
    #[command(subcommand, about = "CPU, memory, storage and battery information")]
    System(system_cli::SystemCommand),
}

struct System;

impl Extension for System {
    fn methods(&self) -> Vec<String> {
        rpc::METHODS.map(String::from).to_vec()
    }

    async fn initialize(&self, context: &Context) -> Result<(), ErrorObject> {
        if context.mode() == RunMode::Service {
            tokio::spawn(service::publish(context.clone()));
        }
        Ok(())
    }

    async fn cli_run(
        &self,
        _context: Context,
        params: CliRunParams,
    ) -> Result<CliRunResult, ErrorObject> {
        let program = match veronica_sdk::cli::parse::<Program>(&params) {
            Ok(program) => program,
            Err(result) => return Ok(result),
        };
        let capture = Capture::new(params.json);
        let output = capture.output();
        let outcome = match &program.root {
            Root::Ports(command) => services_cli::ports(command, output).await,
            Root::Docker(command) => services_cli::docker(command, output).await,
            Root::System(command) => system_cli::run(command, output).await,
        };
        Ok(capture.finish(outcome))
    }

    async fn rpc_call(
        &self,
        _context: Context,
        params: RpcCallParams,
    ) -> Result<Value, ErrorObject> {
        rpc::call(&params.method, params.params).await
    }
}

fn main() -> std::process::ExitCode {
    veronica_sdk::run(System)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 13] = [
        ("ports list", Effect::Read),
        ("ports stop", Effect::Destructive),
        ("docker status", Effect::Read),
        ("docker action", Effect::Destructive),
        ("system snapshot", Effect::Read),
        ("system stats", Effect::Read),
        ("system disks", Effect::Read),
        ("system processes", Effect::Read),
        ("system audio list", Effect::Read),
        ("system audio volume", Effect::Write),
        ("system audio mute", Effect::Write),
        ("system audio unmute", Effect::Write),
        ("system bluetooth", Effect::Read),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }
}
