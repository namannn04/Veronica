use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use clap::builder::ValueParser;
use clap::{ArgAction, Parser};
use serde::Serialize;
use serde_json::{json, Value};
use veronica_protocol::manifest::{Arg, ArgKind, ArgType, Command, Effect};
use veronica_protocol::rpc::{CliRunParams, CliRunResult};
use veronica_protocol::{ExitCode, Manifest};

const BIN_NAME: &str = "vr";
const JSON_FLAG: &str = "json";
const HELP_SUBCOMMAND: &str = "help";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct Failure {
    pub code: ExitCode,
    pub message: String,
}

impl Failure {
    pub fn new(code: ExitCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Capture {
    json: bool,
    document: Mutex<Option<Value>>,
    text: Mutex<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Output<'a> {
    capture: &'a Capture,
}

fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn error_document(code: ExitCode, message: &str) -> Value {
    json!({ "error": { "code": code.code(), "message": message } })
}

impl Capture {
    pub fn new(json: bool) -> Self {
        Self {
            json,
            ..Self::default()
        }
    }

    pub fn output(&self) -> Output<'_> {
        Output { capture: self }
    }

    pub fn finish(self, outcome: anyhow::Result<()>) -> CliRunResult {
        let mut document = self
            .document
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        let mut text = self
            .text
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        let exit_code = match outcome {
            Ok(()) => ExitCode::Success,
            Err(error) => {
                let code = error
                    .downcast_ref::<Failure>()
                    .map_or(ExitCode::Failed, |failure| failure.code);
                let message = format!("{error:#}");
                if self.json {
                    document.get_or_insert_with(|| error_document(code, &message));
                } else {
                    text.push_str("vr: ");
                    text.push_str(&message);
                    text.push('\n');
                }
                code
            }
        };
        CliRunResult {
            exit_code,
            document,
            text: (!text.is_empty()).then_some(text),
        }
    }
}

impl Output<'_> {
    pub fn is_json(self) -> bool {
        self.capture.json
    }

    pub fn write(self, text: &str) {
        if !self.capture.json {
            guard(&self.capture.text).push_str(text);
        }
    }

    pub fn emit<T: Serialize>(
        self,
        value: &T,
        text: impl FnOnce() -> String,
    ) -> anyhow::Result<()> {
        if self.capture.json {
            let mut document = guard(&self.capture.document);
            anyhow::ensure!(
                document.is_none(),
                "a command emits at most one JSON document"
            );
            *document = Some(serde_json::to_value(value)?);
        } else {
            let rendered = text();
            if !rendered.is_empty() {
                let mut captured = guard(&self.capture.text);
                captured.push_str(&rendered);
                captured.push('\n');
            }
        }
        Ok(())
    }
}

