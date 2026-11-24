use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;
const SCRIPT: &str = include_str!("../../../integrations/agents/claude-status.py");
const EVENTS: [&str; 7] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "SessionEnd",
];
pub fn configure(home: &Path, root: &Path, enabled: bool) -> Result<()> {
    let path = home.join(".claude/settings.json");
    let original: Value = crate::private_store::load(&path)?;
    let mut document = if original.is_null() {
        json!({})
    } else {
        original.clone()
    };
    let object = document
        .as_object_mut()
        .context("Claude settings must be a JSON object")?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("Claude hooks must be an object")?;
    let script = root.join("status.py");
    let command = format!(
        "python3 '{}'",
        script.display().to_string().replace('\'', "'\\''")
    );
    for event in EVENTS {
        let entries = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .context("Claude hook event must be an array")?;
        entries.retain(|entry| {
            !entry
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|items| {
                    items.len() == 1 && items[0]["command"].as_str() == Some(&command)
                })
        });
        if enabled {
            entries.push(json!({"hooks":[{"type":"command","command":command,"timeout":3}]}));
        }
    }
    if enabled {
        std::fs::create_dir_all(root)?;
        ensure!(
            !std::fs::symlink_metadata(root)?.file_type().is_symlink(),
            "Agent directory must not be a symlink"
        );
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(script)?;
        f.write_all(SCRIPT.as_bytes())?;
    }
    let current: Value = crate::private_store::load(&path)?;
    ensure!(
        current == original,
        "Claude settings changed; retry the connection"
    );
    crate::private_store::save(&path, &document)
}
pub fn status(root: &Path, pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields = stat
        .rsplit_once(") ")?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    let start = fields.get(19)?;
    let value: Value =
        crate::private_store::load(&root.join("events").join(format!("{pid}-{start}.json")))
            .ok()?;
    let status = value["status"].as_str()?;
    ["working", "blocked", "done", "idle"]
        .contains(&status)
        .then(|| status.to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hook_connection_preserves_other_settings_and_disconnects_only_itself() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let root = temp.path().join("agents");
        let path = home.join(".claude/settings.json");
        let value = json!({"env":{"SAMPLE":"preserve"},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo existing"}]}]}});
        crate::private_store::save(&path, &value).unwrap();
        configure(&home, &root, true).unwrap();
        configure(&home, &root, true).unwrap();
        let connected: Value = crate::private_store::load(&path).unwrap();
        assert_eq!(connected["hooks"]["Stop"].as_array().unwrap().len(), 2);
        configure(&home, &root, false).unwrap();
        let disconnected: Value = crate::private_store::load(&path).unwrap();
        assert_eq!(disconnected["env"], value["env"]);
        assert_eq!(disconnected["hooks"]["Stop"], value["hooks"]["Stop"]);
    }
}
