use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::builder::{PossibleValuesParser, ValueParser};
use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{value_parser, ArgAction, ArgMatches};
use serde_json::{json, Value};
use veronica_core::{AppDirectories, Settings};
use veronica_host::catalog::CatalogStore;
use veronica_host::connection::BackendError;
use veronica_host::host_api::HostServices;
use veronica_host::registry::{self, Origin};
use veronica_host::{Confirmation, Layout, Router};
use veronica_protocol::manifest::{Arg, ArgKind, ArgType, Command, Effect};
use veronica_protocol::rpc::CliRunResult;
use veronica_protocol::{ExitCode, Manifest};

use crate::format;

const HOST_FLAGS: [&str; 4] = ["--json", "--yes", "-y", "--verbose"];
const HOST_NAMES: [&str; 4] = ["json", "yes", "verbose", "help"];
const HOST_SHORTS: [char; 3] = ['y', 'v', 'h'];

pub(crate) struct Route {
    id: String,
    root: Command,
}

#[derive(Default)]
pub(crate) struct Routes {
    layout: Option<Layout>,
    enabled: Vec<Route>,
    disabled: BTreeMap<String, String>,
}

pub(crate) fn read_manifest(layout: &Layout, id: &str) -> Result<Manifest> {
    let path = layout.current_link(id).join("manifest.json");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    Manifest::parse(&raw).with_context(|| format!("{} is not a valid manifest", path.display()))
}

pub(crate) fn arguments() -> Vec<String> {
    std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

pub(crate) fn wants_json(arguments: &[String]) -> bool {
    arguments
        .iter()
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == "--json")
}

fn is_host_flag(arg: &str) -> bool {
    HOST_FLAGS.contains(&arg)
        || arg
            .strip_prefix('-')
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c == 'v'))
}

fn forwarded(arguments: &[String]) -> Vec<String> {
    let mut literal = false;
    arguments
        .iter()
        .filter(|arg| {
            literal = literal || *arg == "--";
            literal || !is_host_flag(arg)
        })
        .cloned()
        .collect()
}

fn quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

