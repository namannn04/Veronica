use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(12);

const DEFAULT_SESSION: &str = "default";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HerdrBoard {
    pub installed: bool,
    pub executable: Option<String>,
    pub sessions: Vec<HerdrSession>,
    pub agents: Vec<HerdrAgent>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HerdrSession {
    #[serde(alias = "session", alias = "id", alias = "session_name")]
    pub name: String,
    #[serde(default = "running_by_default", alias = "alive", alias = "active")]
    pub running: bool,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub session_dir: String,
    #[serde(default, alias = "socket", alias = "path")]
    pub socket_path: String,
    #[serde(default, skip_deserializing)]
    pub error: Option<String>,
}

fn running_by_default() -> bool {
    true
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
    pub attach_command: String,
}

#[derive(Deserialize)]
struct SessionDocument {
    sessions: Vec<HerdrSession>,
}

pub fn executable() -> Option<PathBuf> {
    veronica_core::tools::spec("herdr")?.locate()
}

pub async fn board() -> Result<HerdrBoard> {
    let Some(executable) = executable() else {
        return Ok(HerdrBoard {
            installed: false,
            executable: None,
            sessions: Vec::new(),
            agents: Vec::new(),
            error: None,
        });
    };

    let (mut sessions, mut error) = match sessions(&executable).await {
        Ok(sessions) => (sessions, None),
        Err(reason) => (Vec::new(), Some(reason.to_string())),
    };
    let listed = !sessions.is_empty();
    if !listed {
        sessions.push(HerdrSession {
            name: DEFAULT_SESSION.to_string(),
            running: true,
            default: true,
            session_dir: String::new(),
            socket_path: String::new(),
            error: None,
        });
    }

    let mut agents = Vec::new();
    for session in sessions.iter_mut().filter(|session| session.running) {
        match snapshot(&executable, &session.name).await {
            Ok(mut rows) => agents.append(&mut rows),
            Err(reason) => session.error = Some(reason.to_string()),
        }
    }

    if !listed && agents.is_empty() {
        error = error.or_else(|| sessions.first().and_then(|session| session.error.clone()));
        sessions.clear();
    }

    Ok(HerdrBoard {
        installed: true,
        executable: Some(executable.display().to_string()),
        sessions,
        agents,
        error,
    })
}

async fn sessions(executable: &Path) -> Result<Vec<HerdrSession>> {
    let output = run(executable, &["session", "list", "--json"]).await?;
    let document: SessionDocument =
        serde_json::from_slice(&output).context("Herdr returned an invalid session list")?;
    Ok(document.sessions)
}

async fn snapshot(executable: &Path, session: &str) -> Result<Vec<HerdrAgent>> {
    let output = run(executable, &["--session", session, "api", "snapshot"])
        .await
        .with_context(|| format!("cannot read Herdr session {session}"))?;
    let value: Value =
        serde_json::from_slice(&output).context("Herdr returned an invalid snapshot")?;
    if has_snapshot(&value) {
        return Ok(agents_from_snapshot(session, &value));
    }
    let output = run(executable, &["--session", session, "agent", "list"])
        .await
        .with_context(|| format!("cannot list agents in Herdr session {session}"))?;
    let value: Value =
        serde_json::from_slice(&output).context("Herdr returned an invalid agent list")?;
    Ok(agents_from_snapshot(session, &value))
}

fn has_snapshot(value: &Value) -> bool {
    let payload = unwrap(value);
    payload.get("snapshot").is_some_and(Value::is_object)
        || payload.get("type").and_then(Value::as_str) == Some("session_snapshot")
}

async fn run(executable: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let child = tokio::process::Command::from(veronica_core::host::command(executable))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("cannot run {}", executable.display()))?;
    let output = match tokio::time::timeout(COMMAND_TIMEOUT, child.wait_with_output()).await {
        Ok(output) => output.context("cannot read what Herdr printed")?,
        Err(_) => bail!(
            "Herdr did not answer within {} seconds",
            COMMAND_TIMEOUT.as_secs()
        ),
    };
    if !output.status.success() {
        let message = message(&output.stderr)
            .or_else(|| message(&output.stdout))
            .unwrap_or_else(|| format!("herdr exited {}", output.status));
        bail!("{message}");
    }
    Ok(output.stdout)
}

