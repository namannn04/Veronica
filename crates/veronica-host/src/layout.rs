use std::path::PathBuf;

use anyhow::Result;
use veronica_core::paths::AppDirectories;
use veronica_protocol::PackageVersion;

const EXTENSIONS: &str = "extensions";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    data: PathBuf,
    state: PathBuf,
    cache: PathBuf,
    runtime: PathBuf,
}

impl Layout {
    pub fn current() -> Result<Self> {
        Ok(Self::new(&AppDirectories::current()?))
    }

    pub fn new(directories: &AppDirectories) -> Self {
        Self {
            data: directories.data.join(EXTENSIONS),
            state: directories.state.join(EXTENSIONS),
            cache: directories.cache.join(EXTENSIONS),
            runtime: directories.runtime.clone(),
        }
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.data.join("packages")
    }

    pub fn package_dir(&self, id: &str) -> PathBuf {
        self.packages_dir().join(id)
    }

    pub fn version_dir(&self, id: &str, version: PackageVersion) -> PathBuf {
        self.package_dir(id).join(version.to_string())
    }

    pub fn current_link(&self, id: &str) -> PathBuf {
        self.package_dir(id).join("current")
    }

    pub fn previous_link(&self, id: &str) -> PathBuf {
        self.package_dir(id).join("previous")
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.data.join(".staging")
    }

    pub fn data_dir(&self, id: &str) -> PathBuf {
        self.data.join("data").join(id)
    }

    pub fn dev_link(&self, id: &str) -> PathBuf {
        self.data.join("dev").join(id)
    }

    pub fn installed_file(&self) -> PathBuf {
        self.state.join("installed.json")
    }

    pub fn catalog_file(&self) -> PathBuf {
        self.state.join("catalog.signed.json")
    }

    pub fn catalog_state_file(&self) -> PathBuf {
        self.state.join("catalog-state.json")
    }

    pub fn bundled_file(&self) -> PathBuf {
        self.state.join("bundled.json")
    }

    pub fn jobs_file(&self) -> PathBuf {
        self.state.join("jobs.json")
    }

    pub fn log_file(&self, id: &str) -> PathBuf {
        self.state.join("logs").join(format!("{id}.log"))
    }

    pub fn crash_file(&self, id: &str) -> PathBuf {
        self.state.join("crash").join(format!("{id}.json"))
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.cache.join("downloads")
    }

    pub fn download_part(&self, sha256: &str) -> PathBuf {
        self.downloads_dir().join(format!("{sha256}.part"))
    }

    pub fn lock_file(&self) -> PathBuf {
        self.runtime.join("extensions.lock")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use veronica_core::paths::{AppDirectories, EnvOverrides};

    use super::Layout;

    #[test]
    fn paths_follow_the_xdg_layout() {
        let env = EnvOverrides {
            data_home: Some("/d".into()),
            state_home: Some("/s".into()),
            cache_home: Some("/c".into()),
            runtime_dir: Some("/r".into()),
            ..EnvOverrides::default()
        };
        let layout = Layout::new(&AppDirectories::with_env(Path::new("/home/u"), env));
        let version = "1.2.3".parse().unwrap();
        let cases = [
            (
                layout.version_dir("media", version),
                "/d/veronica/extensions/packages/media/1.2.3",
            ),
            (
                layout.current_link("media"),
                "/d/veronica/extensions/packages/media/current",
            ),
            (
                layout.previous_link("media"),
                "/d/veronica/extensions/packages/media/previous",
            ),
            (layout.staging_dir(), "/d/veronica/extensions/.staging"),
            (
                layout.data_dir("media"),
                "/d/veronica/extensions/data/media",
            ),
            (layout.dev_link("media"), "/d/veronica/extensions/dev/media"),
            (
                layout.installed_file(),
                "/s/veronica/extensions/installed.json",
            ),
            (
                layout.catalog_file(),
                "/s/veronica/extensions/catalog.signed.json",
            ),
            (
                layout.catalog_state_file(),
                "/s/veronica/extensions/catalog-state.json",
            ),
            (layout.jobs_file(), "/s/veronica/extensions/jobs.json"),
            (
                layout.log_file("media"),
                "/s/veronica/extensions/logs/media.log",
            ),
            (
                layout.crash_file("media"),
                "/s/veronica/extensions/crash/media.json",
            ),
            (
                layout.download_part("ab"),
                "/c/veronica/extensions/downloads/ab.part",
            ),
            (layout.lock_file(), "/r/veronica/extensions.lock"),
        ];
        for (actual, expected) in cases {
            assert_eq!(actual, Path::new(expected));
        }
    }
}
