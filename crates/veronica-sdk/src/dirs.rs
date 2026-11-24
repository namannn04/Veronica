use std::path::Path;

use veronica_protocol::rpc::ExtensionDirs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    inner: ExtensionDirs,
}

impl Dirs {
    pub fn new(inner: ExtensionDirs) -> Self {
        Self { inner }
    }

    pub fn install(&self) -> &Path {
        &self.inner.install
    }

    pub fn data(&self) -> &Path {
        &self.inner.data
    }

    pub fn cache(&self) -> &Path {
        &self.inner.cache
    }

    pub fn state(&self) -> &Path {
        &self.inner.state
    }

    pub fn runtime(&self) -> &Path {
        &self.inner.runtime
    }

    pub fn create_writable(&self) -> std::io::Result<()> {
        for dir in [self.data(), self.cache(), self.state(), self.runtime()] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn accessors_return_the_paths_from_initialize() {
        let root = std::env::temp_dir().join(format!("veronica-sdk-dirs-{}", std::process::id()));
        let dirs = Dirs::new(ExtensionDirs {
            install: root.join("install"),
            data: root.join("data"),
            cache: root.join("cache"),
            state: root.join("state"),
            runtime: root.join("runtime"),
        });
        assert_eq!(dirs.install(), root.join("install"));
        assert_eq!(dirs.data(), root.join("data"));
        assert_eq!(dirs.cache(), root.join("cache"));
        assert_eq!(dirs.state(), root.join("state"));
        assert_eq!(dirs.runtime(), root.join("runtime"));
        dirs.create_writable().unwrap();
        for name in ["data", "cache", "state", "runtime"] {
            assert!(root.join(name).is_dir());
        }
        assert!(!PathBuf::from(dirs.install()).exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
