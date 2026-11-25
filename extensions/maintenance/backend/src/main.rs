mod cleaner_cli;
mod packages_cli;
mod rpc;

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
        about = "The disk cleaner: measure developer caches and build output, and move what you choose to the Trash"
    )]
    Cleaner(cleaner_cli::CleanerCommand),
    #[command(
        subcommand,
        alias = "pkg",
        about = "App Maintenance: what is installed, what can be updated, and removing it with the consequences shown first"
    )]
    Maintenance(packages_cli::MaintenanceCommand),
}

async fn run(context: &Context, params: &CliRunParams, confirmed: bool) -> CliRunResult {
    let program = match veronica_sdk::cli::parse::<Program>(params) {
        Ok(program) => program,
        Err(result) => return result,
    };
    if context.mode() == RunMode::Oneshot {
        let _ = std::env::set_current_dir(&params.cwd);
    }
    let capture = Capture::new(params.json);
    let outcome = match &program.root {
        Root::Cleaner(command) => cleaner_cli::run(command, capture.output(), confirmed),
        Root::Maintenance(command) => packages_cli::run(command, capture.output(), confirmed).await,
    };
    capture.finish(outcome)
}

struct Maintenance;

impl Extension for Maintenance {
    fn methods(&self) -> Vec<String> {
        rpc::METHODS.map(String::from).to_vec()
    }

    fn previews_itself(&self, _argv: &[String]) -> bool {
        true
    }

    async fn cli_run(
        &self,
        context: Context,
        params: CliRunParams,
    ) -> Result<CliRunResult, ErrorObject> {
        Ok(run(&context, &params, params.confirm).await)
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
    veronica_sdk::run(Maintenance)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 9] = [
        ("cleaner scan", Effect::Read),
        ("cleaner categories", Effect::Read),
        ("cleaner clean", Effect::Destructive),
        ("cleaner drives", Effect::Read),
        ("maintenance inventory", Effect::Read),
        ("maintenance updates", Effect::Read),
        ("maintenance update", Effect::Destructive),
        ("maintenance remove", Effect::Destructive),
        ("maintenance sources", Effect::Read),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }
}
