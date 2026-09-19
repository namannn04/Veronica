//! App Maintenance: what is installed, what can be updated, and removing it
//! with the consequences shown first.
//!
//! Ported from Edith's `appMaintenance`, which unifies Homebrew casks and
//! formulae, the Mac App Store through `mas`, Sparkle feeds and plain HTTPS
//! app feeds into one inventory. Ubuntu's equivalent triple is apt, snap and
//! flatpak, and the same rules carry over: discovery never installs anything,
//! every write is previewed before it runs, and a removal shows what else goes
//! with it before anything is removed.
//!
//! Privilege is the one thing that genuinely differs. macOS lets an app move a
//! bundle to the Trash with the user's own permissions; apt and snap need root.
//! Veronica never escalates on its own and never wraps a command in `sudo`
//! behind the user's back: reads run unprivileged, and a write is either run
//! through `pkexec` — which shows the desktop's own authentication dialog, so
//! it is the user authenticating rather than Veronica escalating — or printed
//! for the user to run themselves. `--yes` chooses the first; without it the
//! command is only printed.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use tokio::process::Command;

/// Where a package came from, which decides how it updates and whether the
/// update needs root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Apt,
    Snap,
    /// A user-scope flatpak, which is the default and needs no root.
    Flatpak,
}

impl Source {
    pub const ALL: [Source; 3] = [Source::Apt, Source::Snap, Source::Flatpak];

    pub fn title(self) -> &'static str {
        match self {
            Source::Apt => "apt",
            Source::Snap => "snap",
            Source::Flatpak => "flatpak",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Source::ALL
            .into_iter()
            .find(|source| source.title().eq_ignore_ascii_case(raw))
    }

    /// The program that answers for this source.
    fn program(self) -> &'static str {
        match self {
            Source::Apt => "apt",
            Source::Snap => "snap",
            Source::Flatpak => "flatpak",
        }
    }

    /// Whether changing anything here needs root.
    ///
    /// A user-scope flatpak does not, which is why it is the one source whose
    /// updates can run with no authentication dialog at all.
    pub fn needs_root(self) -> bool {
        !matches!(self, Source::Flatpak)
    }
}

/// One package that has a newer version available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub source: Source,
    pub name: String,
    /// What is installed now. `None` where the source does not report it.
    pub installed_version: Option<String>,
    pub available_version: String,
    /// Where it comes from: an apt suite, a snap channel, a flatpak remote.
    pub origin: Option<String>,
}

/// One installed package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub source: Source,
    pub name: String,
    pub version: String,
    /// Disk footprint in bytes, where the source reports one.
    pub size_bytes: Option<u64>,
}

/// What a removal would do, from the package manager's own simulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalPlan {
    pub source: Source,
    pub package: String,
    /// Everything that would go, including the package itself. Edith's
    /// review-first rule: the consequences are shown before anything happens.
    pub removed: Vec<String>,
    /// Packages that would be left behind as no-longer-needed. Not removed by
    /// this operation, but worth knowing about.
    pub now_unused: Vec<String>,
    /// The exact command, so the user can run it themselves instead.
    pub command: String,
    /// Whether this would take out more than the package that was asked for.
    /// The one fact that decides whether a removal is routine or worth a hard
    /// look, so it is carried in the document rather than left for each caller
    /// to re-derive and get subtly different.
    pub removes_more_than_asked: bool,
}

impl RemovalPlan {
    fn takes_more_than(package: &str, removed: &[String]) -> bool {
        removed
            .iter()
            .any(|name| !name.eq_ignore_ascii_case(package))
    }
}

async fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .await
        .with_context(|| format!("{program} is not installed"))?;
    if !output.status.success() {
        bail!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Whether a source is usable on this machine.
pub async fn available(source: Source) -> bool {
    which(source.program()).is_some()
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| std::path::Path::new(dir).join(program))
        .find(|path| path.is_file())
}

