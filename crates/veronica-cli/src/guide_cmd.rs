use std::fmt::Write as _;

use anyhow::Result;
use clap::{ArgMatches, CommandFactory as _};
use serde::Serialize;
use serde_json::{json, Value};
use veronica_protocol::manifest::{Arg, Command, Effect, MAX_MCP_TOOL_NAME};
use veronica_protocol::{ExitCode, Manifest};

use crate::format::Output;
use crate::routes::Routes;

const HOST: &str = "host";
const HOST_FLAGS: [&str; 3] = ["json", "yes", "verbose"];
const WITHOUT_JSON: [&str; 2] = ["database mcp", "mcp serve"];
const WITHOUT_MCP: [&str; 2] = ["database", "mcp"];
const SELF_PREVIEW: [&str; 9] = [
    "cleaner clean",
    "extensions install",
    "extensions remove",
    "extensions setup",
    "extensions update",
    "maintenance remove",
    "maintenance update",
    "mcp install",
    "mcp uninstall",
];

pub(crate) const HOST_EFFECTS: &[(&str, Effect)] = &[
    ("alerts clear", Effect::Write),
    ("alerts state", Effect::Read),
    ("alerts test", Effect::Write),
    ("attention configure", Effect::Write),
    ("attention focus start", Effect::Write),
    ("attention focus status", Effect::Read),
    ("attention focus stop", Effect::Write),
    ("attention history", Effect::Read),
    ("attention record", Effect::Write),
    ("attention status", Effect::Read),
    ("attention summary", Effect::Read),
    ("backup export", Effect::Write),
    ("backup import", Effect::Destructive),
    ("backup inspect", Effect::Read),
    ("cleaner categories", Effect::Read),
    ("cleaner clean", Effect::Destructive),
    ("cleaner drives", Effect::Read),
    ("cleaner scan", Effect::Read),
    ("clipboard clear", Effect::Destructive),
    ("clipboard get", Effect::Read),
    ("clipboard list", Effect::Read),
    ("clipboard record", Effect::Write),
    ("clipboard remove", Effect::Destructive),
    ("color clear", Effect::Destructive),
    ("color copy", Effect::Write),
    ("color list", Effect::Read),
    ("color pick", Effect::Write),
    ("color remove", Effect::Destructive),
    ("color show", Effect::Read),
    ("companion add", Effect::Write),
    ("companion list", Effect::Read),
    ("companion pin", Effect::Write),
    ("companion remove", Effect::Destructive),
    ("companion show", Effect::Read),
    ("config get", Effect::Read),
    ("config list", Effect::Read),
    ("config set", Effect::Write),
    ("config unset", Effect::Write),
    ("database browse", Effect::Read),
    ("database capabilities", Effect::Read),
    ("database connections add", Effect::Write),
    ("database connections get", Effect::Read),
    ("database connections list", Effect::Read),
    ("database connections policy", Effect::Write),
    ("database connections remove", Effect::Destructive),
    ("database mcp", Effect::Read),
    ("database mutations apply", Effect::Destructive),
    ("database mutations preview", Effect::Write),
    ("database operations", Effect::Read),
    ("database query", Effect::Read),
    ("database read", Effect::Read),
    ("database saved-queries get", Effect::Read),
    ("database saved-queries list", Effect::Read),
    ("database saved-queries remove", Effect::Destructive),
    ("database saved-queries save", Effect::Write),
    ("database test", Effect::Read),
    ("diagnose", Effect::Read),
    ("docker action", Effect::Destructive),
    ("docker status", Effect::Read),
    ("emoji copy", Effect::Write),
    ("emoji forget", Effect::Destructive),
    ("emoji groups", Effect::Read),
    ("emoji list", Effect::Read),
    ("emoji recents", Effect::Read),
    ("extensions auto-update exclude", Effect::Write),
    ("extensions auto-update hours", Effect::Write),
    ("extensions auto-update include", Effect::Write),
    ("extensions auto-update off", Effect::Write),
    ("extensions auto-update on", Effect::Write),
    ("extensions disable", Effect::Write),
    ("extensions doctor", Effect::Read),
    ("extensions enable", Effect::Write),
    ("extensions info", Effect::Read),
    ("extensions install", Effect::Destructive),
    ("extensions ls", Effect::Read),
    ("extensions remove", Effect::Destructive),
    ("extensions search", Effect::Read),
    ("extensions setup", Effect::Destructive),
    ("extensions status", Effect::Read),
    ("extensions update", Effect::Destructive),
    ("extensions verify", Effect::Read),
    ("focus-dim fade", Effect::Write),
    ("focus-dim intensity", Effect::Write),
    ("focus-dim mode", Effect::Write),
    ("focus-dim off", Effect::Write),
    ("focus-dim on", Effect::Write),
    ("focus-dim status", Effect::Read),
    ("guide", Effect::Read),
    ("herdr attach", Effect::Read),
    ("herdr board", Effect::Read),
    ("herdr sessions", Effect::Read),
    ("invoke", Effect::Write),
    ("keystroke-highlight disable", Effect::Write),
    ("keystroke-highlight duration", Effect::Write),
    ("keystroke-highlight enable", Effect::Write),
    ("keystroke-highlight off", Effect::Write),
    ("keystroke-highlight on", Effect::Write),
    ("keystroke-highlight position", Effect::Write),
    ("keystroke-highlight status", Effect::Read),
    ("machines add", Effect::Write),
    ("machines container", Effect::Destructive),
    ("machines containers", Effect::Read),
    ("machines discover", Effect::Read),
    ("machines list", Effect::Read),
    ("machines logs", Effect::Read),
    ("machines power", Effect::Destructive),
    ("machines remove", Effect::Destructive),
    ("machines stats", Effect::Read),
    ("machines wake", Effect::Write),
    ("maintenance inventory", Effect::Read),
    ("maintenance remove", Effect::Destructive),
    ("maintenance sources", Effect::Read),
    ("maintenance update", Effect::Destructive),
    ("maintenance updates", Effect::Read),
    ("media library", Effect::Read),
    ("media next", Effect::Write),
    ("media pause", Effect::Write),
    ("media play", Effect::Write),
    ("mcp install", Effect::Destructive),
    ("mcp serve", Effect::Read),
    ("mcp status", Effect::Read),
    ("mcp uninstall", Effect::Destructive),
    ("media players", Effect::Read),
    ("media previous", Effect::Write),
    ("media status", Effect::Read),
    ("media stop", Effect::Write),
    ("media toggle", Effect::Write),
    ("ports list", Effect::Read),
    ("ports stop", Effect::Destructive),
    ("power keep-awake off", Effect::Write),
    ("power keep-awake on", Effect::Write),
    ("power lid-awake off", Effect::Write),
    ("power lid-awake on", Effect::Write),
    ("power status", Effect::Read),
    ("presenter blur", Effect::Write),
    ("presenter disable", Effect::Write),
    ("presenter dismiss", Effect::Write),
    ("presenter enable", Effect::Write),
    ("presenter resume", Effect::Write),
    ("presenter start", Effect::Write),
    ("presenter status", Effect::Read),
    ("presenter stop", Effect::Write),
    ("quinjet launch", Effect::Write),
    ("quinjet open", Effect::Read),
    ("quinjet projects", Effect::Read),
    ("quinjet worktrees", Effect::Read),
    ("shelf add", Effect::Write),
    ("shelf clear", Effect::Destructive),
    ("shelf list", Effect::Read),
    ("shelf remove", Effect::Destructive),
    ("surfaces", Effect::Read),
    ("system audio list", Effect::Read),
    ("system audio mute", Effect::Write),
    ("system audio unmute", Effect::Write),
    ("system audio volume", Effect::Write),
    ("system bluetooth", Effect::Read),
    ("system disks", Effect::Read),
    ("system processes", Effect::Read),
    ("system snapshot", Effect::Read),
    ("system stats", Effect::Read),
    ("tools install", Effect::Write),
    ("tools ls", Effect::Read),
    ("usage alerts", Effect::Read),
    ("usage calendar", Effect::Read),
    ("usage export", Effect::Write),
    ("usage limits", Effect::Read),
    ("usage models", Effect::Read),
    ("usage projects", Effect::Read),
    ("usage refresh", Effect::Write),
    ("usage sources", Effect::Read),
    ("usage summary", Effect::Read),
    ("utility image", Effect::Write),
    ("utility qr", Effect::Write),
    ("workspace doctor", Effect::Read),
    ("workspace graft call", Effect::Write),
    ("workspace graft discover", Effect::Read),
];

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Topic {
    Start,
    Conventions,
    ExitCodes,
    Extensions,
    Agents,
}

