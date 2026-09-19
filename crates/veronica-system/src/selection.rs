//! Putting text on the clipboard.
//!
//! Reading the clipboard needs the compositor, which is why the history is
//! captured by the shell extension. Writing has the same constraint on Wayland:
//! `St.Clipboard.set_text` inside the shell is the only route that does not
//! need a focused window.
//!
//! So this tries, in order:
//!
//! 1. Veronica's own GNOME Shell extension, over the same D-Bus bridge that
//!    already carries Clean Keys. Works on Wayland, needs nothing installed.
//! 2. `wl-copy`, from wl-clipboard, for a Wayland session without the
//!    extension.
//! 3. `xclip`, then `xsel`, for X11.
//!
//! Every route is reported by name on failure, so "cannot copy" always says
//! which of them were tried and what to install.
//!
//! Pasting *in place* — putting the text into the window you were typing in,
//! rather than only on the clipboard — has the same shape: synthesising input
//! into a window Veronica does not own is the compositor's job. Edith posts a
//! ⌘V through a `CGEvent`; here the shell extension sends one Ctrl+V through a
//! virtual keyboard on the seat, which needs no per-session portal approval.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use zbus::Connection;

pub const SHELL_ACTION_BUS: &str = "io.github.namannn04.Veronica.Shell";
pub const SHELL_ACTION_PATH: &str = "/io/github/namannn04/Veronica/Shell";
pub const SHELL_ACTION_INTERFACE: &str = "io.github.namannn04.Veronica.ShellActions";

/// Which route put the text on the clipboard.
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

/// Copy `text`, returning which route succeeded.
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
         install wl-clipboard (Wayland) or xclip (X11). Tried — {}",
        attempts.join("; ")
    )
}

/// Paste what is on the clipboard into the focused window.
///
/// The caller puts the text on the clipboard first; this is only the keystroke.
/// Splitting it that way means a refused paste still leaves the text somewhere
/// the user can reach, which is the difference between a degraded feature and a
/// lost one.
pub async fn paste_in_place() -> Result<()> {
    let connection = Connection::session().await.context("no session bus")?;
    connection
        .call_method(
            Some(SHELL_ACTION_BUS),
            SHELL_ACTION_PATH,
            Some(SHELL_ACTION_INTERFACE),
            "PasteInPlace",
            &(),
        )
        .await
        .context(
            "pasting in place needs Veronica's GNOME Shell extension, which is what \
             synthesises the key press inside the compositor",
        )?;
    Ok(())
}

/// The external tools, in the order they are tried.
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

/// Run one of the command-line tools, handing the text over on stdin so no
/// shell quoting is involved and the content can be anything at all.
fn write_with_command(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(program)
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

    // wl-copy forks a daemon to own the selection and exits; the others exit
    // once they have read stdin. Either way the exit status is meaningful.
    let status = child
        .wait()
        .with_context(|| format!("{program} did not finish"))?;
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}

/// Whether any route looks available, for the diagnostics page. Cheap: it only
/// looks for the executables and does not touch the bus.
pub fn available_command_writers() -> Vec<Writer> {
    COMMAND_WRITERS
        .iter()
        .filter(|(_, program, _)| which(program).is_some())
        .map(|(writer, _, _)| *writer)
        .collect()
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // `false` reads stdin happily and exits non-zero.
        if which("false").is_some() {
            assert!(write_with_command("false", &[], "hi").is_err());
        }
    }

    #[test]
    fn text_reaches_a_tool_intact_including_newlines_and_quotes() {
        // `cat` stands in for a clipboard tool: if the bytes survive the pipe to
        // it, quoting is not involved anywhere.
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