fn message(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes).trim().to_string();
    if text.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<Value>(&text) {
        let nested = value
            .get("error")
            .and_then(|error| error.get("message"))
            .or_else(|| value.get("error"))
            .or_else(|| value.get("message"))
            .and_then(Value::as_str);
        if let Some(nested) = nested {
            return Some(nested.to_string());
        }
    }
    Some(text)
}

#[derive(Debug, Clone, Default)]
struct PaneRecord {
    pane: String,
    kind: Option<String>,
    status: Option<String>,
    title: Option<String>,
    workspace_id: Option<String>,
    cwd: Option<String>,
    focused: bool,
}

impl PaneRecord {
    fn looks_like_agent(&self) -> bool {
        if self.kind.is_some() {
            return true;
        }
        self.status
            .as_deref()
            .is_some_and(|status| self::status(status) != "unknown")
    }

    fn merging(&self, incoming: &PaneRecord) -> PaneRecord {
        let status = match (&incoming.status, &self.status) {
            (Some(incoming), Some(existing))
                if self::status(incoming) == "unknown" && self::status(existing) != "unknown" =>
            {
                Some(existing.clone())
            }
            (Some(incoming), _) => Some(incoming.clone()),
            (None, existing) => existing.clone(),
        };
        PaneRecord {
            pane: if incoming.pane.is_empty() {
                self.pane.clone()
            } else {
                incoming.pane.clone()
            },
            kind: incoming.kind.clone().or_else(|| self.kind.clone()),
            status,
            title: incoming.title.clone().or_else(|| self.title.clone()),
            workspace_id: incoming
                .workspace_id
                .clone()
                .or_else(|| self.workspace_id.clone()),
            cwd: incoming.cwd.clone().or_else(|| self.cwd.clone()),
            focused: incoming.focused || self.focused,
        }
    }
}

fn agents_from_snapshot(session: &str, value: &Value) -> Vec<HerdrAgent> {
    let payload = unwrap(value);
    let snapshot = payload.get("snapshot").unwrap_or(payload);
    let labels = workspace_labels(snapshot);

    let mut records: BTreeMap<String, PaneRecord> = BTreeMap::new();
    for record in list(snapshot, "panes")
        .filter_map(pane_record)
        .filter(PaneRecord::looks_like_agent)
    {
        records.insert(record.pane.clone(), record);
    }
    for record in list(snapshot, "agents").filter_map(pane_record) {
        let merged = match records.get(&record.pane) {
            Some(existing) => existing.merging(&record),
            None => record,
        };
        records.insert(merged.pane.clone(), merged);
    }

    records
        .into_values()
        .map(|record| agent(session, &record, &labels))
        .collect()
}

fn unwrap(value: &Value) -> &Value {
    match value.get("result").or_else(|| value.get("data")) {
        Some(inner) if inner.is_object() => unwrap(inner),
        _ => value,
    }
}

fn list<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn workspace_labels(snapshot: &Value) -> BTreeMap<String, String> {
    list(snapshot, "workspaces")
        .filter_map(|workspace| {
            let id = text(workspace, &["workspace_id", "id"])?;
            let label = text(workspace, &["label", "name"]).unwrap_or_else(|| id.clone());
            Some((id, label))
        })
        .collect()
}

