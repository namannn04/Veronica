use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use super::legacy::SNAP_LAUNCHER;
use super::CommandResult;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub package_url: Option<String>,
    pub published_at: Option<String>,
    pub notes: String,
    pub install_kind: String,
    pub can_install: bool,
}

pub(super) fn install_kind() -> &'static str {
    veronica_core::packaging::Packaging::detect().as_str()
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    html_url: String,
    published_at: Option<String>,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn package_asset<'a>(assets: &'a [GithubAsset], kind: &str) -> Option<&'a GithubAsset> {
    let suffix = match kind {
        "appimage" => ".AppImage",
        "flatpak" => ".flatpak",
        "snap" => ".snap",
        _ => "_amd64.deb",
    };
    assets.iter().find(|asset| asset.name.ends_with(suffix))
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .trim_start_matches(['v', 'V'])
        .split('.')
        .map(|part| {
            part.split(['-', '+'])
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0)
        })
        .collect()
}

fn app_version(tag: &str) -> Option<Vec<u64>> {
    let parts: Vec<&str> = tag.strip_prefix('v')?.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    parts.iter().map(|part| part.parse().ok()).collect()
}

fn app_release(releases: Vec<GithubRelease>) -> Option<GithubRelease> {
    releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| app_version(&release.tag_name).map(|version| (version, release)))
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, release)| release)
}

fn newer_version(latest: &str, current: &str) -> bool {
    let mut latest = version_parts(latest);
    let mut current = version_parts(current);
    let width = latest.len().max(current.len());
    latest.resize(width, 0);
    current.resize(width, 0);
    latest > current
}

#[tauri::command]
pub(crate) async fn update_check() -> CommandResult<UpdateInfo> {
    tauri::async_runtime::spawn_blocking(|| {
        let fetch = |url: &str| std::process::Command::new("curl")
            .args([
                "--fail", "--silent", "--show-error", "--location",
                "--max-time", "12", "--header", "Accept: application/vnd.github+json",
                "--header", "X-GitHub-Api-Version: 2022-11-28",
                url,
            ])
            .output()
            .map_err(|error| format!("cannot run curl to check updates: {error}"));
        let current = env!("CARGO_PKG_VERSION").to_string();
        let release_output = fetch("https://api.github.com/repos/namannn04/Veronica/releases?per_page=100")?;
        let releases: Vec<GithubRelease> = if release_output.status.success() {
            serde_json::from_slice(&release_output.stdout)
                .map_err(|error| format!("GitHub returned invalid release data: {error}"))?
        } else {
            Vec::new()
        };
        if let Some(release) = app_release(releases) {
            let package_url = package_asset(&release.assets, install_kind())
                .map(|asset| asset.browser_download_url.clone());
            return Ok(UpdateInfo {
                update_available: newer_version(&release.tag_name, &current),
                current_version: current,
                latest_version: release.tag_name.trim_start_matches(['v', 'V']).to_string(),
                release_url: release.html_url,
                package_url,
                published_at: release.published_at,
                notes: release.body.unwrap_or_default(),
                install_kind: install_kind().into(),
                can_install: install_kind() != "appimage",
            });
        }

        let manifest_output = fetch("https://raw.githubusercontent.com/namannn04/Veronica/main/apps/desktop/src-tauri/tauri.conf.json")?;
        if !manifest_output.status.success() {
            let detail = String::from_utf8_lossy(&manifest_output.stderr);
            return Err(format!("cannot check Veronica releases: {}", detail.trim()));
        }
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_output.stdout)
            .map_err(|error| format!("GitHub returned an invalid Veronica manifest: {error}"))?;
        let latest = manifest.get("version").and_then(|value| value.as_str())
            .ok_or_else(|| "Veronica's online manifest has no version".to_string())?;
        Ok(UpdateInfo {
            update_available: newer_version(latest, &current),
            current_version: current,
            latest_version: latest.to_string(),
            release_url: "https://github.com/namannn04/Veronica".to_string(),
            package_url: None,
            published_at: None,
            notes: "No packaged GitHub Release has been published yet.".to_string(),
            install_kind: install_kind().into(),
            can_install: false,
        })
    }).await.map_err(|error| format!("update check failed: {error}"))?
}

