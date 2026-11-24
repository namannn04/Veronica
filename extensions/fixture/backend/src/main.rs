use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use rustix::process::{getpid, kill_process, Signal};
use serde_json::{json, Value};
use veronica_sdk::cli::parse;
use veronica_sdk::protocol::rpc::{
    error_code, CliRunParams, CliRunResult, ErrorObject, LogLevel, NotifyParams, RpcCallParams,
};
use veronica_sdk::protocol::ExitCode;
use veronica_sdk::{Capture, Context, Extension, Failure, Output};

const CANCELLED_MARKER: &str = "cancelled";

#[derive(Parser)]
struct Program {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    root: Root,
}

#[derive(Subcommand)]
enum Root {
    Fixture {
        #[command(subcommand)]
        action: Action,
    },
}

#[derive(Subcommand)]
enum Action {
    Echo {
        text: String,
    },
    Fail {
        #[arg(long)]
        code: i32,
    },
    Wipe {
        item: String,
    },
    Wait {
        #[arg(long)]
        seconds: u64,
    },
}

async fn run_cli(
    output: Output<'_>,
    context: &Context,
    confirm: bool,
    root: Root,
) -> anyhow::Result<()> {
    let Root::Fixture { action } = root;
    match action {
        Action::Echo { text } => output.emit(
            &json!({ "text": text, "mode": context.mode(), "pid": std::process::id() }),
            || text.clone(),
        ),
        Action::Fail { code } => {
            let code = ExitCode::from_code(code).unwrap_or(ExitCode::Failed);
            Err(Failure::new(code, format!("failed with {}", code.code())).into())
        }
        Action::Wipe { item } if !confirm => {
            output.emit(&json!({ "wouldRemove": item }), || {
                format!("would remove {item}")
            })?;
            Err(Failure::new(ExitCode::ConfirmationRequired, "wiping needs confirmation").into())
        }
        Action::Wipe { item } => {
            output.emit(&json!({ "removed": item }), || format!("removed {item}"))
        }
        Action::Wait { seconds } => {
            tokio::time::sleep(Duration::from_secs(seconds)).await;
            output.emit(&json!({ "waited": seconds }), || {
                format!("waited {seconds}s")
            })
        }
    }
}

struct CancelledMarker {
    path: PathBuf,
    finished: bool,
}

impl Drop for CancelledMarker {
    fn drop(&mut self) {
        if !self.finished {
            let _ = std::fs::write(&self.path, b"1");
        }
    }
}

fn param<'a>(params: &'a Value, key: &str) -> Result<&'a Value, ErrorObject> {
    params
        .get(key)
        .ok_or_else(|| ErrorObject::new(error_code::INVALID_PARAMS, format!("{key} is required")))
}

fn text(params: &Value, key: &str) -> Result<String, ErrorObject> {
    param(params, key)?
        .as_str()
        .map(String::from)
        .ok_or_else(|| ErrorObject::new(error_code::INVALID_PARAMS, format!("{key} is text")))
}

fn number(params: &Value, key: &str) -> Result<u64, ErrorObject> {
    param(params, key)?
        .as_u64()
        .ok_or_else(|| ErrorObject::new(error_code::INVALID_PARAMS, format!("{key} is a number")))
}

async fn ask_host(context: &Context, params: &Value) -> Result<Value, ErrorObject> {
    let host = context.host();
    match text(params, "method")?.as_str() {
        "settings.get" => Ok(json!(host.settings_get(&text(params, "key")?).await?)),
        "settings.set" => {
            let value = param(params, "value")?.clone();
            host.settings_set(&text(params, "key")?, value).await?;
            Ok(Value::Null)
        }
        "notify" => {
            let title = text(params, "title")?;
            host.notify(NotifyParams {
                title,
                body: None,
                replace_key: None,
            })
            .await?;
            Ok(Value::Null)
        }
        "log" => {
            host.log(LogLevel::Info, &text(params, "message")?).await?;
            Ok(Value::Null)
        }
        "open" => {
            host.open(&text(params, "target")?).await?;
            Ok(Value::Null)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}

fn freeze_soon() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(200));
        let _ = kill_process(getpid(), Signal::STOP);
    });
}

struct Fixture;

impl Extension for Fixture {
    fn methods(&self) -> Vec<String> {
        ["mode", "exit", "crash", "freeze", "sleep", "host"]
            .map(String::from)
            .to_vec()
    }

    async fn cli_run(
        &self,
        context: Context,
        params: CliRunParams,
    ) -> Result<CliRunResult, ErrorObject> {
        let program = match parse::<Program>(&params) {
            Ok(program) => program,
            Err(result) => return Ok(result),
        };
        let capture = Capture::new(params.json);
        let outcome = run_cli(capture.output(), &context, params.confirm, program.root).await;
        Ok(capture.finish(outcome))
    }

    async fn rpc_call(
        &self,
        context: Context,
        params: RpcCallParams,
    ) -> Result<Value, ErrorObject> {
        let arguments = &params.params;
        match params.method.as_str() {
            "mode" => Ok(json!({ "mode": context.mode(), "pid": std::process::id() })),
            "exit" => {
                let code = i32::try_from(number(arguments, "code")?).unwrap_or(1);
                std::process::exit(code)
            }
            "crash" => std::process::abort(),
            "freeze" => {
                freeze_soon();
                Ok(Value::Null)
            }
            "sleep" => {
                let mut marker = CancelledMarker {
                    path: context.dirs().data().join(CANCELLED_MARKER),
                    finished: false,
                };
                tokio::time::sleep(Duration::from_secs(number(arguments, "seconds")?)).await;
                marker.finished = true;
                Ok(Value::Null)
            }
            "host" => ask_host(&context, arguments).await,
            other => Err(ErrorObject::method_not_found(other)),
        }
    }
}

fn main() -> std::process::ExitCode {
    veronica_sdk::run(Fixture)
}