fn pane_record(value: &Value) -> Option<PaneRecord> {
    let pane = text(value, &["pane_id", "pane", "id", "terminal_id", "target"])?;
    Some(PaneRecord {
        pane,
        kind: text(value, &["display_agent", "agent", "kind"]),
        status: text(value, &["agent_status", "status", "state", "agentStatus"]),
        title: text(
            value,
            &[
                "name",
                "title",
                "terminal_title_stripped",
                "terminal_title",
                "summary",
            ],
        ),
        workspace_id: text(value, &["workspace_id", "workspace"]),
        cwd: text(value, &["foreground_cwd", "cwd", "working_directory"]),
        focused: value
            .get("focused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn text(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(key))
        .filter_map(Value::as_str)
        .map(str::trim)
        .find(|text| !text.is_empty())
        .map(str::to_string)
}

fn agent(session: &str, record: &PaneRecord, labels: &BTreeMap<String, String>) -> HerdrAgent {
    let kind = record
        .kind
        .as_deref()
        .map(display_kind)
        .unwrap_or_else(|| "Unknown".to_string());
    let workspace = record
        .workspace_id
        .as_ref()
        .map(|id| labels.get(id).cloned().unwrap_or_else(|| id.clone()))
        .unwrap_or_default();
    let title = record
        .title
        .clone()
        .filter(|title| title != &record.pane)
        .unwrap_or_else(|| kind.clone());
    HerdrAgent {
        id: format!("{session}|{}", record.pane),
        session: session.to_string(),
        kind,
        status: record
            .status
            .as_deref()
            .map(status)
            .unwrap_or_else(|| "unknown".to_string()),
        title,
        workspace,
        cwd: record.cwd.clone().unwrap_or_default(),
        attach_command: attach_command(session, Some(&record.pane)),
        pane_id: record.pane.clone(),
        focused: record.focused,
    }
}

fn status(raw: &str) -> String {
    let normalised = raw.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    match normalised.as_str() {
        "blocked" | "needs_attention" | "waiting" | "waiting_for_input" | "approval" => "blocked",
        "working" | "running" | "busy" | "in_progress" | "thinking" => "working",
        "done" | "complete" | "completed" | "finished" | "success" => "done",
        "idle" | "ready" | "stopped" => "idle",
        _ => "unknown",
    }
    .to_string()
}

fn display_kind(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "Unknown".to_string();
    }
    match trimmed.to_ascii_lowercase().replace(' ', "-").as_str() {
        "claude" | "claude-code" | "claude-code-cli" => "Claude Code",
        "codex" | "openai-codex" => "Codex",
        "opencode" | "open-code" | "opencode2" => "OpenCode",
        "cursor" | "cursor-agent" | "cursor-agent-cli" | "cursor-cli" => "Cursor Agent",
        "copilot" | "github-copilot" | "github-copilot-cli" | "copilot-cli" | "ghcs" => {
            "Copilot CLI"
        }
        "pi" | "py" | "pi-coding-agent" => "Pi",
        "gemini" | "gemini-cli" => "Gemini",
        "grok" | "grok-build" | "grok-cli" => "Grok",
        "cline" => "Cline",
        "fx" | "fx.sh" | "fx-sh" => "FX.sh",
        "devin" | "devin-cli" => "Devin",
        "agy" | "antigravity" | "antigravity-cli" => "Antigravity",
        "amp" | "amp-local" => "Amp",
        "droid" | "factory-droid" => "Droid",
        "kimi" | "kimi-code" => "Kimi",
        "kilo" | "kilo-code" => "Kilo",
        "qwen" | "qwen-code" => "Qwen",
        "hermes" | "hermes-agent" => "Hermes",
        "kiro" | "kiro-cli" => "Kiro",
        "qodercli" | "qoder" | "qoderclicn" => "Qoder",
        "omp" => "OMP",
        "mastracode" | "mastra-code" => "Mastra Code",
        "maki" => "Maki",
        _ => trimmed,
    }
    .to_string()
}

pub fn attach_args(session: &str, pane_id: Option<&str>) -> Vec<String> {
    match pane_id {
        Some(pane) => vec![
            "--session".to_string(),
            session.to_string(),
            "agent".to_string(),
            "attach".to_string(),
            pane.to_string(),
            "--takeover".to_string(),
        ],
        None => vec![
            "session".to_string(),
            "attach".to_string(),
            session.to_string(),
        ],
    }
}

pub fn attach_command(session: &str, pane_id: Option<&str>) -> String {
    std::iter::once("herdr".to_string())
        .chain(attach_args(session, pane_id))
        .map(|word| shell_quote(&word))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:=@+,".contains(character));
    if plain {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

pub fn open_terminal(session: &str, pane_id: Option<&str>) -> Result<()> {
    let executable = executable().ok_or_else(|| anyhow!("Herdr is not installed"))?;
    let mut args = attach_args(session, pane_id);
    let executable_text = executable.display().to_string();
    let terminals: [(&str, Vec<String>); 3] = [
        (
            "kgx",
            std::iter::once("--".to_string())
                .chain(std::iter::once(executable_text.clone()))
                .chain(args.clone())
                .collect(),
        ),
        (
            "gnome-terminal",
            std::iter::once("--".to_string())
                .chain(std::iter::once(executable_text.clone()))
                .chain(args.clone())
                .collect(),
        ),
        (
            "x-terminal-emulator",
            std::iter::once("-e".to_string())
                .chain(std::iter::once(executable_text))
                .chain(args.drain(..))
                .collect(),
        ),
    ];
    for (terminal, arguments) in terminals {
        if veronica_core::host::spawn_detached(
            veronica_core::host::command(terminal)
                .args(arguments)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
        .is_ok()
        {
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
    fn reads_the_snapshot_through_the_cli_envelope() {
        let value = serde_json::json!({
            "id": "cli:api:snapshot",
            "result": {"type": "session_snapshot", "snapshot": {
                "workspaces": [{"workspace_id": "w3", "label": "~"}],
                "panes": [{"pane_id": "w3:p1", "workspace_id": "w3", "agent": "codex", "display_agent": "codex", "agent_status": "working", "title": "Build the board", "foreground_cwd": "/home/x"}],
                "agents": [{"pane_id": "w3:p1", "workspace_id": "w3", "agent": "codex", "display_agent": "codex", "agent_status": "working", "title": "Build the board", "foreground_cwd": "/home/x", "focused": true}],
            }},
        });
        let rows = agents_from_snapshot("default", &value);
        assert_eq!(rows.len(), 1, "the same pane in both arrays is one agent");
        assert_eq!(rows[0].title, "Build the board");
        assert_eq!(rows[0].workspace, "~");
        assert!(rows[0].focused);
    }

    #[test]
    fn keeps_an_agent_that_only_the_pane_list_still_describes() {
        let value = serde_json::json!({"result": {"snapshot": {
            "panes": [{"pane_id": "w3:p1", "display_agent": "opencode", "agent_status": "unknown", "title": "Ship the port"}],
            "agents": [],
        }}});
        let rows = agents_from_snapshot("default", &value);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "OpenCode");
        assert_eq!(rows[0].status, "unknown");
        assert_eq!(rows[0].title, "Ship the port");
    }

    #[test]
    fn leaves_plain_shells_off_the_board() {
        let value = serde_json::json!({"result": {"snapshot": {
            "panes": [{"pane_id": "w3:p1", "agent_status": "unknown", "terminal_title_stripped": "namannn04@box: ~"}],
            "agents": [],
        }}});
        assert!(agents_from_snapshot("default", &value).is_empty());
    }

    #[test]
    fn merges_without_losing_the_live_state() {
        let value = serde_json::json!({"result": {"snapshot": {
            "panes": [{"pane_id": "p1", "display_agent": "claude", "agent_status": "blocked", "foreground_cwd": "/repo"}],
            "agents": [{"pane_id": "p1", "agent_status": "unknown"}],
        }}});
        let rows = agents_from_snapshot("default", &value);
        assert_eq!(rows[0].status, "blocked");
        assert_eq!(rows[0].kind, "Claude Code");
        assert_eq!(rows[0].cwd, "/repo");
    }

    #[test]
    fn reads_the_agent_list_shape_too() {
        let value = serde_json::json!({"result": {"agents": [{"pane_id": "p2", "agent": "grok", "agent_status": "done", "title": "Rebase"}], "type": "agent_list"}});
        assert!(!has_snapshot(&value), "an agent list is not a snapshot");
        let rows = agents_from_snapshot("default", &value);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "Grok");
        assert_eq!(rows[0].status, "done");
        assert_eq!(
            rows[0].attach_command,
            "herdr --session default agent attach p2 --takeover"
        );
    }

    #[test]
    fn normalises_status_words_into_board_states() {
        assert_eq!(status("waiting_for_input"), "blocked");
        assert_eq!(status("needs-attention"), "blocked");
        assert_eq!(status("In Progress"), "working");
        assert_eq!(status("completed"), "done");
        assert_eq!(status("whatever-herdr-adds-next"), "unknown");
    }

    #[test]
    fn names_every_agent_herdr_integrates() {
        for (raw, expected) in [
            ("claude", "Claude Code"),
            ("codex", "Codex"),
            ("opencode", "OpenCode"),
            ("cursor", "Cursor Agent"),
            ("copilot", "Copilot CLI"),
            ("pi", "Pi"),
            ("omp", "OMP"),
            ("devin", "Devin"),
            ("droid", "Droid"),
            ("kimi", "Kimi"),
            ("kilo", "Kilo"),
            ("hermes", "Hermes"),
            ("qodercli", "Qoder"),
            ("qwen", "Qwen"),
            ("mastracode", "Mastra Code"),
            ("antigravity-cli", "Antigravity"),
            ("grok", "Grok"),
            ("gemini", "Gemini"),
        ] {
            assert_eq!(display_kind(raw), expected, "{raw}");
        }
        assert_eq!(display_kind("some-new-agent"), "some-new-agent");
        assert_eq!(display_kind("  "), "Unknown");
    }

    #[test]
    fn takes_a_pane_over_from_another_client() {
        assert_eq!(
            attach_args("default", Some("w3:p1")),
            [
                "--session",
                "default",
                "agent",
                "attach",
                "w3:p1",
                "--takeover"
            ]
        );
        assert_eq!(
            attach_args("default", None),
            ["session", "attach", "default"]
        );
    }

    #[test]
    fn quotes_the_attach_line_for_a_shell() {
        assert_eq!(
            attach_command("default", Some("w3:p1")),
            "herdr --session default agent attach w3:p1 --takeover"
        );
        assert_eq!(
            attach_command("my session", None),
            "herdr session attach 'my session'"
        );
    }

    #[test]
    fn reports_an_error_document_rather_than_the_raw_json() {
        assert_eq!(
            message(br#"{"error":{"message":"no such session"}}"#).as_deref(),
            Some("no such session")
        );
        assert_eq!(
            message(b"  herdr: broken pipe  ").as_deref(),
            Some("herdr: broken pipe")
        );
        assert_eq!(message(b"   "), None);
    }

    #[test]
    fn reads_the_session_list_herdr_prints() {
        let document: SessionDocument = serde_json::from_str(
            r#"{"sessions":[{"default":true,"name":"default","running":true,"session_dir":"/c/herdr","socket_path":"/c/herdr/herdr.sock"}]}"#,
        )
        .expect("the shape herdr 0.8 prints");
        assert_eq!(document.sessions[0].name, "default");
        assert!(document.sessions[0].running);

        let sparse: SessionDocument = serde_json::from_str(r#"{"sessions":[{"name":"work"}]}"#)
            .expect("a session with only a name");
        assert!(
            sparse.sessions[0].running,
            "a session Herdr lists is assumed live"
        );
        assert!(sparse.sessions[0].session_dir.is_empty());
    }
}
