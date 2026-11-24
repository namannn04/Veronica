use std::io::Write;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use zbus::Connection;

pub const SHELL_ACTION_BUS: &str = "io.github.namannn04.Veronica.Shell";
pub const SHELL_ACTION_PATH: &str = "/io/github/namannn04/Veronica/Shell";
pub const SHELL_ACTION_INTERFACE: &str = "io.github.namannn04.Veronica.ShellActions";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer {
    ShellExtension,
    WlCopy,
    Xclip,
    Xsel,
}

impl Writer {
    pub fn title(self) -> &'static str {
        match self {
            Writer::ShellExtension => "the Veronica GNOME extension",
            Writer::WlCopy => "wl-copy",
            Writer::Xclip => "xclip",
            Writer::Xsel => "xsel",
        }
    }
}

pub async fn write(text: &str) -> Result<Writer> {
    let mut attempts: Vec<String> = Vec::new();

    match write_with_shell(text).await {
        Ok(()) => return Ok(Writer::ShellExtension),
        Err(error) => attempts.push(format!("{}: {error:#}", Writer::ShellExtension.title())),
    }

    for (writer, program, args) in COMMAND_WRITERS {
        match write_with_command(program, args, text) {
            Ok(()) => return Ok(*writer),
            Err(error) => attempts.push(format!("{program}: {error:#}")),
        }
    }

    bail!(
        "cannot put text on the clipboard. Enable Veronica's GNOME extension, or \
         install wl-clipboard (Wayland) or xclip (X11). Tried: {}",
        attempts.join("; ")
    )
}

pub async fn paste_in_place() -> Result<()> {
    paste_with_method("PasteInPlace").await
}

pub async fn paste_from_picker() -> Result<()> {
    paste_with_method("PasteEmoji").await
}

async fn paste_with_method(method: &str) -> Result<()> {
    let connection = Connection::session().await.context("no session bus")?;
    connection
        .call_method(
            Some(SHELL_ACTION_BUS),
            SHELL_ACTION_PATH,
            Some(SHELL_ACTION_INTERFACE),
            method,
            &(),
        )
        .await
        .context(
            "pasting in place needs Veronica's GNOME Shell extension, which is what \
             synthesises the key press inside the compositor",
        )?;
    Ok(())
}

const COMMAND_WRITERS: &[(Writer, &str, &[&str])] = &[
    (Writer::WlCopy, "wl-copy", &["--type", "text/plain"]),
    (Writer::Xclip, "xclip", &["-selection", "clipboard"]),
    (Writer::Xsel, "xsel", &["--clipboard", "--input"]),
];

async fn write_with_shell(text: &str) -> Result<()> {
    let connection = Connection::session().await.context("no session bus")?;
    connection
        .call_method(
            Some(SHELL_ACTION_BUS),
            SHELL_ACTION_PATH,
            Some(SHELL_ACTION_INTERFACE),
            "WriteClipboard",
            &(text,),
        )
        .await
        .context("the extension is not running, or is an older version")?;
    Ok(())
}

fn write_with_command(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = veronica_core::host::command(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("cannot run {program}"))?;
    child
        .stdin
        .take()
        .context("no stdin")?
        .write_all(text.as_bytes())
        .with_context(|| format!("{program} refused the text"))?;

    let status = child
        .wait()
        .with_context(|| format!("{program} did not finish"))?;
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}

pub fn available_command_writers() -> Vec<Writer> {
    COMMAND_WRITERS
        .iter()
        .filter(|(_, program, _)| which(program).is_some())
        .map(|(writer, _, _)| *writer)
        .collect()
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    veronica_core::host::which(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn every_command_writer_is_named_and_distinct() {
        let names: Vec<&str> = COMMAND_WRITERS
            .iter()
            .map(|(writer, _, _)| writer.title())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "duplicate writer in {names:?}");
        assert!(!names.is_empty());
    }

    #[test]
    fn wayland_is_tried_before_x11_because_that_is_the_ubuntu_default() {
        let order: Vec<&str> = COMMAND_WRITERS.iter().map(|(_, p, _)| *p).collect();
        assert_eq!(order, vec!["wl-copy", "xclip", "xsel"]);
    }

    #[test]
    fn a_missing_tool_reports_rather_than_panicking() {
        let error = write_with_command("veronica-no-such-tool", &[], "hi").unwrap_err();
        assert!(
            format!("{error:#}").contains("veronica-no-such-tool"),
            "the message should name the tool: {error:#}"
        );
    }

    #[test]
    fn a_tool_that_rejects_the_text_is_a_failure_not_a_success() {
        if which("false").is_some() {
            assert!(write_with_command("false", &[], "hi").is_err());
        }
    }

    #[test]
    fn text_reaches_a_tool_intact_including_newlines_and_quotes() {
        let Some(_) = which("cat") else { return };
        let awkward = "line one\n\"quoted\"\t$HOME 'single' \\ ünïcøde";
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(awkward.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap(), awkward);
    }

    #[test]
    fn which_finds_a_real_program_and_not_a_made_up_one() {
        assert!(which("sh").is_some(), "sh should be on PATH");
        assert!(which("veronica-definitely-absent").is_none());
    }
}
