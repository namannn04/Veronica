//! Herdr session discovery for Veronica's agent board.
//!
//! Herdr owns its socket protocol and ships a stable JSON CLI. Veronica uses
//! that boundary instead of duplicating the protocol, so installed Herdr
//! versions remain the source of truth for sessions and agent state.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HerdrBoard {
    pub installed: bool,
    pub executable: Option<String>,
    pub sessions: Vec<HerdrSession>,
    pub agents: Vec<HerdrAgent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HerdrSession {
    pub name: String,
    pub running: bool,
    #[serde(default)]
    pub default: bool,
    pub session_dir: String,
    pub socket_path: String,
    #[serde(default, skip_deserializing)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HerdrAgent {
    pub id: String,
    pub session: String,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub workspace: String,
    pub cwd: String,
    pub pane_id: String,
    pub focused: bool,
}

#[derive(Deserialize)]
struct SessionDocument {
    sessions: Vec<HerdrSession>,
}

/// Resolve Herdr from the GUI-safe locations as well as PATH. Desktop entries
/// often do not inherit the user's login PATH, while Herdr commonly lives in
/// `~/.local/bin`.
pub fn executable() -> Option<PathBuf> {
    let named = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|directory| directory.join("herdr"));
    let fixed = [PathBuf::from("/usr/bin/herdr"), PathBuf::from("/usr/local/bin/herdr")];
    let local = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/bin/herdr"));
    named.chain(fixed).chain(local).find(|path| path.is_file())
}

