mod common;

use std::fs;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::{HostIdentity, Installer, Layout};

struct Home {
    root: TempDir,
}

impl Home {
    fn with_media() -> Self {
        let home = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let layout = Layout::new(&AppDirectories::with_env(
            home.root.path(),
            EnvOverrides::default(),
        ));
        Installer::new(layout, HostIdentity::current())
            .install_sideload(&common::extension_source(home.root.path(), "media"))
            .unwrap();
        home
    }

    fn vr(&self, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_vr"));
        for key in [
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_STATE_HOME",
            "XDG_RUNTIME_DIR",
        ] {
            command.env_remove(key);
        }
        command
            .env("HOME", self.root.path())
            .env("NO_COLOR", "1")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn the_media_commands_come_from_the_installed_extension() {
    let home = Home::with_media();
    let help = text(&home.vr(&["media", "--help"]));
    assert!(
        help.contains("Control whatever is playing, through MPRIS"),
        "{help}"
    );
    for leaf in ["status", "players", "library", "toggle", "previous"] {
        assert!(help.contains(leaf), "{help}");
    }
    let library = text(&home.vr(&["media", "lib", "--help"]));
    assert!(
        library.contains("Usage: vr media library [OPTIONS] [QUERY]"),
        "{library}"
    );
}

#[test]
fn the_library_lists_and_searches_music_under_home() {
    let home = Home::with_media();
    let album = home.root.path().join("Music/Night Drive");
    fs::create_dir_all(&album).unwrap();
    fs::write(album.join("Analog Sky - Neon.mp3"), b"").unwrap();
    fs::write(album.join("Low Tide - Harbour.flac"), b"").unwrap();
    fs::write(album.join("notes.txt"), b"").unwrap();

    let listed = home.vr(&["--json", "media", "library"]);
    assert_eq!(
        listed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let tracks: Value = serde_json::from_slice(&listed.stdout).unwrap();
    let titles: Vec<&str> = tracks
        .as_array()
        .unwrap()
        .iter()
        .map(|track| track["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Neon", "Harbour"]);
    assert_eq!(tracks[0]["album"], "Night Drive");

    let searched = text(&home.vr(&["media", "library", "harbour"]));
    assert!(searched.contains("Harbour"), "{searched}");
    assert!(!searched.contains("Neon"), "{searched}");
}

#[test]
fn a_missing_library_is_an_error_that_names_the_folder() {
    let home = Home::with_media();
    let output = home.vr(&["media", "library"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no music library"));
}
