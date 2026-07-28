use anyhow::Result;

use crate::install::PluginInstaller;
use crate::lockfile::Lockfile;

/// Check for available plugin updates.
pub struct UpdateChecker {
    installer: PluginInstaller,
}

#[derive(Debug, Clone)]
pub struct AvailableUpdate {
    pub name: String,
    pub current_version: String,
    pub latest_version: String,
    pub source: String,
}

impl UpdateChecker {
    pub fn new(installer: PluginInstaller) -> Self {
        Self { installer }
    }

    /// Check all locked plugins for available updates.
    pub async fn check_outdated(&self, lockfile: &Lockfile) -> Result<Vec<AvailableUpdate>> {
        let mut updates = Vec::new();

        for (name, locked) in &lockfile.plugins {
            match self.check_single(name, locked).await {
                Ok(Some(update)) => updates.push(update),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Failed to check updates for {}: {}", name, e);
                }
            }
        }

        Ok(updates)
    }

    async fn check_single(
        &self,
        name: &str,
        locked: &crate::lockfile::LockedPlugin,
    ) -> Result<Option<AvailableUpdate>> {
        // Parse source to get owner/repo for GitHub API call
        let Some((owner, repo)) = Self::parse_github_source_parts(&locked.source) else {
            return Ok(None);
        };

        let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");

        let client = reqwest::Client::new();
        let response: serde_json::Value = client
            .get(&url)
            .header("User-Agent", "slate-plugin-manager")
            .send()
            .await?
            .json()
            .await?;

        let latest = Self::latest_tag_name(&response);

        if latest != locked.version && !latest.is_empty() {
            Ok(Some(AvailableUpdate {
                name: name.to_string(),
                current_version: locked.version.clone(),
                latest_version: latest.to_string(),
                source: locked.source.clone(),
            }))
        } else {
            Ok(None)
        }
    }

    fn parse_github_source_parts(source: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = source
            .trim_start_matches("https://")
            .trim_start_matches("github.com/")
            .split('/')
            .collect();

        if parts.len() < 2 {
            None
        } else {
            Some((parts[0].to_string(), parts[1].to_string()))
        }
    }

    fn latest_tag_name(response: &serde_json::Value) -> String {
        response["tag_name"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches('v')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::PluginInstaller;
    use crate::lockfile::{LockedPlugin, Lockfile};
    use std::path::PathBuf;

    #[test]
    fn new_stores_installer() {
        let checker = UpdateChecker::new(PluginInstaller::new(PathBuf::from("plugins")));
        let _ = checker.installer;
    }

    #[tokio::test]
    async fn check_single_returns_none_for_non_github_sources() {
        let checker = UpdateChecker::new(PluginInstaller::new(PathBuf::from("plugins")));
        let locked = LockedPlugin {
            source: "not-a-github-source".to_string(),
            version: "1.0.0".to_string(),
            sha256: "hash".to_string(),
            permissions_hash: None,
        };

        assert!(checker
            .check_single("plugin", &locked)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn check_outdated_skips_non_github_entries() {
        let checker = UpdateChecker::new(PluginInstaller::new(PathBuf::from("plugins")));
        let mut lockfile = Lockfile::default();
        lockfile.lock(
            "plugin",
            LockedPlugin {
                source: "invalid".to_string(),
                version: "1.0.0".to_string(),
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );

        let updates = checker.check_outdated(&lockfile).await.unwrap();
        assert!(updates.is_empty());
    }

    #[test]
    fn helper_functions_parse_sources_and_tags() {
        assert_eq!(
            UpdateChecker::parse_github_source_parts("https://github.com/owner/repo/releases"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(UpdateChecker::parse_github_source_parts("invalid"), None);
        assert_eq!(
            UpdateChecker::latest_tag_name(&serde_json::json!({"tag_name":"v1.2.3"})),
            "1.2.3"
        );
        assert_eq!(UpdateChecker::latest_tag_name(&serde_json::json!({})), "");
    }
}
