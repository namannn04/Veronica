//! XDG base directories for Veronica.
//!
//! Edith stores everything under `~/Library/Application Support/Edith`. On
//! Ubuntu the equivalent is the XDG basedir spec, so config, data, cache and
//! runtime are four distinct roots rather than one.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Freedesktop application id. Used for the desktop entry, AppStream
/// metadata, the D-Bus name and the notification sender.
pub const APP_ID: &str = "io.github.namannn04.Veronica";

/// Directory name used inside the XDG roots.
pub const APP_DIR: &str = "veronica";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirectories {
    pub configuration: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    pub runtime: PathBuf,
    pub state: PathBuf,
}

impl AppDirectories {
    /// Resolve the directories from the environment, honouring every XDG
    /// variable and falling back to the spec defaults when one is unset.
    pub fn current() -> Result<Self> {
        let home = home_dir().context("cannot resolve the home directory")?;
        Ok(Self::with_env(&home, EnvOverrides::from_env()))
    }

    /// Pure resolution, so the fallback rules stay testable without touching
    /// the real environment.
    pub fn with_env(home: &Path, env: EnvOverrides) -> Self {
        let base = |explicit: Option<PathBuf>, default: &str| -> PathBuf {
            explicit
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(default))
                .join(APP_DIR)
        };

        let cache = base(env.cache_home, ".cache");
        // XDG_RUNTIME_DIR has no spec-defined fallback. When it is absent the
        // cache is the only writable location guaranteed to exist.
        let runtime = env
            .runtime_dir
            .filter(|p| p.is_absolute())
            .map(|p| p.join(APP_DIR))
            .unwrap_or_else(|| cache.join("runtime"));

        Self {
            configuration: base(env.config_home, ".config"),
            data: base(env.data_home, ".local/share"),
            state: base(env.state_home, ".local/state"),
            cache,
            runtime,
        }
    }

    pub fn all(&self) -> [&Path; 5] {
        [
            &self.configuration,
            &self.data,
            &self.cache,
            &self.runtime,
            &self.state,
        ]
    }

    /// Create every directory. Safe to call on each launch.
    pub fn prepare(&self) -> Result<()> {
        for dir in self.all() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
        }
        Ok(())
    }

    /// Where the usage collector writes `usage.json` and `machines/`.
    pub fn usage_dir(&self) -> PathBuf {
        self.data.join("usage")
    }

    pub fn usage_file(&self) -> PathBuf {
        self.usage_dir().join("usage.json")
    }

    pub fn machines_dir(&self) -> PathBuf {
        self.usage_dir().join("machines")
    }

    /// Extracted copy of the bundled collector script.
    pub fn collector_script(&self) -> PathBuf {
        self.cache.join("bin").join("refresh-usage")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.configuration.join("settings.json")
    }

    pub fn limits_history_file(&self) -> PathBuf {
        self.state.join("limits-history.json")
    }

    pub fn clipboard_db(&self) -> PathBuf {
        self.data.join("clipboard.json")
    }

    /// Unix socket the CLI uses to reach a running app instance.
    pub fn ipc_socket(&self) -> PathBuf {
        self.runtime.join("veronica.sock")
    }
}

#[derive(Debug, Default, Clone)]
pub struct EnvOverrides {
    pub config_home: Option<PathBuf>,
    pub data_home: Option<PathBuf>,
    pub cache_home: Option<PathBuf>,
    pub state_home: Option<PathBuf>,
    pub runtime_dir: Option<PathBuf>,
}

impl EnvOverrides {
    pub fn from_env() -> Self {
        let read = |key: &str| {
            std::env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
        };
        Self {
            config_home: read("XDG_CONFIG_HOME"),
            data_home: read("XDG_DATA_HOME"),
            cache_home: read("XDG_CACHE_HOME"),
            state_home: read("XDG_STATE_HOME"),
            runtime_dir: read("XDG_RUNTIME_DIR"),
        }
    }
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/home/tester")
    }

    #[test]
    fn falls_back_to_spec_defaults_when_nothing_is_set() {
        let dirs = AppDirectories::with_env(&home(), EnvOverrides::default());
        assert_eq!(dirs.configuration, home().join(".config/veronica"));
        assert_eq!(dirs.data, home().join(".local/share/veronica"));
        assert_eq!(dirs.cache, home().join(".cache/veronica"));
        assert_eq!(dirs.state, home().join(".local/state/veronica"));
    }

    #[test]
    fn runtime_falls_back_under_cache_because_the_spec_defines_no_default() {
        let dirs = AppDirectories::with_env(&home(), EnvOverrides::default());
        assert_eq!(dirs.runtime, home().join(".cache/veronica/runtime"));
    }

    #[test]
    fn honours_absolute_overrides() {
        let env = EnvOverrides {
            runtime_dir: Some(PathBuf::from("/run/user/1000")),
            config_home: Some(PathBuf::from("/custom/config")),
            ..Default::default()
        };
        let dirs = AppDirectories::with_env(&home(), env);
        assert_eq!(dirs.runtime, PathBuf::from("/run/user/1000/veronica"));
        assert_eq!(dirs.configuration, PathBuf::from("/custom/config/veronica"));
    }

    #[test]
    fn rejects_relative_overrides_the_spec_says_to_ignore() {
        let env = EnvOverrides {
            data_home: Some(PathBuf::from("relative/path")),
            ..Default::default()
        };
        let dirs = AppDirectories::with_env(&home(), env);
        assert_eq!(dirs.data, home().join(".local/share/veronica"));
    }
}