#[tauri::command]
pub(crate) async fn update_install(version: String) -> CommandResult<String> {
    let valid = version.split('.').count() == 3
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    if !valid {
        return Err(format!("invalid release version: {version}"));
    }
    tauri::async_runtime::spawn_blocking(move || match install_kind() {
        "flatpak" => {
            let output = veronica_core::host::command("flatpak")
                .args(["update", "--noninteractive", "-y", veronica_core::APP_ID])
                .output()
                .map_err(|error| format!("cannot run flatpak: {error}"))?;
            if output.status.success() {
                Ok(format!("Veronica {version} is installed."))
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        }
        "snap" => {
            let output = std::process::Command::new("snap")
                .args(["refresh", "veronica"])
                .output()
                .map_err(|error| format!("cannot run snap: {error}"))?;
            if output.status.success() {
                Ok(format!("Veronica {version} is installed."))
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        }
        "deb" => install_deb_release(&version),
        _ => Err(
            "An AppImage cannot update itself; download the new one from the release page.".into(),
        ),
    })
    .await
    .map_err(|error| format!("update failed: {error}"))?
}

fn install_deb_release(version: &str) -> CommandResult<String> {
    let asset = format!("Veronica_{version}_amd64.deb");
    let root = format!("https://github.com/namannn04/Veronica/releases/download/v{version}");
    let directory = tempfile::Builder::new()
        .prefix(&format!("veronica-update-{version}-"))
        .tempdir()
        .map_err(|error| format!("cannot prepare update download: {error}"))?;
    let work = directory.path();
    (|| {
        let download = |name: &str| -> CommandResult<std::path::PathBuf> {
            let path = work.join(name);
            veronica_system::download::download_https(&format!("{root}/{name}"), &path)
                .map_err(|error| format!("cannot download {name}: {error:#}"))
        };
        let package = download(&asset)?;
        let sums =
            std::fs::read_to_string(download("SHA256SUMS")?).map_err(|error| error.to_string())?;
        let expected = sums
            .lines()
            .find_map(|line| {
                let mut fields = line.split_whitespace();
                let hash = fields.next()?;
                (fields.next()? == asset).then(|| hash.to_string())
            })
            .ok_or_else(|| format!("the release checksums do not list {asset}"))?;
        let actual = std::process::Command::new("sha256sum")
            .arg(&package)
            .output()
            .map_err(|error| format!("cannot run sha256sum: {error}"))?;
        let actual = String::from_utf8_lossy(&actual.stdout);
        if actual.split_whitespace().next() != Some(expected.as_str()) {
            return Err(
                "the download does not match its published checksum; nothing was installed".into(),
            );
        }

        let output = std::process::Command::new("pkexec")
            .args(["apt-get", "install", "--reinstall", "-y"])
            .arg(&package)
            .env("DEBIAN_FRONTEND", "noninteractive")
            .output()
            .map_err(|error| format!("cannot run pkexec: {error}"))?;
        match output.status.code() {
            Some(0) => Ok(format!("Veronica {version} is installed.")),
            Some(126 | 127) => Err("Authentication was cancelled; nothing was installed.".into()),
            _ => {
                let text = String::from_utf8_lossy(&output.stderr);
                let mut tail: Vec<&str> = text.lines().rev().take(5).collect();
                tail.reverse();
                Err(tail.join("\n"))
            }
        }
    })()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn app_restart(app: AppHandle) -> CommandResult<()> {
    let relaunch = restart_command(install_kind(), std::env::current_exe().ok());
    std::process::Command::new(&relaunch[0])
        .args(&relaunch[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start the new version: {error}"))?;
    app.exit(0);
    Ok(())
}

fn restart_command(kind: &str, current_exe: Option<std::path::PathBuf>) -> Vec<String> {
    const WAIT: &str = "sleep 2";
    if kind == "flatpak" {
        vec![
            "flatpak-spawn".into(),
            "--host".into(),
            "sh".into(),
            "-c".into(),
            format!("{WAIT}; exec flatpak run io.github.namannn04.Veronica"),
        ]
    } else {
        let program = if kind == "snap" {
            Some(SNAP_LAUNCHER.to_string())
        } else {
            std::env::var("APPIMAGE").ok().or_else(|| {
                current_exe.map(|path| {
                    let shown = path.display().to_string();
                    shown.trim_end_matches(" (deleted)").to_string()
                })
            })
        };
        let program = program.unwrap_or_else(|| "veronica".into());
        vec![
            "setsid".into(),
            "sh".into(),
            "-c".into(),
            format!("{WAIT}; exec \"$0\""),
            program,
        ]
    }
}

#[tauri::command]
pub(crate) fn cleaner_categories() -> &'static [veronica_core::CleanerCategory] {
    veronica_core::cleaner::CATEGORIES
}

#[tauri::command]
pub(crate) async fn cleaner_scan(
    categories: Vec<String>,
) -> CommandResult<veronica_core::CleanerScan> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = veronica_core::paths::home_dir()
            .ok_or_else(|| "cannot resolve the home directory".to_string())?;
        let selected = if categories.is_empty() {
            None
        } else {
            Some(categories)
        };
        Ok(veronica_core::cleaner::scan_caches(
            &home,
            selected.as_deref(),
        ))
    })
    .await
    .map_err(|error| format!("the scan could not run: {error}"))?
}

#[tauri::command]
pub(crate) async fn cleaner_clean(
    items: Vec<veronica_core::cleaner::Item>,
) -> CommandResult<veronica_core::cleaner::CleanReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = veronica_core::paths::home_dir()
            .ok_or_else(|| "cannot resolve the home directory".to_string())?;
        let mut report = veronica_core::cleaner::CleanReport::default();
        for item in items {
            let target = std::path::Path::new(&item.path);
            if !target.starts_with(&home) {
                report
                    .failed
                    .push((item.path, "outside the home directory".to_string()));
                continue;
            }
            let bytes = veronica_core::cleaner::directory_size(target);
            match veronica_core::cleaner::trash(&home, target) {
                Ok(_) => {
                    report.bytes_reclaimed = report.bytes_reclaimed.saturating_add(bytes);
                    report
                        .trashed
                        .push(veronica_core::cleaner::Item { bytes, ..item });
                }
                Err(error) => report.failed.push((item.path, format!("{error:#}"))),
            }
        }
        Ok(report)
    })
    .await
    .map_err(|error| format!("the clean could not run: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::newer_version;

    fn release(tag: &str, draft: bool, prerelease: bool) -> super::GithubRelease {
        super::GithubRelease {
            tag_name: tag.into(),
            draft,
            prerelease,
            html_url: format!("https://example.test/{tag}"),
            published_at: None,
            body: None,
            assets: Vec::new(),
        }
    }

    #[test]
    fn only_app_version_tags_count_as_app_releases() {
        let releases = vec![
            release("ext/media/v1.0.9", false, false),
            release("v0.1.9", false, false),
            release("v0.1.30", false, true),
            release("v0.2.0", true, false),
            release("v0.1.12", false, false),
            release("ext/usage/v9.9.9", false, false),
            release("v0.1.x", false, false),
        ];
        let picked = super::app_release(releases).map(|release| release.tag_name);
        assert_eq!(picked.as_deref(), Some("v0.1.12"));
        assert!(super::app_release(vec![release("ext/media/v1.0.0", false, false)]).is_none());
        assert_eq!(super::app_version("v1.2.3"), Some(vec![1, 2, 3]));
        assert_eq!(super::app_version("ext/a/v1.2.3"), None);
        assert_eq!(super::app_version("v1.2"), None);
    }

    #[test]
    fn each_installation_is_offered_its_own_package() {
        let assets: Vec<super::GithubAsset> = [
            "SHA256SUMS",
            "Veronica_0.1.15_amd64.AppImage",
            "Veronica_0.1.15_amd64.deb",
            "Veronica_0.1.15_amd64.snap",
            "Veronica_0.1.15_x86_64.flatpak",
        ]
        .into_iter()
        .map(|name| super::GithubAsset {
            name: name.into(),
            browser_download_url: format!("https://example.test/{name}"),
        })
        .collect();
        let pick = |kind| super::package_asset(&assets, kind).map(|asset| asset.name.as_str());
        assert_eq!(pick("deb"), Some("Veronica_0.1.15_amd64.deb"));
        assert_eq!(pick("appimage"), Some("Veronica_0.1.15_amd64.AppImage"));
        assert_eq!(pick("flatpak"), Some("Veronica_0.1.15_x86_64.flatpak"));
        assert_eq!(pick("snap"), Some("Veronica_0.1.15_amd64.snap"));
    }

    #[test]
    fn a_snap_restarts_through_its_stable_launcher() {
        let command =
            super::restart_command("snap", Some("/snap/veronica/12/usr/bin/veronica".into()));
        assert_eq!(
            command.last().map(String::as_str),
            Some("/snap/bin/veronica")
        );
    }

    #[test]
    fn a_restart_starts_the_replaced_binary_without_background() {
        let command = super::restart_command(
            "deb",
            Some(std::path::PathBuf::from("/usr/bin/veronica (deleted)")),
        );
        assert_eq!(command[0], "setsid");
        assert_eq!(command.last().unwrap(), "/usr/bin/veronica");
        assert!(!command.iter().any(|part| part.contains("--background")));

        let flatpak = super::restart_command("flatpak", None);
        assert_eq!(&flatpak[..2], ["flatpak-spawn", "--host"]);
        assert!(flatpak
            .last()
            .unwrap()
            .ends_with("flatpak run io.github.namannn04.Veronica"));
    }

    #[test]
    fn update_versions_compare_numerically_and_ignore_release_prefixes() {
        assert!(newer_version("v0.2.0", "0.1.99"));
        assert!(newer_version("1.0.1", "1.0.0"));
        assert!(!newer_version("0.1.8", "0.1.8"));
        assert!(!newer_version("0.1.7", "0.1.8"));
    }
}