pub fn board() -> Result<HerdrBoard> {
    let Some(executable) = executable() else {
        return Ok(HerdrBoard { installed: false, executable: None, sessions: Vec::new(), agents: Vec::new() });
    };
    let output = Command::new(&executable)
        .args(["session", "list", "--json"])
        .output()
        .context("cannot list Herdr sessions")?;
    if !output.status.success() {
        bail!("Herdr session discovery failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let document: SessionDocument = serde_json::from_slice(&output.stdout).context("Herdr returned an invalid session list")?;
    let mut sessions = document.sessions;
    let mut agents = Vec::new();
    for session in sessions.iter_mut().filter(|session| session.running) {
        match snapshot(&executable, &session.name) {
            Ok(mut rows) => agents.append(&mut rows),
            Err(error) => session.error = Some(error.to_string()),
        }
    }
    Ok(HerdrBoard {
        installed: true,
        executable: Some(executable.display().to_string()),
        sessions,
        agents,
    })
}

fn snapshot(executable: &Path, session: &str) -> Result<Vec<HerdrAgent>> {
    let output = Command::new(executable)
        .args(["--session", session, "api", "snapshot"])
        .output()
        .with_context(|| format!("cannot read Herdr session {session}"))?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let value: Value = serde_json::from_slice(&output.stdout).context("Herdr returned an invalid snapshot")?;
    Ok(agents_from_snapshot(session, &value))
}

fn agents_from_snapshot(session: &str, value: &Value) -> Vec<HerdrAgent> {
    let snapshot = value.get("snapshot").or_else(|| value.get("result").and_then(|result| result.get("snapshot"))).unwrap_or(value);
    let workspaces = snapshot.get("workspaces").and_then(Value::as_array).cloned().unwrap_or_default();
    let label_for = |id: &str| -> String {
        workspaces.iter().find(|workspace| workspace.get("workspace_id").and_then(Value::as_str) == Some(id))
            .and_then(|workspace| workspace.get("label").and_then(Value::as_str)).unwrap_or(id).to_string()
    };
    snapshot.get("agents").and_then(Value::as_array).into_iter().flatten().filter_map(|agent| {
        let pane = agent.get("pane_id")?.as_str()?.to_string();
        let workspace_id = agent.get("workspace_id").and_then(Value::as_str).unwrap_or_default();
        let raw_kind = agent.get("display_agent").or_else(|| agent.get("agent")).and_then(Value::as_str).unwrap_or("Unknown");
        let title = agent.get("name").or_else(|| agent.get("title")).or_else(|| agent.get("terminal_title_stripped")).and_then(Value::as_str).filter(|text| !text.is_empty()).unwrap_or(raw_kind);
        Some(HerdrAgent {
            id: format!("{session}|{pane}"), session: session.to_string(), kind: display_kind(raw_kind),
            status: status(agent.get("agent_status")), title: title.to_string(), workspace: label_for(workspace_id),
            cwd: agent.get("foreground_cwd").or_else(|| agent.get("cwd")).and_then(Value::as_str).unwrap_or_default().to_string(),
            pane_id: pane, focused: agent.get("focused").and_then(Value::as_bool).unwrap_or(false),
        })
    }).collect()
}

fn status(value: Option<&Value>) -> String {
    let raw = value.and_then(Value::as_str).unwrap_or("unknown").to_ascii_lowercase();
    match raw.as_str() {
        "blocked" | "needs_attention" | "waiting" | "waiting_for_input" | "approval" => "blocked",
        "working" | "running" | "busy" | "in_progress" | "thinking" => "working",
        "done" | "complete" | "completed" | "finished" | "success" => "done",
        "idle" | "ready" | "stopped" => "idle",
        _ => "unknown",
    }.to_string()
}

fn display_kind(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().replace(' ', "-").as_str() {
        "claude" | "claude-code" | "claude-code-cli" => "Claude Code",
        "codex" | "openai-codex" => "Codex",
        "opencode" | "open-code" => "OpenCode",
        "cursor" | "cursor-agent" | "cursor-agent-cli" => "Cursor Agent",
        "copilot" | "github-copilot" | "copilot-cli" => "Copilot CLI",
        "gemini" | "gemini-cli" => "Gemini",
        _ if raw.trim().is_empty() => "Unknown",
        _ => raw.trim(),
    }.to_string()
}

pub fn open_terminal(session: &str, pane_id: Option<&str>) -> Result<()> {
    let executable = executable().ok_or_else(|| anyhow!("Herdr is not installed"))?;
    let mut args = if let Some(pane) = pane_id {
        vec!["--session".to_string(), session.to_string(), "agent".to_string(), "attach".to_string(), pane.to_string()]
    } else {
        vec!["session".to_string(), "attach".to_string(), session.to_string()]
    };
    let executable_text = executable.display().to_string();
    let terminals: [(&str, Vec<String>); 3] = [
        ("kgx", std::iter::once("--".to_string()).chain(std::iter::once(executable_text.clone())).chain(args.clone()).collect()),
        ("gnome-terminal", std::iter::once("--".to_string()).chain(std::iter::once(executable_text.clone())).chain(args.clone()).collect()),
        ("x-terminal-emulator", std::iter::once("-e".to_string()).chain(std::iter::once(executable_text)).chain(args.drain(..)).collect()),
    ];
    for (terminal, arguments) in terminals {
        if Command::new(terminal).args(arguments).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok() {
            return Ok(());
        }
    }
    bail!("no supported terminal application is installed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agents_from_the_public_snapshot_shape() {
        let value = serde_json::json!({"snapshot":{"workspaces":[{"workspace_id":"w1","label":"Veronica"}],"agents":[{"pane_id":"p1","workspace_id":"w1","display_agent":"codex","agent_status":"working","name":"Build UI","foreground_cwd":"/repo","focused":true}]}});
        let rows = agents_from_snapshot("default", &value);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "Codex");
        assert_eq!(rows[0].status, "working");
        assert_eq!(rows[0].workspace, "Veronica");
    }

    #[test]
    fn normalises_status_words_like_edith() {
        assert_eq!(status(Some(&Value::String("waiting_for_input".into()))), "blocked");
        assert_eq!(status(Some(&Value::String("completed".into()))), "done");
    }
}