pub fn parse<P: Parser>(params: &CliRunParams) -> Result<P, CliRunResult> {
    let argv = std::iter::once(BIN_NAME).chain(params.argv.iter().map(String::as_str));
    P::try_parse_from(argv).map_err(|error| {
        let rendered = error.render().to_string();
        if !error.use_stderr() {
            return CliRunResult {
                exit_code: ExitCode::Success,
                document: None,
                text: Some(rendered),
            };
        }
        let code = ExitCode::Usage;
        if params.json {
            CliRunResult {
                exit_code: code,
                document: Some(error_document(code, rendered.trim_end())),
                text: None,
            }
        } else {
            CliRunResult {
                exit_code: code,
                document: None,
                text: Some(rendered),
            }
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DescribeError {
    #[error("no effect declared for {}", .0.join(", "))]
    MissingEffects(Vec<String>),
    #[error("effects declared for paths that are not leaf commands: {}", .0.join(", "))]
    UnknownEffects(Vec<String>),
}

pub fn describe(
    mut program: clap::Command,
    effects: &[(&str, Effect)],
) -> Result<Vec<Command>, DescribeError> {
    program.build();
    let mut walker = Walker {
        effects: effects.iter().copied().collect(),
        used: BTreeSet::new(),
        missing: Vec::new(),
    };
    let commands = subcommands(&program)
        .map(|command| walker.command(command, ""))
        .collect();
    if !walker.missing.is_empty() {
        return Err(DescribeError::MissingEffects(walker.missing));
    }
    let unknown: Vec<String> = walker
        .effects
        .keys()
        .filter(|path| !walker.used.contains(*path))
        .map(|path| (*path).to_string())
        .collect();
    if !unknown.is_empty() {
        return Err(DescribeError::UnknownEffects(unknown));
    }
    Ok(commands)
}

struct Walker<'a> {
    effects: BTreeMap<&'a str, Effect>,
    used: BTreeSet<&'a str>,
    missing: Vec<String>,
}

fn subcommands(command: &clap::Command) -> impl Iterator<Item = &clap::Command> {
    command
        .get_subcommands()
        .filter(|sub| sub.get_name() != HELP_SUBCOMMAND)
}

impl Walker<'_> {
    fn command(&mut self, command: &clap::Command, parent: &str) -> Command {
        let name = command.get_name().to_string();
        let path = if parent.is_empty() {
            name.clone()
        } else {
            format!("{parent} {name}")
        };
        let children: Vec<Command> = subcommands(command)
            .map(|sub| self.command(sub, &path))
            .collect();
        let effect = if children.is_empty() {
            self.effect(&path)
        } else {
            None
        };
        let mut json = false;
        let args = command
            .get_arguments()
            .filter(|arg| !is_builtin(arg))
            .filter_map(|arg| {
                if arg.get_long() == Some(JSON_FLAG) {
                    json = true;
                    None
                } else {
                    Some(describe_arg(arg))
                }
            })
            .collect();
        Command {
            name,
            about: command.get_about().map(ToString::to_string),
            long_about: command.get_long_about().map(ToString::to_string),
            hidden: command.is_hide_set(),
            aliases: command.get_all_aliases().map(str::to_string).collect(),
            effect,
            json,
            args,
            stdin: false,
            timeout_seconds: None,
            mcp: None,
            subcommands: children,
        }
    }

    fn effect(&mut self, path: &str) -> Option<Effect> {
        if let Some((key, effect)) = self.effects.get_key_value(path) {
            self.used.insert(key);
            Some(*effect)
        } else {
            self.missing.push(path.to_string());
            None
        }
    }
}

fn is_builtin(arg: &clap::Arg) -> bool {
    matches!(
        arg.get_action(),
        ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version
    )
}

fn describe_arg(arg: &clap::Arg) -> Arg {
    let kind = if arg.is_positional() {
        ArgKind::Positional
    } else if arg.get_action().takes_values() {
        ArgKind::Option
    } else {
        ArgKind::Flag
    };
    let value_type = if kind == ArgKind::Flag {
        ArgType::Boolean
    } else {
        value_type(arg.get_value_parser())
    };
    let multiple = matches!(arg.get_action(), ArgAction::Append | ArgAction::Count)
        || arg
            .get_num_args()
            .is_some_and(|range| range.max_values() > 1);
    let values = if kind == ArgKind::Flag || value_type != ArgType::String {
        Vec::new()
    } else {
        arg.get_possible_values()
            .iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| value.get_name().to_string())
            .collect()
    };
    Arg {
        name: arg.get_id().to_string(),
        kind,
        long: arg.get_long().map(str::to_string),
        short: arg.get_short(),
        value_type,
        default: if kind == ArgKind::Flag {
            None
        } else {
            default_value(arg, value_type, multiple)
        },
        required: arg.is_required_set(),
        multiple,
        values,
        help: arg
            .get_help()
            .or_else(|| arg.get_long_help())
            .map(ToString::to_string),
    }
}

fn value_type(parser: &ValueParser) -> ArgType {
    let id = parser.type_id();
    let is = |types: &[TypeId]| types.iter().any(|candidate| id == *candidate);
    if is(&[
        TypeId::of::<i8>(),
        TypeId::of::<i16>(),
        TypeId::of::<i32>(),
        TypeId::of::<i64>(),
        TypeId::of::<isize>(),
        TypeId::of::<u8>(),
        TypeId::of::<u16>(),
        TypeId::of::<u32>(),
        TypeId::of::<u64>(),
        TypeId::of::<usize>(),
    ]) {
        ArgType::Integer
    } else if is(&[TypeId::of::<f32>(), TypeId::of::<f64>()]) {
        ArgType::Number
    } else if is(&[TypeId::of::<bool>()]) {
        ArgType::Boolean
    } else if is(&[TypeId::of::<PathBuf>()]) {
        ArgType::Path
    } else {
        ArgType::String
    }
}

fn default_value(arg: &clap::Arg, value_type: ArgType, multiple: bool) -> Option<Value> {
    let defaults: Vec<Value> = arg
        .get_default_values()
        .iter()
        .map(|raw| typed(&raw.to_string_lossy(), value_type))
        .collect();
    if multiple {
        (!defaults.is_empty()).then_some(Value::Array(defaults))
    } else {
        defaults.into_iter().next()
    }
}

fn typed(raw: &str, value_type: ArgType) -> Value {
    let parsed = match value_type {
        ArgType::Integer => raw.parse::<i64>().ok().map(Value::from),
        ArgType::Number => raw
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
        ArgType::Boolean => raw.parse::<bool>().ok().map(Value::Bool),
        ArgType::String | ArgType::Path => None,
    };
    parsed.unwrap_or_else(|| Value::String(raw.to_string()))
}

pub const BLESS_VAR: &str = "VERONICA_BLESS";

fn keep_authored(described: &mut [Command], authored: &[Command]) {
    for command in described {
        if let Some(existing) = authored.iter().find(|item| item.name == command.name) {
            command.stdin = existing.stdin;
            command.timeout_seconds = existing.timeout_seconds;
            command.mcp.clone_from(&existing.mcp);
            keep_authored(&mut command.subcommands, &existing.subcommands);
        }
    }
}

pub fn sync_manifest(path: &std::path::Path, mut described: Vec<Command>) -> Result<(), String> {
    let raw = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut manifest: Manifest = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    keep_authored(&mut described, &manifest.commands);
    if described == manifest.commands {
        return Ok(());
    }
    if std::env::var_os(BLESS_VAR).is_none() {
        return Err(format!(
            "the commands in {} drifted from the backend; rerun with {BLESS_VAR}=1 and format the file",
            path.display()
        ));
    }
    manifest.commands = described;
    let mut body = serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    body.push('\n');
    std::fs::write(path, body).map_err(|error| error.to_string())
}