/// Parse `apt list --upgradable`.
///
/// Each line is `name/suite new-version arch [upgradable from: old-version]`.
/// The header and any progress noise apt writes are skipped rather than parsed
/// into a package called "Listing...".
pub fn parse_apt_updates(output: &str) -> Vec<Update> {
    let parsed: Vec<(Update, String)> = output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (name, rest) = line.split_once('/')?;
            if name.is_empty() || name.contains(char::is_whitespace) {
                return None;
            }
            let mut fields = rest.split_whitespace();
            let suite = fields.next()?;
            let available = fields.next()?;
            let architecture = fields.next()?;
            let installed = line
                .split_once("[upgradable from:")
                .map(|(_, tail)| tail.trim_end_matches(']').trim().to_string());
            Some((
                Update {
                    source: Source::Apt,
                    name: name.to_string(),
                    installed_version: installed,
                    available_version: available.to_string(),
                    origin: Some(suite.to_string()),
                },
                architecture.to_string(),
            ))
        })
        .collect();

    // `apt list` emits one row per architecture, but the package field omits
    // that architecture. Without restoring it, a multiarch install produces
    // duplicate rows with duplicate UI keys and an ambiguous update command.
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for (update, _) in &parsed {
        *counts.entry(update.name.clone()).or_default() += 1;
    }
    let mut seen = std::collections::HashSet::new();
    parsed
        .into_iter()
        .filter_map(|(mut update, architecture)| {
            if counts.get(&update.name).copied().unwrap_or_default() > 1 {
                update.name = format!("{}:{architecture}", update.name);
            }
            seen.insert(update.name.clone()).then_some(update)
        })
        .collect()
}

/// Parse `snap refresh --list`.
///
/// A table with a header row, and the words "All snaps up to date." when there
/// is nothing — which is not a package and must not become one.
pub fn parse_snap_updates(output: &str) -> Vec<Update> {
    output
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Name"))
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let version = fields.next()?;
            Some(Update {
                source: Source::Snap,
                name: name.to_string(),
                // snap's refresh list reports what you would get, not what you
                // have; claiming otherwise would put the wrong version on the
                // left of the arrow.
                installed_version: None,
                available_version: version.to_string(),
                origin: fields.nth(2).map(str::to_string),
            })
        })
        .collect()
}

/// Parse `flatpak remote-ls --updates --columns=application,version,origin`,
/// which is tab separated with no header.
pub fn parse_flatpak_updates(output: &str) -> Vec<Update> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t').map(str::trim);
            let name = fields.next().filter(|name| !name.is_empty())?;
            Some(Update {
                source: Source::Flatpak,
                name: name.to_string(),
                installed_version: None,
                available_version: fields.next().unwrap_or_default().to_string(),
                origin: fields.next().map(str::to_string),
            })
        })
        .collect()
}

/// Every update available, from every source this machine has.
///
/// A source that is missing or that fails contributes nothing rather than
/// failing the whole discovery: a machine with no flatpak still has apt
/// updates worth showing. Discovery never installs anything.
pub async fn updates() -> Vec<Update> {
    let mut all = Vec::new();

    if available(Source::Apt).await {
        // `apt list` warns that its interface is unstable when not a tty; the
        // format has been stable for a decade and the warning goes to stderr.
        match run("apt", &["list", "--upgradable"]).await {
            Ok(output) => all.extend(parse_apt_updates(&output)),
            Err(error) => tracing::debug!("apt updates unavailable: {error:#}"),
        }
    }
    if available(Source::Snap).await {
        match run("snap", &["refresh", "--list"]).await {
            Ok(output) => all.extend(parse_snap_updates(&output)),
            Err(error) => tracing::debug!("snap updates unavailable: {error:#}"),
        }
    }
    if available(Source::Flatpak).await {
        match run(
            "flatpak",
            &[
                "remote-ls",
                "--updates",
                "--columns=application,version,origin",
            ],
        )
        .await
        {
            Ok(output) => all.extend(parse_flatpak_updates(&output)),
            Err(error) => tracing::debug!("flatpak updates unavailable: {error:#}"),
        }
    }

    all.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    all
}

