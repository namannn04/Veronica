//! Probing the tools in `veronica_core::tools`.
//!
//! The catalogue knows where a tool would be; this asks whether it is there
//! and answers for itself. Edith's `ExtensionLifecycleProbe.toolReadiness` does
//! the same three-way split, for the same reason: "installed" and "missing" are
//! not the whole story. A file on PATH that will not run — a broken symlink, a
//! half-finished npm install, a binary for the wrong architecture — is a third
//! state, and reporting it as missing would send the user to install something
//! they already have.
//!
//! The version string is the first non-empty line the tool prints, on stdout or
//! stderr, which is Edith's rule. `ssh -V` writes to stderr and several of the
//! others write to stdout, so reading only one of them would leave half the
//! catalogue looking broken.

use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use serde::Serialize;
use veronica_core::tools::ToolSpec;

/// How long one tool may take to say its version.
///
/// Edith allows five seconds for everything but `yt-dlp`. Nothing in this
/// catalogue does work on `--version`, so a tool that has not answered by then
/// is wedged, and waiting longer only holds up the rest of the list.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Readiness {
    Installed {
        path: String,
        version: String,
    },
    Uninstalled,
    /// Found, but it would not run. `detail` names the file, because which of
    /// several copies is being used is usually the answer.
    Error {
        detail: String,
    },
}

impl Readiness {
    pub fn is_installed(&self) -> bool {
        matches!(self, Readiness::Installed { .. })
    }

    /// One line for a table cell.
    pub fn summary(&self) -> String {
        match self {
            Readiness::Installed { version, .. } => version.clone(),
            Readiness::Uninstalled => "not installed".to_string(),
            Readiness::Error { .. } => "will not run".to_string(),
        }
    }
}

/// Where the tool is and what it says it is.
pub async fn readiness(tool: &ToolSpec) -> Readiness {
    let Some(path) = tool.locate() else {
        return Readiness::Uninstalled;
    };
    match version(&path, tool.version_args).await {
        Ok(version) => Readiness::Installed {
            path: path.display().to_string(),
            version,
        },
        Err(detail) => Readiness::Error {
            detail: format!("{} is there, but {detail}.", path.display()),
        },
    }
}

/// One tool, probed, with everything a screen or a terminal needs to say
/// about it. The CLI and the app share the shape so their answers cannot drift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub id: &'static str,
    pub display_name: &'static str,
    pub why: &'static str,
    pub instruction: &'static str,
    /// The extensions that named this tool, so a missing one points somewhere.
    pub wanted_by: Vec<&'static str>,
    #[serde(flatten)]
    pub readiness: Readiness,
}

impl Report {
    /// What to do about a tool that is not ready.
    ///
    /// A tool that is present but will not run needs the diagnosis, not the
    /// install line: telling someone to install what they already have is how
    /// a broken symlink turns into an afternoon.
    pub fn note(&self) -> String {
        match &self.readiness {
            Readiness::Error { detail } => detail.clone(),
            _ => format!("{} {}", self.why, self.instruction),
        }
    }
}

/// Every tool in the catalogue, probed together.
///
/// Concurrently, because five sequential five-second timeouts is half a minute
/// of a list that should feel instant, and the probes do not contend.
pub async fn catalogue() -> Vec<Report> {
    let probes = veronica_core::tools::CATALOG.iter().map(|tool| async move {
        Report {
            id: tool.id,
            display_name: tool.display_name,
            why: tool.why,
            instruction: tool.instruction,
            wanted_by: veronica_core::tools::wanted_by(tool.id),
            readiness: readiness(tool).await,
        }
    });
    futures_util::future::join_all(probes).await
}

/// The catalogue, plus what each extension is short of.
///
/// The rule about which tools satisfy an extension stays here rather than
/// being shipped to the interface with the raw lists: one place decides, so
/// the page, the CLI and any later readiness command cannot answer
/// differently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Survey {
    pub tools: Vec<Report>,
    /// Extension id to the tools it needs and does not have, by id. An
    /// extension that is satisfied is absent rather than present and empty.
    pub unmet: BTreeMap<&'static str, Vec<&'static str>>,
}

pub async fn survey() -> Survey {
    let tools = catalogue().await;
    let unmet = veronica_core::extensions::ENTRIES
        .iter()
        .filter_map(|entry| {
            let missing: Vec<&'static str> = unmet(entry, &tools)
                .into_iter()
                .map(|tool| tool.id)
                .collect();
            (!missing.is_empty()).then_some((entry.id, missing))
        })
        .collect();
    Survey { tools, unmet }
}

