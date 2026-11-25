mod cli;
mod rpc;
mod shared;

use clap::{Parser, Subcommand};
use serde_json::Value;
use veronica_core::AttentionRepository;
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
    #[command(
        subcommand,
        about = "Attention focus sessions and local attention history"
    )]
    Attention(cli::AttentionCommand),
}

struct Attention;

fn share_tracking_settings() -> anyhow::Result<()> {
    let directories = AppDirectories::current()?;
    let settings = AttentionRepository::new(directories.attention_dir()).load_settings()?;
    shared::mirror_once(&directories.settings_file(), &settings)
}

impl Extension for Attention {
    fn methods(&self) -> Vec<String> {
        rpc::METHODS.map(String::from).to_vec()
    }

    async fn initialize(&self, _context: &Context) -> Result<(), ErrorObject> {
        if let Err(error) = share_tracking_settings() {
            eprintln!("cannot share the attention tracking settings: {error:#}");
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
        let Root::Attention(command) = &program.root;
        let outcome = cli::run(command, capture.output());
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
    veronica_sdk::run(Attention)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use veronica_sdk::protocol::manifest::Effect;

    use super::Program;

    const EFFECTS: [(&str, Effect); 8] = [
        ("attention configure", Effect::Write),
        ("attention focus start", Effect::Write),
        ("attention focus status", Effect::Read),
        ("attention focus stop", Effect::Write),
        ("attention history", Effect::Read),
        ("attention record", Effect::Write),
        ("attention status", Effect::Read),
        ("attention summary", Effect::Read),
    ];

    #[test]
    fn the_manifest_lists_every_command() {
        let commands = veronica_sdk::cli::describe(Program::command(), &EFFECTS).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.json");
        veronica_sdk::cli::sync_manifest(&manifest, commands).unwrap();
    }
}
