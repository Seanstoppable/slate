use anyhow::{Context, Result};
use std::path::PathBuf;

/// Manages plugin installation from GitHub repositories.
pub struct PluginInstaller {
    plugins_dir: PathBuf,
}

impl PluginInstaller {
    pub fn new(plugins_dir: PathBuf) -> Self {
        Self { plugins_dir }
    }

    /// Default plugins directory (~/.local/share/slate/plugins/)
    pub fn default_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_dir().context("Could not determine data directory")?;
        Ok(data_dir.join("slate").join("plugins"))
    }

    /// Install a plugin from a GitHub source (e.g., "github.com/owner/repo").
    pub async fn install(&self, source: &str, version: Option<&str>) -> Result<InstalledPlugin> {
        let (owner, repo) = parse_github_source(source)?;

        // Determine version to install
        let latest_version = match version {
            Some(_) => None,
            None => Some(self.fetch_latest_version(&owner, &repo).await?),
        };
        let version = resolve_version(version, latest_version.as_deref())?;

        // Download release asset (WASM file)
        let download_url = format!(
            "https://github.com/{}/{}/releases/download/v{}/{}.wasm",
            owner, repo, version, repo
        );

        let dest_dir = self.plugins_dir.join(&repo);
        std::fs::create_dir_all(&dest_dir)?;

        let client = reqwest::Client::new();
        let response = client
            .get(&download_url)
            .header("User-Agent", "slate-plugin-manager")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to download plugin from {}: {}",
                download_url,
                response.status()
            );
        }

        let bytes = response.bytes().await?;
        let wasm_path = dest_dir.join(format!("{}.wasm", repo));
        std::fs::write(&wasm_path, &bytes)?;

        // Compute SHA256 for lockfile integrity
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(&bytes));

        Ok(InstalledPlugin {
            name: repo,
            source: source.to_string(),
            version,
            path: wasm_path,
            sha256: hash,
        })
    }

    /// Remove an installed plugin.
    pub fn remove(&self, name: &str) -> Result<()> {
        let plugin_dir = self.plugins_dir.join(name);
        if plugin_dir.exists() {
            std::fs::remove_dir_all(&plugin_dir)?;
        }
        Ok(())
    }

    /// List installed plugins.
    pub fn list_installed(&self) -> Result<Vec<String>> {
        let mut plugins = Vec::new();
        if self.plugins_dir.exists() {
            for entry in std::fs::read_dir(&self.plugins_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        plugins.push(name.to_string());
                    }
                }
            }
        }
        Ok(plugins)
    }

    async fn fetch_latest_version(&self, owner: &str, repo: &str) -> Result<String> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            owner, repo
        );
        let client = reqwest::Client::new();
        let response: serde_json::Value = client
            .get(&url)
            .header("User-Agent", "slate-plugin-manager")
            .send()
            .await?
            .json()
            .await?;

        let tag = response["tag_name"]
            .as_str()
            .context("No tag_name in latest release")?;

        Ok(tag.trim_start_matches('v').to_string())
    }
}

/// A successfully installed plugin.
#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    pub name: String,
    pub source: String,
    pub version: String,
    pub path: PathBuf,
    pub sha256: String,
}

/// Parse "github.com/owner/repo" into (owner, repo).
fn parse_github_source(source: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = source
        .trim_start_matches("https://")
        .trim_start_matches("github.com/")
        .split('/')
        .collect();

    if parts.len() < 2 {
        anyhow::bail!("Invalid GitHub source: {}. Expected format: github.com/owner/repo", source);
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn resolve_version(requested: Option<&str>, latest: Option<&str>) -> Result<String> {
    match requested {
        Some(version) => Ok(version.to_string()),
        None => latest
            .map(str::to_string)
            .context("No latest version available"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_source() {
        let (owner, repo) = parse_github_source("github.com/slate-community/slate-github").unwrap();
        assert_eq!(owner, "slate-community");
        assert_eq!(repo, "slate-github");
    }

    #[test]
    fn test_parse_github_source_with_https() {
        let (owner, repo) =
            parse_github_source("https://github.com/user/plugin").unwrap();
        assert_eq!(owner, "user");
        assert_eq!(repo, "plugin");
    }

    #[test]
    fn test_resolve_version_prefers_requested_version() {
        let resolved = resolve_version(Some("1.2.3"), Some("9.9.9")).unwrap();
        assert_eq!(resolved, "1.2.3");
    }

    #[test]
    fn test_resolve_version_falls_back_to_latest_version() {
        let resolved = resolve_version(None, Some("2.0.0")).unwrap();
        assert_eq!(resolved, "2.0.0");
    }

    #[test]
    fn test_resolve_version_errors_without_requested_or_latest_version() {
        assert!(resolve_version(None, None).is_err());
    }
}