/// Parse `dpkg-query -W -f='${Package}\t${Version}\t${Installed-Size}\n'`.
///
/// `Installed-Size` is in kibibytes, which is a footgun worth converting once
/// here rather than at every call site.
pub fn parse_apt_installed(output: &str, manual: &[String]) -> Vec<Installed> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next()?.trim();
            if name.is_empty() {
                return None;
            }
            // Only what the user asked for. The three thousand packages pulled
            // in as dependencies are not applications anybody installed.
            if !manual.iter().any(|wanted| wanted == name) {
                return None;
            }
            let version = fields.next().unwrap_or_default().trim().to_string();
            Some(Installed {
                source: Source::Apt,
                name: name.to_string(),
                version,
                size_bytes: fields
                    .next()
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .map(|kib| kib.saturating_mul(1024)),
            })
        })
        .collect()
}

/// Parse `snap list`, whose Size column is absent and whose header is one row.
pub fn parse_snap_installed(output: &str) -> Vec<Installed> {
    output
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Name"))
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let version = fields.next()?;
            Some(Installed {
                source: Source::Snap,
                name: name.to_string(),
                version: version.to_string(),
                size_bytes: None,
            })
        })
        .collect()
}

/// Parse `flatpak list --app --columns=application,version`.
pub fn parse_flatpak_installed(output: &str) -> Vec<Installed> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t').map(str::trim);
            let name = fields.next().filter(|name| !name.is_empty())?;
            Some(Installed {
                source: Source::Flatpak,
                name: name.to_string(),
                version: fields.next().unwrap_or_default().to_string(),
                size_bytes: None,
            })
        })
        .collect()
}

/// What is installed, across every source.
///
/// For apt this is what the user asked for — `apt-mark showmanual` — not every
/// package on the system. Listing three thousand dependencies would be a
/// package database, not an inventory of applications.
pub async fn inventory() -> Vec<Installed> {
    let mut all = Vec::new();

    if available(Source::Apt).await {
        let manual: Vec<String> = run("apt-mark", &["showmanual"])
            .await
            .unwrap_or_default()
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        if !manual.is_empty() {
            match run(
                "dpkg-query",
                &["-W", "-f=${Package}\t${Version}\t${Installed-Size}\n"],
            )
            .await
            {
                Ok(output) => all.extend(parse_apt_installed(&output, &manual)),
                Err(error) => tracing::debug!("dpkg inventory unavailable: {error:#}"),
            }
        }
    }
    if available(Source::Snap).await {
        if let Ok(output) = run("snap", &["list"]).await {
            all.extend(parse_snap_installed(&output));
        }
    }
    if available(Source::Flatpak).await {
        if let Ok(output) = run(
            "flatpak",
            &["list", "--app", "--columns=application,version"],
        )
        .await
        {
            all.extend(parse_flatpak_installed(&output));
        }
    }

    all.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    all
}

/// Pull the package lists out of `apt-get -s remove`'s simulation.
///
/// The simulation is the package manager's own answer to "what would this do",
/// which is the only trustworthy source for it: guessing from a dependency
/// graph would eventually disagree with what actually happens.
pub fn parse_removal_plan(output: &str, package: &str) -> (Vec<String>, Vec<String>) {
    let mut removed = Vec::new();
    let mut unused = Vec::new();
    let mut section: Option<&str> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("The following packages will be REMOVED")
            || trimmed.starts_with("The following package will be REMOVED")
        {
            section = Some("removed");
            continue;
        }
        if trimmed.starts_with("The following package was automatically installed")
            || trimmed.starts_with("The following packages were automatically installed")
        {
            section = Some("unused");
            continue;
        }
        // A continuation line is indented; anything flush left ends the list.
        if !line.starts_with(' ') {
            section = None;
            continue;
        }
        let names = trimmed.split_whitespace().map(str::to_string);
        match section {
            Some("removed") => removed.extend(names),
            Some("unused") => unused.extend(names),
            _ => {}
        }
    }

    // apt says nothing about a package that is not installed; reporting the
    // named package as removable anyway would be a plan that does nothing.
    if removed.is_empty() && output.contains(&format!("Remv {package}")) {
        removed.push(package.to_string());
    }
    (removed, unused)
}