impl Topic {
    const ALL: [Self; 5] = [
        Self::Start,
        Self::Conventions,
        Self::ExitCodes,
        Self::Extensions,
        Self::Agents,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Conventions => "conventions",
            Self::ExitCodes => "exit-codes",
            Self::Extensions => "extensions",
            Self::Agents => "agents",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Start => "Getting started",
            Self::Conventions => "Conventions",
            Self::ExitCodes => "Exit codes",
            Self::Extensions => "Working with extensions",
            Self::Agents => "Agents and MCP",
        }
    }

    fn body(self) -> String {
        match self {
            Self::Start => START.to_string(),
            Self::Conventions => CONVENTIONS.to_string(),
            Self::ExitCodes => ExitCode::ALL
                .iter()
                .map(|code| format!("{:>4}  {}", code.code(), code.meaning()))
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Extensions => EXTENSIONS.to_string(),
            Self::Agents => AGENTS.to_string(),
        }
    }
}

const START: &str = "\
`vr` drives everything Veronica does. Commands read as `vr <area> <action>`, and
`vr --help` lists the areas this computer has, including the ones that installed
extensions add. `vr <area> --help` describes each action and its arguments.

  vr diagnose             the session, its capabilities and the extension state
  vr extensions           every extension and whether it can run here
  vr extensions setup     install the starter extensions of a profile
  vr guide <topic>        one part of this manual
  vr guide --json         every command as data, for scripts and agents";

