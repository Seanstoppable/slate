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
        let download_url = build_download_url(&owner, &repo, &version);

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
        let hash = compute_sha256(&bytes);

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
        let url = latest_release_api_url(owner, repo);
        let client = reqwest::Client::new();
        let response: serde_json::Value = client
            .get(&url)
            .header("User-Agent", "slate-plugin-manager")
            .send()
            .await?
            .json()
            .await?;

        parse_latest_release_tag(&response)
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
        anyhow::bail!(
            "Invalid GitHub source: {}. Expected format: github.com/owner/repo",
            source
        );
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

fn latest_release_api_url(owner: &str, repo: &str) -> String {
    format!("https://api.github.com/repos/{owner}/{repo}/releases/latest")
}

fn build_download_url(owner: &str, repo: &str, version: &str) -> String {
    format!("https://github.com/{owner}/{repo}/releases/download/v{version}/{repo}.wasm")
}

fn parse_latest_release_tag(response: &serde_json::Value) -> Result<String> {
    let tag = response["tag_name"]
        .as_str()
        .context("No tag_name in latest release")?;

    Ok(tag.trim_start_matches('v').to_string())
}

fn compute_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const KNOWN_RELEASE_OWNER: &str = "sqlc-dev";
    const KNOWN_RELEASE_REPO: &str = "sqlc-gen-greeter";
    const KNOWN_RELEASE_SOURCE: &str = "github.com/sqlc-dev/sqlc-gen-greeter";
    const KNOWN_RELEASE_VERSION: &str = "0.1.0";

    #[test]
    fn test_parse_github_source() {
        let (owner, repo) = parse_github_source("github.com/slate-community/slate-github").unwrap();
        assert_eq!(owner, "slate-community");
        assert_eq!(repo, "slate-github");
    }

    #[test]
    fn test_parse_github_source_with_https() {
        let (owner, repo) = parse_github_source("https://github.com/user/plugin").unwrap();
        assert_eq!(owner, "user");
        assert_eq!(repo, "plugin");
    }

    #[test]
    fn test_parse_github_source_errors_with_invalid_input() {
        let err = parse_github_source("github.com-only").unwrap_err();
        assert!(err.to_string().contains("Invalid GitHub source"));
    }

    #[test]
    fn test_parse_github_source_ignores_extra_path_segments() {
        let (owner, repo) = parse_github_source("github.com/user/plugin/releases/latest").unwrap();
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

    #[test]
    fn test_list_installed_returns_empty_for_empty_directory() {
        let dir = tempdir().unwrap();
        let plugins_dir = dir.path().join("plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        let installer = PluginInstaller::new(plugins_dir);

        let installed = installer.list_installed().unwrap();

        assert!(installed.is_empty());
    }

    #[test]
    fn test_list_installed_returns_plugin_directories() {
        let dir = tempdir().unwrap();
        let plugins_dir = dir.path().join("plugins");
        std::fs::create_dir_all(plugins_dir.join("plugin-a")).unwrap();
        std::fs::create_dir_all(plugins_dir.join("plugin-b")).unwrap();
        std::fs::write(plugins_dir.join("README.txt"), "not a plugin dir").unwrap();
        let installer = PluginInstaller::new(plugins_dir);

        let mut installed = installer.list_installed().unwrap();
        installed.sort();

        assert_eq!(
            installed,
            vec!["plugin-a".to_string(), "plugin-b".to_string()]
        );
    }

    #[test]
    fn test_remove_deletes_existing_plugin_directory() {
        let dir = tempdir().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let plugin_dir = plugins_dir.join("plugin-a");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        let installer = PluginInstaller::new(plugins_dir);

        installer.remove("plugin-a").unwrap();

        assert!(!plugin_dir.exists());
    }

    #[test]
    fn test_remove_ignores_missing_plugin_directory() {
        let dir = tempdir().unwrap();
        let installer = PluginInstaller::new(dir.path().join("plugins"));

        installer.remove("missing-plugin").unwrap();
    }

    #[test]
    fn test_default_dir_ends_with_slate_plugins() {
        let dir = PluginInstaller::default_dir().unwrap();
        assert!(dir.ends_with(std::path::Path::new("slate").join("plugins")));
    }

    #[test]
    fn helper_functions_build_urls_and_hashes() {
        assert_eq!(
            latest_release_api_url("owner", "repo"),
            "https://api.github.com/repos/owner/repo/releases/latest"
        );
        assert_eq!(
            build_download_url("owner", "repo", "1.2.3"),
            "https://github.com/owner/repo/releases/download/v1.2.3/repo.wasm"
        );
        assert_eq!(
            parse_latest_release_tag(&serde_json::json!({"tag_name":"v2.0.0"})).unwrap(),
            "2.0.0"
        );
        assert!(parse_latest_release_tag(&serde_json::json!({})).is_err());
        assert_eq!(
            compute_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn fetch_latest_version_reads_known_release_tag() {
        let installer = PluginInstaller::new(PathBuf::from("plugins"));

        let latest = match installer
            .fetch_latest_version(KNOWN_RELEASE_OWNER, KNOWN_RELEASE_REPO)
            .await
        {
            Ok(latest) => latest,
            Err(err) => {
                eprintln!("skipping network-dependent assertion: {err}");
                return;
            }
        };

        assert_eq!(latest, KNOWN_RELEASE_VERSION);
    }

    #[tokio::test]
    async fn install_downloads_known_release_and_writes_wasm_file() {
        let dir = tempdir().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let installer = PluginInstaller::new(plugins_dir.clone());

        let installed = match installer
            .install(KNOWN_RELEASE_SOURCE, Some(KNOWN_RELEASE_VERSION))
            .await
        {
            Ok(installed) => installed,
            Err(err) => {
                eprintln!("skipping network-dependent assertion: {err}");
                return;
            }
        };

        let bytes = std::fs::read(&installed.path).unwrap();
        assert_eq!(installed.name, KNOWN_RELEASE_REPO);
        assert_eq!(installed.source, KNOWN_RELEASE_SOURCE);
        assert_eq!(installed.version, KNOWN_RELEASE_VERSION);
        assert_eq!(
            installed.path,
            plugins_dir
                .join(KNOWN_RELEASE_REPO)
                .join(format!("{}.wasm", KNOWN_RELEASE_REPO))
        );
        assert!(bytes.starts_with(b"\0asm"));
        assert_eq!(installed.sha256, compute_sha256(&bytes));
    }

    #[tokio::test]
    async fn install_creates_destination_directory_before_request_failures() {
        let dir = tempdir().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let installer = PluginInstaller::new(plugins_dir.clone());
        let plugin_dir = plugins_dir.join(KNOWN_RELEASE_REPO);
        let wasm_path = plugin_dir.join(format!("{}.wasm", KNOWN_RELEASE_REPO));

        let err = match installer
            .install(KNOWN_RELEASE_SOURCE, Some("0.1.0-does-not-exist"))
            .await
        {
            Ok(_) => panic!("expected missing release asset to fail"),
            Err(err) => err,
        };

        if err
            .to_string()
            .contains("error sending request for url")
            || err.to_string().contains("tunnel error")
        {
            eprintln!("skipping network-dependent assertion: {err}");
            return;
        }

        assert!(plugin_dir.exists());
        assert!(!wasm_path.exists());
        assert!(err.to_string().contains("Failed to download plugin"));
    }

    #[tokio::test]
    async fn install_validates_source_before_creating_directories() {
        let dir = tempdir().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let installer = PluginInstaller::new(plugins_dir.clone());

        let err = installer
            .install("not-a-github-source", Some(KNOWN_RELEASE_VERSION))
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Invalid GitHub source"));
        assert!(!plugins_dir.exists());
    }

    #[tokio::test]
    async fn install_returns_latest_lookup_error_before_creating_plugin_dir() {
        let dir = tempdir().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let installer = PluginInstaller::new(plugins_dir.clone());

        let err = installer
            .install("github.com/sqlc-dev/definitely-no-such-plugin-for-slate-tests", None)
            .await
            .unwrap_err();

        assert!(!plugins_dir.exists());
        assert!(err.to_string().contains("No tag_name in latest release"));
    }
}
