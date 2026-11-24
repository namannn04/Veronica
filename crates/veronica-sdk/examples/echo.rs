use std::time::Duration;

use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use veronica_sdk::cli::{parse, Output};
use veronica_sdk::protocol::rpc::{
    error_code, CliRunParams, CliRunResult, ErrorObject, RpcCallParams,
};
use veronica_sdk::protocol::ExitCode;
use veronica_sdk::{Capture, Context, Extension, Failure};

#[derive(Parser)]
struct Program {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    root: Root,
}

#[derive(Subcommand)]
enum Root {
    Echo {
        #[command(subcommand)]
        action: Action,
    },
}

#[derive(Subcommand)]
enum Action {
    Say {
        text: String,
        #[arg(long)]
        upper: bool,
    },
    Fail {
        #[arg(long)]
        code: i32,
    },
    Erase {
        item: String,
    },
}

fn run_cli(output: Output<'_>, root: Root) -> anyhow::Result<()> {
    let Root::Echo { action } = root;
    match action {
        Action::Say { text, upper } => {
            let text = if upper { text.to_uppercase() } else { text };
            output.emit(&json!({ "text": text }), || text.clone())
        }
        Action::Fail { code } => {
            let code = ExitCode::from_code(code).unwrap_or(ExitCode::Failed);
            Err(Failure::new(code, format!("failed with {}", code.code())).into())
        }
        Action::Erase { item } => {
            output.emit(&json!({ "erased": item }), || format!("erased {item}"))
        }
    }
}

struct Echo;

impl Extension for Echo {
    fn methods(&self) -> Vec<String> {
        ["echo", "ask-host", "dirs", "sleep"]
            .map(String::from)
            .to_vec()
    }

    async fn cli_run(
        &self,
        _context: Context,
        params: CliRunParams,
    ) -> Result<CliRunResult, ErrorObject> {
        let program = match parse::<Program>(&params) {
            Ok(program) => program,
            Err(result) => return Ok(result),
        };
        let capture = Capture::new(params.json);
        let outcome = run_cli(capture.output(), program.root);
        Ok(capture.finish(outcome))
    }

    async fn rpc_call(
        &self,
        context: Context,
        params: RpcCallParams,
    ) -> Result<Value, ErrorObject> {
        match params.method.as_str() {
            "echo" => Ok(params.params),
            "ask-host" => {
                let key = params.params["key"].as_str().ok_or_else(|| {
                    ErrorObject::new(error_code::INVALID_PARAMS, "key must be a string")
                })?;
                let value = context.host().settings_get(key).await?;
                context.host().emit("echo.asked", json!({ "key": key }))?;
                Ok(json!({ "value": value }))
            }
            "dirs" => Ok(json!({
                "data": context.dirs().data(),
                "mode": context.mode(),
            })),
            "sleep" => {
                tokio::time::sleep(Duration::from_mins(1)).await;
                Ok(Value::Null)
            }
            other => Err(ErrorObject::method_not_found(other)),
        }
    }
}

fn main() -> std::process::ExitCode {
    veronica_sdk::run(Echo)
}