/// Ask apt what removing `package` would do. Changes nothing.
pub async fn removal_plan(package: &str) -> Result<RemovalPlan> {
    validate_name(package)?;
    let output = run("apt-get", &["-s", "remove", package]).await?;
    let (removed, now_unused) = parse_removal_plan(&output, package);
    if removed.is_empty() {
        bail!("apt would remove nothing for {package:?}; is it installed?");
    }
    Ok(RemovalPlan {
        source: Source::Apt,
        removes_more_than_asked: RemovalPlan::takes_more_than(package, &removed),
        package: package.to_string(),
        removed,
        now_unused,
        command: removal_command(package, false)?.join(" "),
    })
}

/// The argv used to remove one apt package.
///
/// Keeping the preview and execution routes on one builder prevents the UI
/// from advertising `sudo` while the real operation uses the desktop's
/// authentication dialog through `pkexec`.
pub fn removal_command(package: &str, assume_yes: bool) -> Result<Vec<String>> {
    validate_name(package)?;
    let mut argv = vec!["apt-get".to_string(), "remove".to_string()];
    if assume_yes {
        argv.push("-y".to_string());
    }
    argv.push(package.to_string());
    Ok(privileged_command(Source::Apt, &argv))
}

/// A package name Veronica is willing to put on a command line.
///
/// Everything here is passed as a separate argv entry, never through a shell,
/// so this is belt and braces — but a name is the one field that comes from
/// outside, and Debian's own policy is narrower than this already.
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 200 {
        bail!("not a package name: {name:?}");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || ".+-_:@/".contains(c))
    {
        bail!("not a package name: {name:?}");
    }
    Ok(())
}

/// The argv that applies an update for one source.
///
/// Returned rather than run, so the caller can print it, and so this is
/// testable without changing the machine.
pub fn update_command(source: Source, names: &[String]) -> Result<Vec<String>> {
    for name in names {
        validate_name(name)?;
    }
    Ok(match source {
        Source::Apt if names.is_empty() => {
            vec!["apt-get".into(), "upgrade".into(), "-y".into()]
        }
        Source::Apt => {
            // `install` rather than `upgrade` is how apt updates a chosen
            // subset; `upgrade <name>` is not a thing.
            let mut argv = vec![
                "apt-get".into(),
                "install".into(),
                "-y".into(),
                "--only-upgrade".into(),
            ];
            argv.extend(names.iter().cloned());
            argv
        }
        Source::Snap if names.is_empty() => vec!["snap".into(), "refresh".into()],
        Source::Snap => {
            let mut argv = vec!["snap".into(), "refresh".into()];
            argv.extend(names.iter().cloned());
            argv
        }
        Source::Flatpak if names.is_empty() => {
            vec!["flatpak".into(), "update".into(), "-y".into()]
        }
        Source::Flatpak => {
            let mut argv = vec!["flatpak".into(), "update".into(), "-y".into()];
            argv.extend(names.iter().cloned());
            argv
        }
    })
}

/// Wrap an argv in `pkexec` when the source needs root and we are not root.
///
/// `pkexec` shows the desktop's own authentication dialog, so the user
/// authenticates rather than Veronica escalating. Already being root — someone
/// running `sudo vr` — needs no wrapper.
pub fn privileged_command(source: Source, argv: &[String]) -> Vec<String> {
    if !source.needs_root() || is_root() {
        return argv.to_vec();
    }
    let mut wrapped = vec!["pkexec".to_string()];
    wrapped.extend(argv.iter().cloned());
    wrapped
}

fn is_root() -> bool {
    // Reading the effective uid without libc: /proc/self/status is where the
    // kernel publishes it, and this crate already reads procfs everywhere else.
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("Uid:"))?
                .split_whitespace()
                .nth(2)?
                .parse::<u32>()
                .ok()
        })
        .map(|euid| euid == 0)
        .unwrap_or(false)
}

/// What running an update produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    pub source: Source,
    pub names: Vec<String>,
    pub command: String,
    pub succeeded: bool,
    /// The tail of the output, which is what says why when it failed.
    pub output: String,
}

