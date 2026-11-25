use std::path::Path;

use serde_json::json;
use veronica_protocol::manifest::{Arg, ArgKind, Command, Effect};
use veronica_protocol::rpc::{CliRunParams, CliRunResult};
use veronica_protocol::{ExitCode, Manifest};

#[derive(Debug, Clone)]
pub(crate) enum Gate {
    Manifest(Vec<Command>),
    Unreadable,
}

impl Gate {
    pub(crate) fn load(install: &Path) -> Self {
        let path = install.join("manifest.json");
        let parsed = std::fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|raw| Manifest::parse(&raw).map_err(|error| error.to_string()));
        match parsed {
            Ok(manifest) => Self::Manifest(manifest.commands),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "cannot read {}, so every command needs confirmation",
                    path.display()
                );
                Self::Unreadable
            }
        }
    }

    pub(crate) fn needs_confirmation(&self, argv: &[String]) -> bool {
        match self {
            Self::Unreadable => true,
            Self::Manifest(roots) => leaf(roots, argv).is_some_and(|command| {
                !matches!(command.effect, Some(Effect::Read | Effect::Write))
            }),
        }
    }
}

fn long_name(arg: &Arg) -> &str {
    arg.long.as_deref().unwrap_or(&arg.name)
}

pub(crate) fn leaf<'a>(roots: &'a [Command], argv: &[String]) -> Option<&'a Command> {
    walk(roots, argv).and_then(|path| path.last().copied())
}

fn walk<'a>(roots: &'a [Command], argv: &[String]) -> Option<Vec<&'a Command>> {
    let mut children = roots;
    let mut path: Vec<&Command> = Vec::new();
    let mut current: Option<&Command> = None;
    let mut options: Vec<&Arg> = Vec::new();
    let mut tokens = argv.iter();
    while let Some(token) = tokens.next() {
        if token == "--" {
            break;
        }
        if let Some(name) = token.strip_prefix("--") {
            if !name.contains('=') && options.iter().any(|arg| long_name(arg) == name) {
                tokens.next();
            }
            continue;
        }
        if let Some(cluster) = token.strip_prefix('-').filter(|rest| !rest.is_empty()) {
            let valued = cluster
                .chars()
                .position(|short| options.iter().any(|arg| arg.short == Some(short)));
            if valued == Some(cluster.chars().count() - 1) {
                tokens.next();
            }
            continue;
        }
        let Some(next) = children
            .iter()
            .find(|command| command.names().any(|name| name == token))
        else {
            break;
        };
        options.extend(next.args.iter().filter(|arg| arg.kind == ArgKind::Option));
        current = Some(next);
        path.push(next);
        children = &next.subcommands;
        if next.is_leaf() {
            break;
        }
    }
    current.filter(|command| command.is_leaf()).map(|_| path)
}

impl Gate {
    pub(crate) fn path(&self, argv: &[String]) -> Option<String> {
        match self {
            Self::Unreadable => None,
            Self::Manifest(roots) => walk(roots, argv).map(|path| {
                path.iter()
                    .map(|command| command.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
        }
    }
}

pub(crate) fn destructive_preview(path: &str, json: bool) -> CliRunResult {
    CliRunResult {
        exit_code: ExitCode::ConfirmationRequired,
        document: json.then(|| json!({ "command": path, "effect": "destructive" })),
        text: (!json).then(|| format!("`vr {path}` removes or replaces data.\n\n")),
    }
}

pub(crate) fn confirmation_required(params: &CliRunParams) -> CliRunResult {
    let message = "this command changes or removes data and needs confirmation";
    CliRunResult {
        exit_code: ExitCode::ConfirmationRequired,
        document: params
            .json
            .then(|| json!({ "confirmationRequired": true, "preview": { "message": message } })),
        text: (!params.json).then(|| format!("{message}\n")),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use veronica_protocol::manifest::Command;

    use super::{leaf, Gate};

    fn roots() -> Vec<Command> {
        serde_json::from_value(json!([{
            "name": "files",
            "aliases": ["f"],
            "args": [{ "name": "root", "kind": "option", "long": "root", "short": "r", "type": "path" }],
            "subcommands": [
                { "name": "ls", "effect": "read" },
                { "name": "rm", "effect": "destructive",
                  "args": [{ "name": "path", "kind": "positional", "type": "path" }] },
                { "name": "touch", "effect": "write" }
            ]
        }]))
        .unwrap()
    }

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn the_walk_skips_option_values_and_follows_aliases() {
        let roots = roots();
        let name = |items: &[&str]| leaf(&roots, &argv(items)).map(|command| command.name.clone());
        assert_eq!(name(&["files", "rm", "ls"]).as_deref(), Some("rm"));
        assert_eq!(name(&["f", "--root", "rm", "ls"]).as_deref(), Some("ls"));
        assert_eq!(name(&["files", "-r", "rm", "ls"]).as_deref(), Some("ls"));
        assert_eq!(name(&["files", "--root=x", "rm"]).as_deref(), Some("rm"));
        assert_eq!(name(&["files", "--", "rm"]), None);
        assert_eq!(name(&["files", "nonsense", "rm"]), None);
        assert_eq!(name(&["files"]), None);
    }

    #[test]
    fn only_read_and_write_leaves_run_without_confirmation() {
        let gate = Gate::Manifest(roots());
        assert!(gate.needs_confirmation(&argv(&["files", "rm", "a"])));
        assert!(!gate.needs_confirmation(&argv(&["files", "touch"])));
        assert!(!gate.needs_confirmation(&argv(&["files", "ls"])));
        assert!(!gate.needs_confirmation(&argv(&["files", "bogus"])));
        assert!(Gate::Unreadable.needs_confirmation(&argv(&["files", "ls"])));
    }
}
