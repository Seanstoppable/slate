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
        let parts: Vec<&str> = locked
            .source
            .trim_start_matches("https://")
            .trim_start_matches("github.com/")
            .split('/')
            .collect();

        if parts.len() < 2 {
            return Ok(None);
        }

        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            parts[0], parts[1]
        );

        let client = reqwest::Client::new();
        let response: serde_json::Value = client
            .get(&url)
            .header("User-Agent", "slate-plugin-manager")
            .send()
            .await?
            .json()
            .await?;

        let latest = response["tag_name"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches('v');

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
}