/// The tools `entry` needs but does not have, empty when it is satisfied.
///
/// Under `Any` one installed tool is enough, so nothing is named while the
/// other is missing; with none of them installed every candidate is named,
/// because any one of them would fix it.
pub fn unmet(
    entry: &veronica_core::extensions::ExtensionEntry,
    reports: &[Report],
) -> Vec<&'static ToolSpec> {
    let required = veronica_core::tools::required_by(entry);
    let installed = |tool: &ToolSpec| {
        reports
            .iter()
            .any(|report| report.id == tool.id && report.readiness.is_installed())
    };
    match veronica_core::tools::rule(entry.id) {
        veronica_core::tools::ToolRule::Any if required.iter().any(|tool| installed(tool)) => {
            Vec::new()
        }
        veronica_core::tools::ToolRule::Any => required,
        veronica_core::tools::ToolRule::All => required
            .into_iter()
            .filter(|tool| !installed(tool))
            .collect(),
    }
}

/// What `install` did, or what it left for the user to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Outcome {
    AlreadyInstalled {
        path: String,
        version: String,
    },
    Installed {
        path: String,
        version: String,
    },
    /// Veronica ran nothing. `command` is what would do it when there is one
    /// Veronica could have run, and `instruction` stands alone either way.
    NotRun {
        command: Option<String>,
        instruction: &'static str,
        reason: String,
    },
}

