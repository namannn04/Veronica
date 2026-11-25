use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use serde::Serialize;
use veronica_core::{CompanionItem, CompanionRepository};

struct Recording {
    child: Child,
    path: PathBuf,
    started: Instant,
}

static ACTIVE: Mutex<Option<Recording>> = Mutex::new(None);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    recording: bool,
    elapsed_seconds: u64,
}

fn active() -> std::sync::MutexGuard<'static, Option<Recording>> {
    ACTIVE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn status() -> Status {
    let active = active();
    Status {
        recording: active.is_some(),
        elapsed_seconds: active
            .as_ref()
            .map_or(0, |recording| recording.started.elapsed().as_secs()),
    }
}

pub(crate) fn start(repository: &CompanionRepository) -> Result<()> {
    let mut active = active();
    anyhow::ensure!(active.is_none(), "a voice memo is already recording");
    let directory = repository.recordings_dir();
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!(
        "voice-{}-{}.wav",
        chrono::Utc::now().timestamp_millis(),
        std::process::id()
    ));
    let mut child = veronica_core::host::command("pw-record")
        .args([
            "--media-category",
            "Capture",
            "--media-role",
            "Communication",
            "--rate",
            "48000",
            "--channels",
            "1",
        ])
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot start PipeWire recorder")?;
    std::thread::sleep(Duration::from_millis(120));
    if let Some(status) = child.try_wait()? {
        let _ = std::fs::remove_file(&path);
        anyhow::bail!("PipeWire recorder exited immediately ({status})");
    }
    *active = Some(Recording {
        child,
        path,
        started: Instant::now(),
    });
    Ok(())
}

fn halt(recording: &mut Recording) {
    let _ = std::process::Command::new("kill")
        .args(["-INT", &recording.child.id().to_string()])
        .status();
    for _ in 0..20 {
        if recording.child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = recording.child.kill();
    let _ = recording.child.wait();
}

pub(crate) fn stop(repository: &CompanionRepository) -> Result<Option<CompanionItem>> {
    let Some(mut recording) = active().take() else {
        return Ok(None);
    };
    halt(&mut recording);
    let bytes = std::fs::metadata(&recording.path).map_or(0, |metadata| metadata.len());
    if bytes <= 44 {
        let _ = std::fs::remove_file(&recording.path);
        anyhow::bail!("the microphone produced no audio; check PipeWire input access");
    }
    repository
        .add_voice(
            &recording.path,
            recording.started.elapsed().as_secs().max(1),
            chrono::Utc::now(),
        )
        .map(Some)
}

pub(crate) fn cancel() {
    if let Some(mut recording) = active().take() {
        halt(&mut recording);
        let _ = std::fs::remove_file(recording.path);
    }
}