pub(crate) fn rerun(arguments: &[String]) -> String {
    let split = arguments
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(arguments.len());
    let (options, literal) = arguments.split_at(split);
    std::iter::once("vr".to_string())
        .chain(options.iter().map(|arg| quote(arg)))
        .chain(std::iter::once("--yes".to_string()))
        .chain(literal.iter().map(|arg| quote(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Routes {
    pub(crate) fn discover() -> Self {
        Self::load().unwrap_or_else(|error| {
            eprintln!("vr: installed extensions are unavailable: {error:#}");
            Self::default()
        })
    }

    fn load() -> Result<Self> {
        let directories = AppDirectories::current()?;
        let layout = Layout::new(&directories);
        let settings = Settings::load(&directories.settings_file())?;
        let registry = registry::view(&layout.installed_file(), &settings)?;
        let mut routes = Self {
            layout: Some(layout.clone()),
            ..Self::default()
        };
        let packaged = registry
            .extensions
            .iter()
            .filter(|(_, entry)| entry.origin != Origin::Builtin);
        for (id, entry) in packaged {
            let manifest = match read_manifest(&layout, id) {
                Ok(manifest) => manifest,
                Err(error) => {
                    eprintln!("vr: skipping {id}: {error:#}");
                    continue;
                }
            };
            for root in manifest.commands {
                if entry.enabled {
                    routes.enabled.push(Route {
                        id: id.clone(),
                        root,
                    });
                } else {
                    for name in root.names() {
                        routes.disabled.insert(name.to_string(), id.clone());
                    }
                }
            }
        }
        Ok(routes)
    }

    pub(crate) fn command(&mut self, mut program: clap::Command) -> clap::Command {
        let mut taken: BTreeSet<String> = program
            .get_subcommands()
            .flat_map(|sub| std::iter::once(sub.get_name()).chain(sub.get_all_aliases()))
            .map(str::to_string)
            .collect();
        self.enabled.retain(|route| {
            let free = route.root.names().all(|name| !taken.contains(name));
            if free {
                taken.extend(route.root.names().map(str::to_string));
            } else {
                eprintln!(
                    "vr: {} cannot use the command `{}` because it is already taken",
                    route.id, route.root.name
                );
            }
            free
        });
        for route in &self.enabled {
            program = program.subcommand(build(&route.root));
        }
        program
    }

    pub(crate) fn roots(&self) -> impl Iterator<Item = (&str, &Command)> {
        self.enabled
            .iter()
            .map(|route| (route.id.as_str(), &route.root))
    }

    pub(crate) fn route(&self, name: &str) -> Option<&Route> {
        self.enabled.iter().find(|route| route.root.name == name)
    }

    pub(crate) fn reject(&self, error: &clap::Error, arguments: &[String]) -> ExitCode {
        let json = wants_json(arguments);
        if error.kind() == ErrorKind::InvalidSubcommand {
            let root = arguments.iter().find(|arg| !arg.starts_with('-'));
            if let Some(ContextValue::String(name)) = error.get(ContextKind::InvalidSubcommand) {
                if root == Some(name) {
                    if let Some(message) = self.unavailable(name) {
                        return format::fail(json, ExitCode::Unavailable, &message);
                    }
                }
            }
        }
        let _ = error.print();
        if !error.use_stderr() {
            return ExitCode::Success;
        }
        if json {
            let rendered = error.render().to_string();
            format::document(&format::error_document(
                ExitCode::Usage,
                rendered.lines().next().unwrap_or_default(),
            ));
        }
        ExitCode::Usage
    }

    fn unavailable(&self, root: &str) -> Option<String> {
        if let Some(id) = self.disabled.get(root) {
            return Some(format!(
                "{id} is disabled; turn it on with `vr extensions enable {id}`"
            ));
        }
        let store = CatalogStore::new(self.layout.clone()?);
        let loaded = store.load(chrono::Utc::now()).ok()?;
        let extension = loaded.catalog.extension(root)?;
        Some(format!(
            "`{root}` comes from the {} extension, which is not installed; install it with `vr extensions install {}`",
            extension.title, extension.id
        ))
    }
}

fn build(command: &Command) -> clap::Command {
    let mut built = clap::Command::new(command.name.clone())
        .aliases(command.aliases.clone())
        .args(
            command
                .args
                .iter()
                .filter(|arg| !collides(arg))
                .map(argument),
        )
        .subcommands(command.subcommands.iter().map(build));
    if let Some(about) = &command.about {
        built = built.about(about.clone());
    }
    if let Some(long_about) = &command.long_about {
        built = built.long_about(long_about.clone());
    }
    if command.hidden {
        built = built.hide(true);
    }
    if !command.is_leaf() {
        built = built
            .subcommand_required(true)
            .arg_required_else_help(true)
            .disable_help_subcommand(true);
    }
    built
}

fn collides(arg: &Arg) -> bool {
    HOST_NAMES.contains(&arg.name.as_str())
        || arg
            .long
            .as_deref()
            .is_some_and(|long| HOST_NAMES.contains(&long))
}

fn argument(arg: &Arg) -> clap::Arg {
    let mut built = clap::Arg::new(arg.name.clone()).required(arg.required);
    if let Some(help) = &arg.help {
        built = built.help(help.clone());
    }
    if arg.kind == ArgKind::Flag {
        return named(built.action(ArgAction::SetTrue), arg);
    }
    built = built
        .value_name(arg.name.to_uppercase())
        .value_parser(parser(arg))
        .action(if arg.multiple {
            ArgAction::Append
        } else {
            ArgAction::Set
        });
    if arg.multiple && arg.kind == ArgKind::Positional {
        built = built.num_args(1..);
    }
    built = match &arg.default {
        Some(Value::Array(items)) => built.default_values(items.iter().map(literal)),
        Some(value) => built.default_value(literal(value)),
        None => built,
    };
    if arg.kind == ArgKind::Option {
        named(built, arg)
    } else {
        built
    }
}

fn named(built: clap::Arg, arg: &Arg) -> clap::Arg {
    let built = built.long(arg.long.clone().unwrap_or_else(|| arg.name.clone()));
    match arg.short.filter(|short| !HOST_SHORTS.contains(short)) {
        Some(short) => built.short(short),
        None => built,
    }
}

fn literal(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn parser(arg: &Arg) -> ValueParser {
    match arg.value_type {
        ArgType::Integer => value_parser!(i64).into(),
        ArgType::Number => value_parser!(f64).into(),
        ArgType::Boolean => value_parser!(bool),
        ArgType::Path => value_parser!(PathBuf),
        ArgType::String if !arg.values.is_empty() => {
            PossibleValuesParser::new(arg.values.clone()).into()
        }
        ArgType::String => value_parser!(String),
    }
}

fn leaf<'a, 'm>(
    mut command: &'a Command,
    mut matches: &'m ArgMatches,
) -> (&'a Command, &'m ArgMatches) {
    while let Some((name, next_matches)) = matches.subcommand() {
        let Some(next) = command.subcommands.iter().find(|sub| sub.name == name) else {
            break;
        };
        command = next;
        matches = next_matches;
    }
    (command, matches)
}

fn wants_stdin(command: &Command, matches: &ArgMatches) -> bool {
    let mut positionals = command
        .args
        .iter()
        .filter(|arg| arg.kind == ArgKind::Positional)
        .peekable();
    let all_given = positionals.peek().is_some()
        && positionals.all(|arg| {
            matches.value_source(&arg.name) == Some(clap::parser::ValueSource::CommandLine)
        });
    command.stdin && !all_given
}

fn failure(code: ExitCode, message: &str, json: bool) -> CliRunResult {
    CliRunResult {
        exit_code: code,
        document: json.then(|| format::error_document(code, message)),
        text: (!json).then(|| format!("vr: {message}\n")),
    }
}

pub(crate) fn backend_failure(error: &BackendError) -> format::Failure {
    let message = match error {
        BackendError::NotInstalled(id) => {
            format!("{id} is not installed; install it with `vr extensions install {id}`")
        }
        BackendError::Disabled(id) => {
            format!("{id} is disabled; turn it on with `vr extensions enable {id}`")
        }
        other => other.to_string(),
    };
    format::Failure::new(error.exit_code(), message)
}

pub(crate) async fn interruptible<T>(work: impl std::future::Future<Output = T>) -> Option<T> {
    tokio::select! {
        outcome = work => Some(outcome),
        _ = tokio::signal::ctrl_c() => None,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn execute(
    id: &str,
    argv: Vec<String>,
    json: bool,
    confirmation: Confirmation,
    limit: Option<Duration>,
    stdin: Option<String>,
) -> Result<CliRunResult> {
    let router = Router::oneshot(Arc::new(HostServices::current()?));
    let call = async {
        let run = router.run_cli_with_stdin(id, argv, json, confirmation, stdin);
        match limit {
            Some(limit) => tokio::time::timeout(limit, run).await.unwrap_or_else(|_| {
                let message = format!("{id} did not finish within {}s", limit.as_secs());
                failure(ExitCode::Timeout, &message, json)
            }),
            None => run.await,
        }
    };
    Ok(interruptible(call)
        .await
        .unwrap_or_else(|| failure(ExitCode::Interrupted, "interrupted", json)))
}

fn confirmation(result: CliRunResult, rerun: &str, json: bool) -> CliRunResult {
    if json {
        let preview = result.document.unwrap_or(Value::Null);
        let document = if preview.get("confirmationRequired").is_some() {
            preview
        } else {
            json!({ "confirmationRequired": true, "preview": preview, "rerun": rerun })
        };
        return CliRunResult {
            exit_code: ExitCode::ConfirmationRequired,
            document: Some(document),
            text: result.text,
        };
    }
    let mut text = result.text.unwrap_or_default();
    text.push_str("Nothing was changed. To go ahead, run:\n  ");
    text.push_str(rerun);
    text.push('\n');
    CliRunResult {
        exit_code: ExitCode::ConfirmationRequired,
        document: result.document,
        text: Some(text),
    }
}

fn emit(result: CliRunResult, json: bool) -> ExitCode {
    let code = result.exit_code;
    let shown = code.is_success() || code == ExitCode::ConfirmationRequired;
    if json {
        if let Some(text) = &result.text {
            eprint!("{text}");
        }
        let document = result.document.unwrap_or_else(|| {
            if shown {
                Value::Null
            } else {
                format::error_document(code, code.meaning())
            }
        });
        format::document(&document);
        if !shown {
            if let Some(message) = document["error"]["message"].as_str() {
                eprintln!("vr: {message}");
            }
        }
    } else {
        if let Some(document) = &result.document {
            format::document(document);
        }
        match &result.text {
            Some(text) if shown => print!("{text}"),
            Some(text) => eprint!("{text}"),
            None => {}
        }
    }
    if code == ExitCode::ConfirmationRequired {
        eprintln!("vr: confirmation required");
    }
    code
}

pub(crate) fn dispatch(
    route: &Route,
    matches: &ArgMatches,
    arguments: &[String],
    json: bool,
    yes: bool,
) -> ExitCode {
    let (leaf, leaf_matches) = matches
        .subcommand()
        .map_or((&route.root, matches), |(_, root)| leaf(&route.root, root));
    let limit = leaf
        .timeout_seconds
        .map(|seconds| Duration::from_secs(u64::from(seconds)));
    let confirmed = Confirmation::from_yes_flag(yes);
    let stdin = if wants_stdin(leaf, leaf_matches) {
        let mut text = String::new();
        if let Err(error) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text) {
            return format::fail(
                json,
                ExitCode::Failed,
                &format!("cannot read stdin: {error}"),
            );
        }
        Some(text)
    } else {
        None
    };
    let result = match execute(
        &route.id,
        forwarded(arguments),
        json,
        confirmed,
        limit,
        stdin,
    ) {
        Ok(result) => result,
        Err(error) => return format::fail(json, ExitCode::Failed, &format!("{error:#}")),
    };
    let destructive = leaf.effect == Some(Effect::Destructive);
    let result = if destructive && !yes && result.exit_code == ExitCode::ConfirmationRequired {
        confirmation(result, &rerun(arguments), json)
    } else {
        result
    };
    emit(result, json)
}

#[cfg(test)]
mod tests {
    use super::{forwarded, quote, rerun};

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn host_flags_are_not_forwarded_to_the_backend() {
        let argv = strings(&["--json", "fixture", "-vv", "echo", "-y", "--", "--json"]);
        assert_eq!(
            forwarded(&argv),
            strings(&["fixture", "echo", "--", "--json"])
        );
    }

    #[test]
    fn rerun_lines_are_shell_safe() {
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
        assert_eq!(
            rerun(&strings(&["fixture", "wipe", "a b"])),
            "vr fixture wipe 'a b' --yes"
        );
        assert_eq!(
            rerun(&strings(&["fixture", "wipe", "--", "--yes"])),
            "vr fixture wipe --yes -- --yes"
        );
    }
}