/// Get a tool, or say why that is not Veronica's to do.
///
/// Edith's `ed tools install`: a tool that is already there is reported, not
/// reinstalled, and nothing here can remove one. The routes differ by what
/// they need.
///
/// npm runs unprivileged and is never escalated. `npm install -g` under
/// `pkexec` would install into root's prefix, which is a worse outcome than
/// the failure it was meant to avoid, so a prefix the user cannot write to is
/// reported with the instruction rather than worked around.
///
/// apt needs root, and follows the rule the rest of Veronica follows: without
/// `--yes` the command is printed, with it the command runs through `pkexec`,
/// so the desktop's own dialog asks the user to authenticate. Veronica never
/// writes `sudo` on their behalf.
pub async fn install(tool: &ToolSpec, assume_yes: bool) -> anyhow::Result<Outcome> {
    if let Readiness::Installed { path, version } = readiness(tool).await {
        return Ok(Outcome::AlreadyInstalled { path, version });
    }

    let argv: Vec<String> = match tool.install {
        veronica_core::tools::Install::Npm { package } => {
            // Asked before running rather than diagnosed afterwards: on a
            // stock Ubuntu the global prefix is /usr/local, npm fails with a
            // forty-line EACCES trace, and its own advice is to re-run as
            // root — which would install the tool into root's prefix, where
            // the user cannot run it.
            if let Some(reason) = npm_prefix_problem().await {
                return Ok(Outcome::NotRun {
                    command: Some(format!("npm install -g {package}")),
                    instruction: tool.instruction,
                    reason,
                });
            }
            vec!["npm".into(), "install".into(), "-g".into(), package.into()]
        }
        veronica_core::tools::Install::Apt { package } => {
            crate::packages::validate_name(package)?;
            let argv = vec![
                "apt-get".to_string(),
                "install".to_string(),
                "-y".to_string(),
                package.to_string(),
            ];
            let wrapped = crate::packages::privileged_command(crate::packages::Source::Apt, &argv);
            if !assume_yes {
                return Ok(Outcome::NotRun {
                    command: Some(wrapped.join(" ")),
                    instruction: tool.instruction,
                    reason: "Installing it needs root. Run it yourself, or pass --yes to \
                             authenticate through the desktop's own dialog."
                        .to_string(),
                });
            }
            wrapped
        }
        veronica_core::tools::Install::Manual => {
            return Ok(Outcome::NotRun {
                command: None,
                instruction: tool.instruction,
                reason: format!("{} has no install Veronica can drive.", tool.display_name),
            })
        }
    };

    run_streaming(&argv).await?;

    // An install that reports success and leaves nothing on the search path is
    // the npm-prefix-not-on-PATH case, and it is worth saying plainly: the
    // package is on disk and the tool still will not run.
    match readiness(tool).await {
        Readiness::Installed { path, version } => Ok(Outcome::Installed { path, version }),
        Readiness::Error { detail } => anyhow::bail!("{detail}"),
        Readiness::Uninstalled => anyhow::bail!(
            "`{}` succeeded, but no `{}` appeared anywhere Veronica looks. Its install \
             prefix is probably not one of {}.",
            argv.join(" "),
            tool.executable,
            veronica_core::tools::search_path()
                .iter()
                .map(|directory| directory.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Why `npm install -g` would fail here, or `None` when it would work.
///
/// Writability is established by writing: a directory Veronica creates and
/// removes at once. Reading the mode bits would mean reasoning about the
/// effective uid, the group list, ACLs and the mount's read-only flag, and
/// getting any of those wrong produces the confident wrong answer that is
/// worse than no check at all.
async fn npm_prefix_problem() -> Option<String> {
    let Some(npm) = veronica_core::tools::locate_in(&veronica_core::tools::search_path(), "npm")
    else {
        return Some(
            "npm is not installed, and this tool is published as an npm package. Install \
             Node.js — `sudo apt install nodejs npm` — and try again."
                .to_string(),
        );
    };

    let output = tokio::time::timeout(
        PROBE_TIMEOUT,
        tokio::process::Command::new(&npm)
            .args(["prefix", "-g"])
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    let prefix = std::path::PathBuf::from(first_line(&output.stdout)?);

    // npm writes packages under <prefix>/lib/node_modules and links them into
    // <prefix>/bin, so the nearest directory that already exists is the one
    // whose permissions decide the outcome.
    let target = prefix.join("lib/node_modules");
    let existing = target
        .ancestors()
        .find(|directory| directory.is_dir())?
        .to_path_buf();

    let probe = existing.join(format!(".veronica-write-probe-{}", std::process::id()));
    match std::fs::create_dir(&probe) {
        Ok(()) => {
            let _ = std::fs::remove_dir(&probe);
            None
        }
        Err(_) => Some(format!(
            "npm's global prefix is {}, which you cannot write to, and Veronica will not run \
             npm as root: that installs the tool into root's prefix, where you cannot run it. \
             Point npm at a prefix you own instead — `npm config set prefix ~/.npm-global` — \
             and make sure ~/.npm-global/bin is on your PATH. Veronica already looks there.",
            existing.display()
        )),
    }
}

/// Run an install, with its output on stderr as it happens.
///
/// Not captured and not on stdout: an install can be a silent minute, and
/// stdout carries exactly one document, so the log belongs on the other
/// stream — which is where `vr` puts every log already.
async fn run_streaming(argv: &[String]) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut child = tokio::process::Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot run {}", argv[0]))?;

    let mut out = BufReader::new(child.stdout.take().expect("piped")).lines();
    let mut err = BufReader::new(child.stderr.take().expect("piped")).lines();

    // Both streams are drained together: reading one to the end first lets the
    // other fill its pipe buffer and block the installer forever. Each is
    // tracked separately and the loop ends only when both have, because a
    // finished stream answers instantly and would otherwise win every race and
    // cut the other one off mid-log.
    let (mut out_done, mut err_done) = (false, false);
    let mut tail: Vec<String> = Vec::new();
    while !out_done || !err_done {
        let line = tokio::select! {
            line = out.next_line(), if !out_done => match line? {
                Some(line) => Some(line),
                None => { out_done = true; None }
            },
            line = err.next_line(), if !err_done => match line? {
                Some(line) => Some(line),
                None => { err_done = true; None }
            },
        };
        if let Some(line) = line {
            eprintln!("{line}");
            tail.push(line);
            // Only the end of the log is worth quoting back in an error.
            if tail.len() > 10 {
                tail.remove(0);
            }
        }
    }

    let status = child
        .wait()
        .await
        .context("cannot wait for the installer")?;
    if !status.success() {
        anyhow::bail!(
            "`{}` exited {}: {}",
            argv.join(" "),
            status,
            tail.join(" | ")
        );
    }
    Ok(())
}

/// The first non-empty line the tool prints for its version arguments.
async fn version(path: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let child = tokio::process::Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("it cannot be run: {error}"))?;

    let output = match tokio::time::timeout(PROBE_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(format!("reading its output failed: {error}")),
        Err(_) => {
            return Err(format!(
                "it did not answer `{}` within {} seconds",
                args.join(" "),
                PROBE_TIMEOUT.as_secs()
            ))
        }
    };

    // A tool that exits non-zero but still printed its version has answered the
    // question; the exit status only decides the wording when nothing is there.
    match first_line(&output.stdout).or_else(|| first_line(&output.stderr)) {
        Some(line) => Ok(line),
        None if output.status.success() => {
            Err(format!("it printed nothing for `{}`", args.join(" ")))
        }
        None => Err(format!("it exited {}", output.status)),
    }
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_non_empty_line_is_the_version() {
        assert_eq!(
            first_line(b"\n\n  1.2.3 (build 9)  \nsecond line\n"),
            Some("1.2.3 (build 9)".to_string())
        );
        assert_eq!(first_line(b""), None);
        assert_eq!(first_line(b"   \n\t\n"), None);
    }

    /// `ssh -V` writes to stderr and exits non-zero on some builds. Both are
    /// normal, and neither means the tool is broken.
    #[tokio::test]
    async fn a_version_on_stderr_still_counts() {
        let (_root, script) = write_script("#!/bin/sh\necho 'OpenSSH_9.6p1' >&2\nexit 1\n");
        assert_eq!(
            version(&script, &["-V"]).await,
            Ok("OpenSSH_9.6p1".to_string())
        );
    }

    #[tokio::test]
    async fn a_tool_that_prints_nothing_is_an_error_rather_than_a_version() {
        let (_root, script) = write_script("#!/bin/sh\nexit 0\n");
        assert!(version(&script, &["--version"]).await.is_err());
    }

    #[tokio::test]
    async fn a_tool_that_hangs_is_given_up_on() {
        let (_root, script) = write_script("#!/bin/sh\nsleep 30\n");
        let started = std::time::Instant::now();
        let probed = tokio::time::timeout(
            PROBE_TIMEOUT + Duration::from_secs(5),
            version(&script, &["--version"]),
        )
        .await
        .expect("the probe has to return on its own");
        assert!(probed.is_err());
        assert!(started.elapsed() < PROBE_TIMEOUT + Duration::from_secs(2));
    }

    #[tokio::test]
    async fn a_tool_that_is_not_there_is_uninstalled_rather_than_broken() {
        let absent = ToolSpec {
            id: "nothing-is-called-this",
            display_name: "Nothing",
            why: "It does not exist.",
            executable: "veronica-no-such-tool",
            version_args: &["--version"],
            install: veronica_core::tools::Install::Manual,
            instruction: "There is nothing to install.",
        };
        assert_eq!(readiness(&absent).await, Readiness::Uninstalled);
    }

    fn report(id: &'static str, installed: bool) -> Report {
        let tool = veronica_core::tools::spec(id).unwrap();
        Report {
            id: tool.id,
            display_name: tool.display_name,
            why: tool.why,
            instruction: tool.instruction,
            wanted_by: veronica_core::tools::wanted_by(tool.id),
            readiness: if installed {
                Readiness::Installed {
                    path: format!("/usr/bin/{id}"),
                    version: "1.0".to_string(),
                }
            } else {
                Readiness::Uninstalled
            },
        }
    }

    fn entry(id: &str) -> &'static veronica_core::extensions::ExtensionEntry {
        veronica_core::extensions::ENTRIES
            .iter()
            .find(|entry| entry.id == id)
            .unwrap()
    }

    fn ids(tools: Vec<&'static ToolSpec>) -> Vec<&'static str> {
        tools.into_iter().map(|tool| tool.id).collect()
    }

    /// Agent Usage is Edith's one exception: Claude and Codex report the same
    /// kind of numbers, so either alone is a working page.
    #[test]
    fn agent_usage_is_satisfied_by_either_provider() {
        let reports = vec![report("claude", true), report("codex", false)];
        assert!(unmet(entry("usage"), &reports).is_empty());
    }

    #[test]
    fn agent_usage_with_no_provider_names_both() {
        let reports = vec![report("claude", false), report("codex", false)];
        assert_eq!(
            ids(unmet(entry("usage"), &reports)),
            vec!["claude", "codex"]
        );
    }

    /// Everything else needs what it declares. This is the case the Extensions
    /// page got wrong: Herdr reported Ready with no herdr on the machine.
    #[test]
    fn herdr_without_herdr_is_unmet() {
        let reports = vec![report("herdr", false)];
        assert_eq!(ids(unmet(entry("herdr"), &reports)), vec!["herdr"]);
    }

    #[test]
    fn an_extension_that_needs_no_tool_is_never_unmet() {
        assert!(unmet(entry("emoji"), &[]).is_empty());
    }

    /// Whatever this machine has installed, the map has to be readable by id
    /// on both sides: a key nothing matches, or an empty list, would show the
    /// page a warning it cannot word.
    #[tokio::test]
    async fn a_survey_only_names_real_extensions_and_real_shortfalls() {
        let survey = survey().await;
        assert_eq!(survey.tools.len(), veronica_core::tools::CATALOG.len());
        for (extension, missing) in &survey.unmet {
            assert!(
                veronica_core::extensions::ENTRIES
                    .iter()
                    .any(|entry| entry.id == *extension),
                "'{extension}' is not an extension"
            );
            assert!(!missing.is_empty(), "'{extension}' is short of nothing");
            for id in missing {
                assert!(veronica_core::tools::spec(id).is_some());
            }
        }
    }

    /// A tool that is not on this machine under any name, so the install
    /// routes are reached rather than short-circuited by what happens to be
    /// installed on the machine running the tests.
    fn absent(install: veronica_core::tools::Install) -> ToolSpec {
        ToolSpec {
            id: "absent",
            display_name: "Absent",
            why: "Nothing needs it.",
            executable: "veronica-no-such-tool",
            version_args: &["--version"],
            install,
            instruction: "Install it by hand.",
        }
    }

    /// Nothing Veronica can drive is still an answer, not a failure.
    #[tokio::test]
    async fn a_manual_route_runs_nothing_and_hands_back_the_instruction() {
        let outcome = install(&absent(veronica_core::tools::Install::Manual), true)
            .await
            .unwrap();
        match outcome {
            Outcome::NotRun {
                command,
                instruction,
                ..
            } => {
                assert_eq!(command, None);
                assert_eq!(instruction, "Install it by hand.");
            }
            other => panic!("expected NotRun, got {other:?}"),
        }
    }

    /// The rule the rest of Veronica follows: a change that needs root is
    /// printed unless the user asked for the authentication dialog. Whatever
    /// is printed is what would run, never a `sudo` invented for the message.
    #[tokio::test]
    async fn an_apt_route_without_yes_prints_the_command_it_would_run() {
        let tool = absent(veronica_core::tools::Install::Apt {
            package: "openssh-client",
        });
        match install(&tool, false).await.unwrap() {
            Outcome::NotRun { command, .. } => {
                let command = command.expect("apt has a command Veronica could run");
                assert!(command.contains("apt-get install -y openssh-client"));
                assert!(!command.contains("sudo"));
                assert_eq!(
                    command,
                    crate::packages::privileged_command(
                        crate::packages::Source::Apt,
                        &[
                            "apt-get".into(),
                            "install".into(),
                            "-y".into(),
                            "openssh-client".into()
                        ]
                    )
                    .join(" ")
                );
            }
            other => panic!("expected NotRun, got {other:?}"),
        }
    }

    /// Both streams have to be read to the end, and this script proves it:
    /// stdout closes a second before stderr says anything. A loop that stops
    /// at the first exhausted stream is finished by then, and loses the error
    /// — which is the line that mattered.
    #[tokio::test]
    async fn a_failed_install_quotes_the_end_of_both_its_streams() {
        let (_root, script) = write_script(
            "#!/bin/sh\necho 'to stdout'\nexec 1>&-\nsleep 1\necho 'to stderr' >&2\nexit 3\n",
        );
        let error = run_streaming(&[script.display().to_string()])
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("to stdout"), "{error}");
        assert!(error.contains("to stderr"), "{error}");
    }

    /// An installer noisier than a pipe buffer must not wedge. npm is, on a
    /// cold cache.
    #[tokio::test]
    async fn an_installer_louder_than_a_pipe_buffer_still_finishes() {
        let (_root, script) = write_script(
            "#!/bin/sh\ni=0\nwhile [ $i -lt 1200 ]; do\n  \
             echo \"out $i ----------------------------------------------------------------\"\n  \
             echo \"err $i ----------------------------------------------------------------\" >&2\n  \
             i=$((i+1))\ndone\n",
        );
        let ran = tokio::time::timeout(
            Duration::from_secs(30),
            run_streaming(&[script.display().to_string()]),
        )
        .await
        .expect("a full pipe buffer must not deadlock");
        assert!(ran.is_ok());
    }

    /// The directory is returned alongside the path because dropping it
    /// deletes the script, and the write handle is closed before the script is
    /// run: an open writer makes the exec fail with "Text file busy".
    fn write_script(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("tool");
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (root, path)
    }
}