const CONVENTIONS: &str = "\
--json      print exactly one JSON document on stdout: sorted keys, a trailing
            newline, nothing else. Messages and warnings go to stderr.
--yes, -y   confirm a command that removes or replaces data. Without it such a
            command changes nothing: it prints a preview and exits 5. In JSON
            the preview is {\"confirmationRequired\": true, \"preview\": ...,
            \"rerun\": \"...\"}, and rerun is the same command with --yes.
-v          more log detail on stderr; repeat it for more.

A failure in JSON mode prints {\"error\": {\"code\": N, \"message\": \"...\"}} and
exits with the same code. `vr guide exit-codes` lists them.";

const EXTENSIONS: &str = "\
Features arrive as extensions. Some are still built in; the rest come from the
signed catalog or, while developing one, from a folder.

  vr extensions search <words>    what the catalog offers
  vr extensions info <id>         what an extension does, needs and adds
  vr extensions install <id>      preview the install; add --yes to download,
                                  verify and install it
  vr extensions install ./folder  a development install, never auto-updated
  vr extensions update [id]       preview updates; --yes refreshes the catalog
                                  and applies them
  vr extensions enable <id>       turn one on; disable turns it off
  vr extensions remove <id>       remove it, keeping its data unless --purge
  vr extensions verify [id]       re-hash installed files; exits 8 on a mismatch
  vr extensions doctor            why an extension cannot run here
  vr extensions auto-update       show or change automatic updates

Only commands given --yes reach the network; everything else reads the catalog
already on disk.";

const AGENTS: &str = "\
An agent should run `vr guide --json` once. It lists every command on this
computer, from Veronica and from installed extensions, with its arguments, its
effect and whether it prints JSON:

  read         looks only; safe to run at any time
  write        changes settings or state that is easy to change back
  destructive  removes or replaces data; ask the person before adding --yes

Run commands with --json and read the exit code. A destructive command run
without --yes exits 5 and prints what it would do, which is the preview to show
the person. Commands that need the desktop session exit 4 when there is none.

`vr mcp` serves the same commands as MCP tools over stdin and stdout, one tool
per command, named veronica_<area>_<action>. A destructive tool changes nothing
until it is called with confirm set, and veronica_catalog finds and installs
extensions. The server also lists the read-only database tools
database_connections and database_capabilities, and the resources
veronica://guide, veronica://guide/<topic> and veronica://extensions/<id>.

`vr mcp install --client claude|codex|cursor|vscode|all` registers the server
with an agent client: it previews the change until given --yes, keeps every
other entry and leaves a backup beside the file. `vr mcp status` shows which
clients have it, and `vr mcp uninstall` takes it out again.";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Leaf {
    pub(crate) path: String,
    pub(crate) origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) about: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) effect: Option<Effect>,
    pub(crate) json: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mcp_tool: Option<String>,
    pub(crate) args: Vec<Arg>,
}

fn host_tree() -> Vec<Command> {
    veronica_sdk::cli::describe(crate::Cli::command(), HOST_EFFECTS)
        .expect("every host command declares its effect")
}

fn mcp_tool(host: bool, path: &str, command: &Command, json: bool) -> Option<String> {
    let hidden = host
        && WITHOUT_MCP
            .iter()
            .any(|root| path == *root || path.starts_with(&format!("{root} ")));
    if !json || hidden || !command.mcp_exposed() {
        return None;
    }
    let route: Vec<&str> = path.split(' ').collect();
    let name = Manifest::mcp_tool_name(&route, command);
    (name.len() <= MAX_MCP_TOOL_NAME).then_some(name)
}