/// Apply an update. The caller is responsible for having confirmed it.
pub async fn apply_update(source: Source, names: &[String]) -> Result<UpdateResult> {
    if !available(source).await {
        bail!("{} is not installed on this computer", source.title());
    }
    let argv = privileged_command(source, &update_command(source, names)?);
    let output = Command::new(&argv[0])
        .args(&argv[1..])
        .env("DEBIAN_FRONTEND", "noninteractive")
        .output()
        .await
        .with_context(|| format!("cannot run {}", argv.join(" ")))?;

    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    // The tail is what carries the error; the head is a wall of progress.
    let tail: String = text
        .lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    Ok(UpdateResult {
        source,
        names: names.to_vec(),
        command: argv.join(" "),
        succeeded: output.status.success(),
        output: tail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APT: &str = "\
Listing...
alsa-ucm-conf/resolute-updates,resolute-updates 1.2.15.3-1ubuntu1.5 all [upgradable from: 1.2.15.3-1ubuntu1.4]
apparmor/resolute-updates 5.0.2-0ubuntu1~26.04.1 amd64 [upgradable from: 5.0.0~beta1-0ubuntu7]
";

    const SNAP: &str = "\
Name         Version  Rev   Size   Publisher    Notes
firefox      155.0-1  8819  274MB  mozilla**    -
thunderbird  155.0-1  1240  232MB  canonical**  -
";

    #[test]
    fn apt_updates_carry_both_versions_and_the_suite() {
        let updates = parse_apt_updates(APT);
        assert_eq!(updates.len(), 2, "the Listing... header is not a package");
        assert_eq!(updates[0].name, "alsa-ucm-conf");
        assert_eq!(updates[0].available_version, "1.2.15.3-1ubuntu1.5");
        assert_eq!(
            updates[0].installed_version.as_deref(),
            Some("1.2.15.3-1ubuntu1.4")
        );
        assert_eq!(
            updates[0].origin.as_deref(),
            Some("resolute-updates,resolute-updates")
        );
    }

    #[test]
    fn multiarch_updates_have_distinct_actionable_names() {
        let updates = parse_apt_updates(
            "libgpu/stable 2.0 amd64 [upgradable from: 1.0]\n\
             libgpu/stable 2.0 i386 [upgradable from: 1.0]\n",
        );
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "libgpu:amd64");
        assert_eq!(updates[1].name, "libgpu:i386");
    }

    #[test]
    fn a_line_that_is_not_a_package_is_skipped() {
        // apt writes progress and warnings on the same stream in some configs.
        let updates = parse_apt_updates("Listing...\nW: Some warning / with a slash\n");
        assert!(updates.is_empty(), "got {updates:#?}");
    }

    #[test]
    fn snap_updates_skip_the_header_row() {
        let updates = parse_snap_updates(SNAP);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "firefox");
        assert_eq!(updates[0].available_version, "155.0-1");
        assert_eq!(updates[0].source, Source::Snap);
    }

    #[test]
    fn snap_does_not_claim_to_know_the_installed_version() {
        // `snap refresh --list` reports what you would get, not what you have.
        assert!(parse_snap_updates(SNAP)[0].installed_version.is_none());
    }

    #[test]
    fn nothing_to_refresh_produces_no_packages() {
        assert!(parse_snap_updates("All snaps up to date.\n").is_empty());
        assert!(parse_snap_updates("").is_empty());
    }

    #[test]
    fn flatpak_updates_are_tab_separated_with_no_header() {
        let updates = parse_flatpak_updates("org.gnome.Boxes\t46.0\tflathub\n\n");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "org.gnome.Boxes");
        assert_eq!(updates[0].origin.as_deref(), Some("flathub"));
    }

    #[test]
    fn the_inventory_lists_what_was_asked_for_not_every_dependency() {
        let manual = vec!["firefox".to_string()];
        let installed =
            parse_apt_installed("firefox\t1:1snap1\t1024\nlibc6\t2.39\t12000\n", &manual);
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].name, "firefox");
        // dpkg reports kibibytes, which is a footgun worth converting once.
        assert_eq!(installed[0].size_bytes, Some(1024 * 1024));
    }

    #[test]
    fn a_removal_plan_lists_what_goes_and_what_is_left_unused() {
        let output = "\
Solving dependencies...
The following package was automatically installed and is no longer required:
  nvidia-firmware-580
Use 'apt autoremove' to remove it.
The following packages will be REMOVED:
  firefox firefox-locale-en
0 upgraded, 0 newly installed, 2 to remove and 91 not upgraded.
Remv firefox [1:1snap1-0ubuntu8]
";
        let (removed, unused) = parse_removal_plan(output, "firefox");
        assert_eq!(removed, ["firefox", "firefox-locale-en"]);
        assert_eq!(unused, ["nvidia-firmware-580"]);
    }

    #[test]
    fn a_plan_that_takes_more_than_asked_says_so() {
        assert!(RemovalPlan::takes_more_than(
            "firefox",
            &["firefox".into(), "firefox-locale-en".into()]
        ));
        assert!(!RemovalPlan::takes_more_than(
            "firefox",
            &["firefox".into()]
        ));
        // A dependency going with it is exactly the case worth flagging, even
        // when the named package is not itself in the list.
        assert!(RemovalPlan::takes_more_than("firefox", &["libnss3".into()]));
    }

    #[test]
    fn a_package_name_from_outside_is_validated() {
        for good in ["firefox", "libc6", "g++", "python3.12", "gcc-13:amd64"] {
            assert!(validate_name(good).is_ok(), "{good} should be allowed");
        }
        for bad in ["", "a; rm -rf /", "$(whoami)", "one two", "back`tick`"] {
            assert!(validate_name(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn updating_everything_and_updating_a_subset_use_different_apt_verbs() {
        // `apt-get upgrade <name>` is not a thing; a subset needs `install
        // --only-upgrade`, and getting this wrong upgrades the whole system.
        assert_eq!(
            update_command(Source::Apt, &[]).unwrap(),
            ["apt-get", "upgrade", "-y"]
        );
        assert_eq!(
            update_command(Source::Apt, &["firefox".into()]).unwrap(),
            ["apt-get", "install", "-y", "--only-upgrade", "firefox"]
        );
    }

    #[test]
    fn snap_and_flatpak_take_their_own_verbs() {
        assert_eq!(
            update_command(Source::Snap, &[]).unwrap(),
            ["snap", "refresh"]
        );
        assert_eq!(
            update_command(Source::Snap, &["firefox".into()]).unwrap(),
            ["snap", "refresh", "firefox"]
        );
        assert_eq!(
            update_command(Source::Flatpak, &[]).unwrap(),
            ["flatpak", "update", "-y"]
        );
    }

    #[test]
    fn a_bad_name_never_reaches_a_command_line() {
        assert!(update_command(Source::Apt, &["a; rm -rf /".into()]).is_err());
        assert!(removal_command("a; rm -rf /", true).is_err());
    }

    #[test]
    fn a_removal_command_uses_pkexec_not_sudo() {
        let argv = removal_command("firefox", false).unwrap();
        assert!(!argv.iter().any(|part| part == "sudo"));
        assert_eq!(argv.iter().filter(|part| *part == "apt-get").count(), 1);
        assert_eq!(argv.last().map(String::as_str), Some("firefox"));
        if !is_root() {
            assert_eq!(argv.first().map(String::as_str), Some("pkexec"));
        }
    }

    #[test]
    fn a_root_needing_source_is_wrapped_in_pkexec_and_flatpak_is_not() {
        // pkexec shows the desktop's own dialog: the user authenticates, rather
        // than Veronica escalating on their behalf.
        let argv = vec!["apt-get".to_string(), "upgrade".to_string()];
        let wrapped = privileged_command(Source::Apt, &argv);
        if is_root() {
            assert_eq!(wrapped, argv, "already root needs no wrapper");
        } else {
            assert_eq!(wrapped[0], "pkexec");
            assert_eq!(&wrapped[1..], &argv[..]);
        }
        assert_eq!(
            privileged_command(Source::Flatpak, &["flatpak".into(), "update".into()]),
            ["flatpak", "update"],
            "a user-scope flatpak needs no authentication at all"
        );
    }

    #[test]
    fn sources_round_trip_through_their_names() {
        for source in Source::ALL {
            assert_eq!(Source::parse(source.title()), Some(source));
        }
        assert_eq!(Source::parse("APT"), Some(Source::Apt));
        assert_eq!(Source::parse("nix"), None);
    }
}
