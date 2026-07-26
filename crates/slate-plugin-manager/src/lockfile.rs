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
        if path.exists() {
            Self::load_from(&path)
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