fn leaves(origin: &str, command: &Command, parent: &str, out: &mut Vec<Leaf>) {
    let path = if parent.is_empty() {
        command.name.clone()
    } else {
        format!("{parent} {}", command.name)
    };
    for child in &command.subcommands {
        leaves(origin, child, &path, out);
    }
    if !command.is_leaf() {
        return;
    }
    let host = origin == HOST;
    let json = command.json && !(host && WITHOUT_JSON.contains(&path.as_str()));
    out.push(Leaf {
        json,
        mcp_tool: mcp_tool(host, &path, command, json),
        args: command
            .args
            .iter()
            .filter(|arg| !(host && HOST_FLAGS.contains(&arg.name.as_str())))
            .cloned()
            .collect(),
        about: command.about.clone(),
        effect: command.effect,
        origin: origin.to_string(),
        path,
    });
}

pub(crate) fn catalog(routes: &Routes) -> Vec<Leaf> {
    let mut commands = Vec::new();
    for command in &host_tree() {
        leaves(HOST, command, "", &mut commands);
    }
    for (id, root) in routes.roots() {
        leaves(id, root, "", &mut commands);
    }
    commands.sort_by(|left, right| left.path.cmp(&right.path));
    commands
}

pub(crate) fn topic(id: &str) -> Option<Value> {
    let topic = Topic::ALL.into_iter().find(|topic| topic.id() == id)?;
    Some(json!({ "id": topic.id(), "title": topic.title(), "text": topic.body() }))
}

pub(crate) fn topics() -> impl Iterator<Item = (&'static str, &'static str)> {
    Topic::ALL
        .into_iter()
        .map(|topic| (topic.id(), topic.title()))
}

pub(crate) fn document(routes: &Routes) -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "topics": Topic::ALL
            .iter()
            .map(|topic| json!({ "id": topic.id(), "title": topic.title() }))
            .collect::<Vec<_>>(),
        "exitCodes": ExitCode::ALL
            .iter()
            .map(|code| json!({ "code": code.code(), "meaning": code.meaning() }))
            .collect::<Vec<_>>(),
        "commands": catalog(routes),
    })
}

pub(crate) fn effect(path: &str) -> Option<Effect> {
    HOST_EFFECTS
        .iter()
        .find(|(candidate, _)| *candidate == path)
        .map(|(_, effect)| *effect)
}

pub(crate) fn path(matches: &ArgMatches) -> String {
    let mut names = Vec::new();
    let mut current = matches;
    while let Some((name, next)) = current.subcommand() {
        names.push(name);
        current = next;
    }
    names.join(" ")
}

pub(crate) fn needs_confirmation(path: &str) -> bool {
    effect(path) == Some(Effect::Destructive) && !SELF_PREVIEW.contains(&path)
}

pub(crate) fn run(routes: &Routes, topic: Option<Topic>, output: Output) -> Result<()> {
    match (topic, output) {
        (Some(topic), _) => {
            let body = topic.body();
            let document = json!({ "id": topic.id(), "title": topic.title(), "text": body });
            output.emit(&document, || format!("{}\n\n{body}", topic.title()))
        }
        (None, Output::Json) => output.emit(&document(routes), String::new),
        (None, Output::Text) => {
            let mut text = String::new();
            for topic in Topic::ALL {
                let _ = write!(text, "{}\n\n{}\n\n", topic.title(), topic.body());
            }
            let _ = write!(text, "Topics: {}", Topic::ALL.map(Topic::id).join(", "));
            output.emit(&(), || text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_host_leaf_declares_an_effect() {
        let found = catalog(&Routes::default());
        assert_eq!(found.len(), HOST_EFFECTS.len());
        for path in SELF_PREVIEW.iter().chain(WITHOUT_JSON.iter()) {
            assert!(effect(path).is_some(), "{path}");
        }
        let install = found
            .iter()
            .find(|leaf| leaf.path == "extensions install")
            .unwrap();
        assert_eq!(install.effect, Some(Effect::Destructive));
        assert!(install.json);
        assert!(install.args.iter().all(|arg| arg.name != "yes"));
    }

    #[test]
    fn destructive_host_commands_are_gated_unless_they_preview_themselves() {
        assert!(needs_confirmation("clipboard clear"));
        assert!(!needs_confirmation("cleaner clean"));
        assert!(!needs_confirmation("clipboard list"));
        assert!(!needs_confirmation("nothing here"));
    }
}
