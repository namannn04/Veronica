mod common;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};
use veronica_protocol::{canonical, ExitCode};

const PATIENCE: Duration = Duration::from_mins(1);

struct Sandbox {
    root: TempDir,
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = pipe.read_to_string(&mut text);
        text
    })
}

impl Sandbox {
    fn new() -> Self {
        let sandbox = Self {
            root: tempfile::tempdir().unwrap(),
        };
        for dir in [sandbox.empty_path(), sandbox.work()] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let layout = Layout::new(&AppDirectories::with_env(
            sandbox.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(sandbox.root.path(), "fixture"))
            .unwrap();
        sandbox
    }

    fn empty_path(&self) -> PathBuf {
        self.root.path().join("empty-path")
    }

    fn work(&self) -> PathBuf {
        self.root.path().join("work")
    }

    fn vr(&self, args: &[String]) -> Run {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vr"))
            .env_clear()
            .env("HOME", self.root.path())
            .env("PATH", self.empty_path())
            .env("LANG", "C.UTF-8")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/session")
            .env("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/nonexistent/system")
            .current_dir(self.work())
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = drain(child.stdout.take().unwrap());
        let stderr = drain(child.stderr.take().unwrap());
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if started.elapsed() > PATIENCE {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        Run {
            code: status.and_then(|status| status.code()),
            stdout: stdout.join().unwrap(),
            stderr: stderr.join().unwrap(),
        }
    }
}

fn sample(arg: &Value, scratch: &Path) -> String {
    if let Some(first) = arg["values"].as_array().and_then(|values| values.first()) {
        return first.as_str().unwrap().to_string();
    }
    match arg["type"].as_str() {
        Some("integer" | "number") => "1".to_string(),
        Some("boolean") => "true".to_string(),
        Some("path") => scratch.join("missing").display().to_string(),
        _ => "sample".to_string(),
    }
}

fn argv(leaf: &Value, scratch: &Path) -> Vec<String> {
    let mut argv = vec!["--json".to_string()];
    argv.extend(leaf["path"].as_str().unwrap().split(' ').map(String::from));
    for arg in leaf["args"].as_array().unwrap() {
        if arg["required"] != true {
            continue;
        }
        match arg["kind"].as_str() {
            Some("positional") => argv.push(sample(arg, scratch)),
            Some("option") => {
                let long = arg["long"].as_str().or_else(|| arg["name"].as_str());
                argv.push(format!("--{}", long.unwrap()));
                argv.push(sample(arg, scratch));
            }
            _ => {}
        }
    }
    argv
}

fn single_document(run: &Run) -> Result<Value, String> {
    let value: Value = serde_json::from_str(&run.stdout)
        .map_err(|error| format!("stdout is not one JSON value ({error}): {:?}", run.stdout))?;
    if run.stdout == canonical::value_to_string_pretty(&value) {
        Ok(value)
    } else {
        Err(format!("stdout is not canonical: {:?}", run.stdout))
    }
}

fn check(leaf: &Value, run: &Run) -> Result<(), String> {
    let code = run.code.ok_or("killed or timed out")?;
    let exit =
        ExitCode::from_code(code).ok_or_else(|| format!("exit {code} is not in the table"))?;
    let document = single_document(run)?;
    let nothing_to_do = ["updated", "install"]
        .iter()
        .any(|key| document[*key].as_array().is_some_and(Vec::is_empty));
    if leaf["effect"] == "destructive" && exit.is_success() && !nothing_to_do {
        return Err("a destructive command ran without --yes".to_string());
    }
    let explained = exit.is_success()
        || (exit == ExitCode::ConfirmationRequired && document["confirmationRequired"] == true)
        || document["error"]["code"] == code;
    if explained {
        Ok(())
    } else {
        Err(format!("exit {code} came without an error document"))
    }
}

#[test]
fn every_json_leaf_prints_one_canonical_document_and_a_known_exit_code() {
    let sandbox = Sandbox::new();
    let guide = sandbox.vr(&["--json".to_string(), "guide".to_string()]);
    assert_eq!(guide.code, Some(0), "{}", guide.stderr);
    let guide = single_document(&guide).unwrap();
    let leaves: Vec<&Value> = guide["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|leaf| leaf["json"] == true)
        .collect();
    assert!(leaves.iter().any(|leaf| leaf["origin"] == "host"));
    assert!(leaves.iter().any(|leaf| leaf["origin"] == "fixture"));
    assert!(leaves.iter().all(|leaf| leaf["effect"].is_string()));

    let scratch = sandbox.root.path().join("scratch");
    let failures: Vec<String> = leaves
        .iter()
        .filter_map(|leaf| {
            let args = argv(leaf, &scratch);
            let run = sandbox.vr(&args);
            check(leaf, &run)
                .err()
                .map(|problem| format!("vr {}: {problem}\n{}", args.join(" "), run.stderr))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
