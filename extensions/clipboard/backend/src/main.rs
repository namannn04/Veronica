mod cli;
mod rpc;

use clap::{Parser, Subcommand};
use serde_json::Value;
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::protocol::rpc::{CliRunParams, CliRunResult, ErrorObject, RpcCallParams};
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
    #[command(subcommand, alias = "clip", about = "The clipboard history")]
    Clipboard(cli::ClipboardCommand),
}

struct Clipboard;

impl Extension for Clipboard {
    fn methods(&self) -> Vec<String> {
        rpc::METHODS.map(String::from).to_vec()
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
        let Root::Clipboard(command) = &program.root;
        let outcome = AppDirectories::current().and_then(|directories| {
            cli::run(
                &directories,
                command,
                capture.output(),
                params.stdin.as_deref(),
            )
        });
        Ok(capture.finish(outcome))
    }

    async fn rpc_call(
        &self,
        _context: Context,
        params: RpcCallParams,
    ) -> Result<Value, ErrorObject> {
        rpc::call(&params.method, params.params)
    }
}

fn main() -> std::process::ExitCode {
    veronica_sdk::run(Clipboard)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 5] = [
        ("clipboard record", Effect::Write),
        ("clipboard list", Effect::Read),
        ("clipboard get", Effect::Read),
        ("clipboard remove", Effect::Destructive),
        ("clipboard clear", Effect::Destructive),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }
}
