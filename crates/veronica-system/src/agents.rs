//! User-owned interactive agent terminals. External processes are observations only.
use anyhow::{ensure, Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Instant,
};
const OUTPUT_LIMIT: usize = 256 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentView {
    pub id: String,
    pub name: String,
    pub pid: u32,
    pub cwd: String,
    pub status: String,
    pub owned: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutput {
    pub base: u64,
    pub cursor: u64,
    pub bytes: String,
    pub status: String,
}
#[derive(Default)]
struct Output {
    bytes: VecDeque<u8>,
    end: u64,
}
struct Terminal {
    view: AgentView,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    input: mpsc::SyncSender<Vec<u8>>,
    output: Arc<Mutex<Output>>,
}
#[derive(Default)]
pub struct AgentManager {
    terminals: Mutex<HashMap<String, Terminal>>,
    accepting: AtomicBool,
    epoch: AtomicU64,
}
impl AgentManager {
    pub fn open(&self) {
        if !self.accepting.swap(true, Ordering::SeqCst) {
            self.epoch.fetch_add(1, Ordering::SeqCst);
        }
    }
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }
    pub fn start(&self, profile: AgentProfile, cwd: String, epoch: u64) -> Result<AgentView> {
        ensure!(
            profile.name.len() <= 80 && !profile.name.is_empty(),
            "Choose a provider name"
        );
        ensure!(
            profile.arguments.len() <= 32
                && profile
                    .arguments
                    .iter()
                    .all(|a| a.len() <= 2048 && !a.contains('\0')),
            "Too many or oversized arguments"
        );
        let cwd = PathBuf::from(cwd)
            .canonicalize()
            .context("Choose an existing project directory")?;
        ensure!(cwd.is_dir(), "Project must be a directory");
        let binary = resolve(&profile.executable).context(
            "Agent executable is not installed; configure its path or install the CLI first",
        )?;
        let mut terminals = self
            .terminals
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal lock failed"))?;
        ensure!(
            self.accepting.load(Ordering::SeqCst) && self.epoch() == epoch,
            "Agent window has closed; reopen it before starting a terminal"
        );
        ensure!(std::env::var_os("FLATPAK_ID").is_none(),"Managed host terminals require the Debian or AppImage installation; external Herdr viewing remains available in Flatpak");
        ensure!(
            terminals.len() < 4,
            "Close an existing terminal first (maximum four)"
        );
        let pair = native_pty_system().openpty(PtySize {
            rows: 28,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut command = CommandBuilder::new(binary);
        command.args(profile.arguments);
        command.cwd(&cwd);
        command.env("TERM", "xterm-256color");
        command.env("VERONICA_AGENT_ID", &id);
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let pid = child
            .process_id()
            .context("Cannot identify owned terminal")?;
        let view = AgentView {
            id: id.clone(),
            name: profile.name,
            pid,
            cwd: cwd.display().to_string(),
            status: "running".into(),
            owned: true,
        };
        let output = Arc::new(Mutex::new(Output::default()));
        let target = output.clone();
        std::thread::spawn(move || {
            let mut bytes = [0u8; 8192];
            while let Ok(n) = reader.read(&mut bytes) {
                if n == 0 {
                    break;
                }
                let Ok(mut out) = target.lock() else { break };
                out.bytes.extend(&bytes[..n]);
                out.end += n as u64;
                while out.bytes.len() > OUTPUT_LIMIT {
                    out.bytes.pop_front();
                }
            }
        });
        let (input, receive) = mpsc::sync_channel::<Vec<u8>>(16);
        std::thread::spawn(move || {
            while let Ok(bytes) = receive.recv() {
                if writer.write_all(&bytes).is_err() {
                    break;
                }
            }
        });
        terminals.insert(
            id,
            Terminal {
                view: view.clone(),
                child,
                master: pair.master,
                input,
                output,
            },
        );
        Ok(view)
    }
    pub fn list(&self) -> Vec<AgentView> {
        self.terminals
            .lock()
            .map(|mut all| {
                all.values_mut()
                    .map(|t| {
                        let mut v = t.view.clone();
                        if let Ok(Some(status)) = t.child.try_wait() {
                            v.status = format!("exited ({})", status.exit_code())
                        }
                        v
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn output(&self, id: &str, cursor: u64) -> Result<TerminalOutput> {
        let mut all = self
            .terminals
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal lock failed"))?;
        let t = all.get_mut(id).context("Terminal has closed")?;
        let status = if let Some(status) = t.child.try_wait()? {
            format!("exited ({})", status.exit_code())
        } else {
            "running".into()
        };
        let out = t
            .output
            .lock()
            .map_err(|_| anyhow::anyhow!("Output lock failed"))?;
        let base = out.end - out.bytes.len() as u64;
        let start = cursor.max(base).min(out.end);
        let bytes: Vec<u8> = out
            .bytes
            .iter()
            .skip((start - base) as usize)
            .copied()
            .collect();
        use base64::Engine;
        Ok(TerminalOutput {
            base,
            cursor: out.end,
            bytes: base64::engine::general_purpose::STANDARD.encode(bytes),
            status,
        })
    }
    pub fn input(&self, id: &str, bytes: String) -> Result<()> {
        ensure!(bytes.len() <= 8192, "Terminal input exceeds 8 KiB");
        let all = self
            .terminals
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal lock failed"))?;
        let t = all.get(id).context("Terminal has closed")?;
        t.input
            .try_send(bytes.into_bytes())
            .context("Terminal input queue is full or closed")
    }
    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        ensure!(
            (10..=300).contains(&cols) && (2..=120).contains(&rows),
            "Invalid terminal dimensions"
        );
        let all = self
            .terminals
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal lock failed"))?;
        all.get(id)
            .context("Terminal has closed")?
            .master
            .resize(PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            })?;
        Ok(())
    }
    pub fn stop(&self, id: &str) -> Result<()> {
        if let Some(mut t) = self
            .terminals
            .lock()
            .map_err(|_| anyhow::anyhow!("Terminal lock failed"))?
            .remove(id)
        {
            terminate(&mut t)
        }
        Ok(())
    }
    pub fn stop_all(&self) {
        self.accepting.store(false, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut all) = self.terminals.lock() {
            for (_, mut t) in all.drain() {
                terminate(&mut t)
            }
        }
    }
}
impl Drop for AgentManager {
    fn drop(&mut self) {
        self.stop_all()
    }
}
fn terminate(t: &mut Terminal) {
    // Job control can put foreground commands in separate process groups within
    // our PTY session. Stop those groups too, never another user's session.
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.take(8192).flatten() {
            if let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) {
                if let Some((_, tail)) = stat.rsplit_once(") ") {
                    let fields = tail.split_whitespace().collect::<Vec<_>>();
                    if fields.get(3).and_then(|v| v.parse::<u32>().ok()) == Some(t.view.pid) {
                        if let Some(group) = fields.get(2).and_then(|v| v.parse::<i32>().ok()) {
                            if group > 1 {
                                unsafe {
                                    libc::kill(-group, libc::SIGKILL);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // portable-pty creates a new session/process group for each child; never accept arbitrary PIDs.
    unsafe {
        libc::kill(-(t.view.pid as i32), libc::SIGKILL);
    }
    let _ = t.child.kill();
    let _ = t.child.wait();
}
pub(crate) fn resolve(value: &str) -> Option<PathBuf> {
    if value.is_empty() || value.len() > 4096 || value.starts_with('-') {
        return None;
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return path.is_file().then_some(path);
    }
    if value.contains('/') {
        return None;
    }
    let mut dirs =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
    if let Some(home) = std::env::var_os("HOME") {
        for d in [".local/bin", ".cargo/bin"] {
            dirs.push(PathBuf::from(&home).join(d))
        }
    }
    dirs.into_iter()
        .map(|d| d.join(value))
        .find(|p| p.is_file())
}
pub fn defaults() -> Vec<AgentProfile> {
    [
        ("claude", "Claude Code", "claude"),
        ("codex", "Codex", "codex"),
    ]
    .into_iter()
    .map(|(id, name, exe)| AgentProfile {
        id: id.into(),
        name: name.into(),
        executable: resolve(exe)
            .map(|p| p.display().to_string())
            .unwrap_or(exe.into()),
        arguments: if id == "codex" {
            vec!["--no-daemon".into()]
        } else {
            Vec::new()
        },
    })
    .collect()
}
pub fn external() -> Vec<AgentView> {
    use std::os::unix::fs::MetadataExt;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let uid = unsafe { libc::geteuid() };
    let mut result = Vec::new();
    let started = Instant::now();
    for entry in entries.take(8192).flatten() {
        if result.len() >= 64 || started.elapsed().as_millis() > 300 {
            break;
        }
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if entry.metadata().map(|m| m.uid() != uid).unwrap_or(true) {
            continue;
        }
        let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        let comm = comm.trim();
        let name = match comm {
            "claude" => "Claude Code",
            "codex" => "Codex",
            "opencode" => "OpenCode",
            "aider" => "Aider",
            _ => continue,
        };
        result.push(AgentView {
            id: format!("external-{pid}"),
            name: name.into(),
            pid,
            cwd: std::fs::read_link(entry.path().join("cwd"))
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            status: "running · activity unknown".into(),
            owned: false,
        });
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_pty_accepts_input_and_closes_owned_process() {
        let manager = AgentManager::default();
        let root = tempfile::tempdir().unwrap();
        let p = AgentProfile {
            id: "test".into(),
            name: "Test shell".into(),
            executable: "/bin/sh".into(),
            arguments: vec!["-i".into()],
        };
        manager.open();
        let view = manager
            .start(
                p.clone(),
                root.path().display().to_string(),
                manager.epoch(),
            )
            .unwrap();
        manager
            .input(&view.id, "printf 'PTY_OK\\n'\r".into())
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let out = manager.output(&view.id, 0).unwrap();
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(out.bytes)
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("PTY_OK"));
        manager.resize(&view.id, 80, 24).unwrap();
        manager.stop_all();
        assert!(manager.list().is_empty());
        assert!(manager
            .start(p, root.path().display().to_string(), 0)
            .is_err());
        assert!(manager.input(&view.id, "bad".into()).is_err());
        assert!(!PathBuf::from(format!("/proc/{}", view.pid)).exists());
    }
    #[test]
    fn no_external_pid_control_or_shell_interpolation() {
        let manager = AgentManager::default();
        assert!(manager.stop("external-1").is_ok());
        assert!(manager.input("external-1", "x".into()).is_err());
        assert!(resolve("sh;touch /tmp/bad").is_none());
    }
}
