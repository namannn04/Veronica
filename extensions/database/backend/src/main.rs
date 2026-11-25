mod cli;
mod rpc;

use clap::{Parser, Subcommand};
use serde_json::Value;
use veronica_core::AppDirectories;
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
        alias = "db",
        about = "Explore databases and run guarded changes"
    )]
    Database(cli::DatabaseCommand),
}

struct Database;

impl Extension for Database {
    fn methods(&self) -> Vec<String> {
        rpc::METHODS.map(String::from).to_vec()
    }

    async fn cli_run(
        &self,
        context: Context,
        params: CliRunParams,
    ) -> Result<CliRunResult, ErrorObject> {
        let program = match veronica_sdk::cli::parse::<Program>(&params) {
            Ok(program) => program,
            Err(result) => return Ok(result),
        };
        if context.mode() == RunMode::Oneshot {
            let _ = std::env::set_current_dir(&params.cwd);
        }
        let capture = Capture::new(params.json);
        let Root::Database(command) = &program.root;
        let outcome = match AppDirectories::current() {
            Ok(directories) => {
                cli::run(
                    &directories,
                    command,
                    params.stdin.as_deref(),
                    capture.output(),
                )
                .await
            }
            Err(error) => Err(error),
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
    veronica_sdk::run(Database)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 17] = [
        ("database connections list", Effect::Read),
        ("database connections get", Effect::Read),
        ("database connections add", Effect::Write),
        ("database connections policy", Effect::Write),
        ("database connections remove", Effect::Destructive),
        ("database capabilities", Effect::Read),
        ("database test", Effect::Read),
        ("database browse", Effect::Read),
        ("database read", Effect::Read),
        ("database query", Effect::Read),
        ("database saved-queries list", Effect::Read),
        ("database saved-queries save", Effect::Write),
        ("database saved-queries get", Effect::Read),
        ("database saved-queries remove", Effect::Destructive),
        ("database mutations preview", Effect::Write),
        ("database mutations apply", Effect::Destructive),
        ("database operations", Effect::Read),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }
}
