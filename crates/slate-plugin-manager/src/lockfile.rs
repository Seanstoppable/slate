use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The lockfile tracks installed plugin versions and integrity hashes.
/// Located at ~/.config/slate/slate-lock.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(default)]
    pub plugins: HashMap<String, LockedPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPlugin {
    pub source: String,
    pub version: String,
    pub sha256: String,
    #[serde(default)]
    pub permissions_hash: Option<String>,
}

impl Lockfile {
    /// Load the lockfile from the default path.
    pub fn load_default() -> Result<Self> {
        let path = Self::default_path()?;
        Self::load_default_from_path(&path)
    }

    fn load_default_from_path(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load_from(path)
        } else {
            Ok(Self::default())
        }
    }

    /// Load lockfile from a specific path.
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read lockfile: {}", path.display()))?;
        toml::from_str(&content).context("Failed to parse lockfile")
    }

    /// Save lockfile to the default path.
    pub fn save_default(&self) -> Result<()> {
        let path = Self::default_path()?;
        self.save_to(&path)
    }

    /// Save lockfile to a specific path.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Lock a plugin entry.
    pub fn lock(&mut self, name: &str, plugin: LockedPlugin) {
        self.plugins.insert(name.to_string(), plugin);
    }

    /// Remove a plugin from the lockfile.
    pub fn unlock(&mut self, name: &str) {
        self.plugins.remove(name);
    }

    /// Check if a plugin is locked at a specific version.
    pub fn get(&self, name: &str) -> Option<&LockedPlugin> {
        self.plugins.get(name)
    }

    /// Default lockfile path.
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not determine config directory")?;
        Ok(config_dir.join("slate").join("slate-lock.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lockfile_serializes_and_deserializes() {
        let mut lockfile = Lockfile::default();
        lockfile.lock(
            "slate-github",
            LockedPlugin {
                source: "github.com/slate-community/slate-github".to_string(),
                version: "1.2.3".to_string(),
                sha256: "deadbeef".to_string(),
                permissions_hash: Some("abc123".to_string()),
            },
        );

        let serialized = toml::to_string(&lockfile).unwrap();
        let deserialized: Lockfile = toml::from_str(&serialized).unwrap();

        let plugin = deserialized.get("slate-github").unwrap();
        assert_eq!(plugin.source, "github.com/slate-community/slate-github");
        assert_eq!(plugin.version, "1.2.3");
        assert_eq!(plugin.sha256, "deadbeef");
        assert_eq!(plugin.permissions_hash.as_deref(), Some("abc123"));
    }

    #[test]
    fn lock_and_unlock_manage_entries() {
        let mut lockfile = Lockfile::default();
        lockfile.lock(
            "plugin",
            LockedPlugin {
                source: "github.com/user/plugin".to_string(),
                version: "0.1.0".to_string(),
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );
        assert!(lockfile.get("plugin").is_some());

        lockfile.unlock("plugin");
        assert!(lockfile.get("plugin").is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("slate-lock.toml");
        let mut lockfile = Lockfile::default();
        lockfile.lock(
            "plugin",
            LockedPlugin {
                source: "github.com/user/plugin".to_string(),
                version: "1.0.0".to_string(),
                sha256: "hash".to_string(),
                permissions_hash: Some("perm-hash".to_string()),
            },
        );

        lockfile.save_to(&path).unwrap();
        let loaded = Lockfile::load_from(&path).unwrap();

        let plugin = loaded.get("plugin").unwrap();
        assert_eq!(plugin.source, "github.com/user/plugin");
        assert_eq!(plugin.version, "1.0.0");
        assert_eq!(plugin.sha256, "hash");
        assert_eq!(plugin.permissions_hash.as_deref(), Some("perm-hash"));
    }

    #[test]
    fn load_from_invalid_content_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("invalid-lock.toml");
        std::fs::write(&path, "not = [valid").unwrap();

        let err = Lockfile::load_from(&path).unwrap_err();
        assert!(err.to_string().contains("Failed to parse lockfile"));
    }

    #[test]
    fn save_to_creates_parent_directories() {
        let dir = tempdir().unwrap();
        let path = dir
            .path()
            .join("nested")
            .join("plugins")
            .join("slate-lock.toml");
        let lockfile = Lockfile::default();

        lockfile.save_to(&path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn load_default_from_path_returns_default_when_file_is_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing").join("slate-lock.toml");

        let loaded = Lockfile::load_default_from_path(&path).unwrap();

        assert!(loaded.plugins.is_empty());
    }
}
