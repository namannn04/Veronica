use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use veronica_protocol::manifest::Effect;
use veronica_protocol::rpc::CliRunParams;
use veronica_protocol::ExitCode;
use veronica_sdk::cli::{describe, parse, DescribeError};
use veronica_sdk::{Capture, Failure};

#[derive(Debug, Parser)]
struct Program {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    root: Root,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Style {
    Plain,
    Loud,
}

#[derive(Debug, Subcommand)]
enum Root {
    #[command(about = "Say hello")]
    Greet {
        name: String,
        #[arg(long, short, default_value_t = 1)]
        times: u32,
        #[arg(long, value_enum, default_value = "plain")]
        style: Style,
        #[arg(long)]
        ratio: Option<f64>,
        #[arg(long)]
        quiet: bool,
    },
    #[command(about = "Manage files", visible_alias = "f")]
    Files {
        #[command(subcommand)]
        action: FilesAction,
    },
}

#[derive(Debug, Subcommand)]
enum FilesAction {
    #[command(alias = "list")]
    Ls {
        #[arg(help = "Directory to list")]
        dir: Option<PathBuf>,
    },
    Rm {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
}

const EFFECTS: [(&str, Effect); 3] = [
    ("greet", Effect::Read),
    ("files ls", Effect::Read),
    ("files rm", Effect::Destructive),
];

#[test]
fn describe_turns_a_clap_tree_into_manifest_commands() {
    let commands = describe(Program::command(), &EFFECTS).unwrap();
    let described = serde_json::to_value(&commands).unwrap();
    assert_eq!(
        described,
        json!([
            {
                "name": "greet",
                "about": "Say hello",
                "effect": "read",
                "json": true,
                "args": [
                    { "name": "name", "kind": "positional", "type": "string", "required": true },
                    { "name": "times", "kind": "option", "long": "times", "short": "t", "type": "integer", "default": 1 },
                    { "name": "style", "kind": "option", "long": "style", "type": "string", "default": "plain", "values": ["plain", "loud"] },
                    { "name": "ratio", "kind": "option", "long": "ratio", "type": "number" },
                    { "name": "quiet", "kind": "flag", "long": "quiet", "type": "boolean" }
                ]
            },
            {
                "name": "files",
                "about": "Manage files",
                "aliases": ["f"],
                "json": true,
                "subcommands": [
                    {
                        "name": "ls",
                        "aliases": ["list"],
                        "effect": "read",
                        "json": true,
                        "args": [
                            { "name": "dir", "kind": "positional", "type": "path", "help": "Directory to list" }
                        ]
                    },
                    {
                        "name": "rm",
                        "effect": "destructive",
                        "json": true,
                        "args": [
                            { "name": "paths", "kind": "positional", "type": "path", "required": true, "multiple": true }
                        ]
                    }
                ]
            }
        ])
    );
}

#[test]
fn every_leaf_needs_an_effect() {
    let error = describe(Program::command(), &EFFECTS[..2]).unwrap_err();
    assert_eq!(
        error,
        DescribeError::MissingEffects(vec!["files rm".into()])
    );
}

#[test]
fn effects_for_unknown_or_branch_paths_are_rejected() {
    let mut effects = EFFECTS.to_vec();
    effects.push(("files", Effect::Read));
    effects.push(("greet loudly", Effect::Write));
    let error = describe(Program::command(), &effects).unwrap_err();
    assert_eq!(
        error,
        DescribeError::UnknownEffects(vec!["files".into(), "greet loudly".into()])
    );
}

fn params(argv: &[&str], json: bool) -> CliRunParams {
    CliRunParams {
        argv: argv.iter().map(ToString::to_string).collect(),
        json,
        confirm: false,
        stdin: None,
        cwd: PathBuf::from("/"),
        tty: false,
    }
}

#[test]
fn parse_reads_argv_from_the_root_command() {
    let program: Program = parse(&params(&["files", "rm", "a", "b"], false)).unwrap();
    let Root::Files {
        action: FilesAction::Rm { paths },
    } = program.root
    else {
        panic!("expected files rm");
    };
    assert_eq!(paths, vec![PathBuf::from("a"), PathBuf::from("b")]);
}

#[test]
fn help_succeeds_with_text_and_bad_usage_exits_two() {
    let help = parse::<Program>(&params(&["greet", "--help"], false)).unwrap_err();
    assert_eq!(help.exit_code, ExitCode::Success);
    assert!(help.text.unwrap().contains("Say hello"));

    let usage = parse::<Program>(&params(&["greet"], false)).unwrap_err();
    assert_eq!(usage.exit_code, ExitCode::Usage);
    assert!(usage.text.unwrap().contains("<NAME>"));

    let usage = parse::<Program>(&params(&["greet"], true)).unwrap_err();
    assert_eq!(usage.exit_code, ExitCode::Usage);
    assert_eq!(usage.document.unwrap()["error"]["code"], json!(2));
}

#[test]
fn text_output_is_captured_line_by_line() {
    let capture = Capture::new(false);
    let output = capture.output();
    output
        .emit(&json!({ "ignored": true }), || "first".into())
        .unwrap();
    output.emit(&json!(null), String::new).unwrap();
    output.emit(&json!(null), || "second".into()).unwrap();
    let result = capture.finish(Ok(()));
    assert_eq!(result.exit_code, ExitCode::Success);
    assert_eq!(result.document, None);
    assert_eq!(result.text.as_deref(), Some("first\nsecond\n"));
}

#[test]
fn json_output_captures_exactly_one_document() {
    let capture = Capture::new(true);
    let output = capture.output();
    assert!(output.is_json());
    output
        .emit(&json!({ "b": 1, "a": 2 }), || unreachable!())
        .unwrap();
    assert!(output.emit(&json!(1), String::new).is_err());
    let result = capture.finish(Ok(()));
    assert_eq!(result.document, Some(json!({ "a": 2, "b": 1 })));
    assert_eq!(result.text, None);
}

#[test]
fn failures_carry_their_exit_code_and_an_error_document() {
    let capture = Capture::new(true);
    let result = capture.finish(Err(Failure::new(ExitCode::NotFound, "no such thing").into()));
    assert_eq!(result.exit_code, ExitCode::NotFound);
    assert_eq!(
        result.document,
        Some(json!({ "error": { "code": 3, "message": "no such thing" } }))
    );

    let capture = Capture::new(false);
    let result = capture.finish(Err(anyhow::anyhow!("broke")));
    assert_eq!(result.exit_code, ExitCode::Failed);
    assert_eq!(result.text.as_deref(), Some("vr: broke\n"));
    assert_eq!(result.document, None::<Value>);
}
