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
