use anyhow::Result;

use crate::install::PluginInstaller;
use crate::lockfile::Lockfile;

/// Check for available plugin updates.
pub struct UpdateChecker {
    _installer: PluginInstaller,
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
        Self {
            _installer: installer,
        }
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

    const KNOWN_RELEASE_SOURCE: &str = "github.com/sqlc-dev/sqlc-gen-greeter";

    async fn latest_known_release_version() -> Result<String> {
        let client = reqwest::Client::new();
        let response: serde_json::Value = client
            .get("https://api.github.com/repos/sqlc-dev/sqlc-gen-greeter/releases/latest")
            .header("User-Agent", "slate-plugin-manager-tests")
            .send()
            .await?
            .json()
            .await?;
        Ok(UpdateChecker::latest_tag_name(&response))
    }

    #[test]
    fn new_stores_installer() {
        let checker = UpdateChecker::new(PluginInstaller::new(PathBuf::from("plugins")));
        let _ = checker._installer;
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

    #[tokio::test]
    async fn check_single_returns_update_for_outdated_known_release() {
        let checker = UpdateChecker::new(PluginInstaller::new(PathBuf::from("plugins")));
        let locked = LockedPlugin {
            source: KNOWN_RELEASE_SOURCE.to_string(),
            version: "0.0.0".to_string(),
            sha256: "hash".to_string(),
            permissions_hash: None,
        };

        let update = match checker.check_single("greeter", &locked).await {
            Ok(Some(update)) => update,
            Ok(None) => panic!("expected available update"),
            Err(err) => {
                eprintln!("skipping network-dependent assertion: {err}");
                return;
            }
        };
        let latest = match latest_known_release_version().await {
            Ok(latest) => latest,
            Err(err) => {
                eprintln!("skipping network-dependent assertion: {err}");
                return;
            }
        };

        assert_eq!(update.name, "greeter");
        assert_eq!(update.current_version, "0.0.0");
        assert_eq!(update.latest_version, latest);
        assert_eq!(update.source, KNOWN_RELEASE_SOURCE);
    }

    #[tokio::test]
    async fn check_single_returns_none_when_version_matches_latest_release() {
        let checker = UpdateChecker::new(PluginInstaller::new(PathBuf::from("plugins")));
        let latest = match latest_known_release_version().await {
            Ok(latest) => latest,
            Err(err) => {
                eprintln!("skipping network-dependent assertion: {err}");
                return;
            }
        };
        let locked = LockedPlugin {
            source: KNOWN_RELEASE_SOURCE.to_string(),
            version: latest,
            sha256: "hash".to_string(),
            permissions_hash: None,
        };

        match checker.check_single("greeter", &locked).await {
            Ok(result) => assert!(result.is_none()),
            Err(err) => {
                eprintln!("skipping network-dependent assertion: {err}");
            }
        }
    }

    #[tokio::test]
    async fn check_outdated_collects_only_entries_with_newer_versions() {
        let checker = UpdateChecker::new(PluginInstaller::new(PathBuf::from("plugins")));
        let latest = match latest_known_release_version().await {
            Ok(latest) => latest,
            Err(err) => {
                eprintln!("skipping network-dependent assertion: {err}");
                return;
            }
        };
        let mut lockfile = Lockfile::default();
        lockfile.lock(
            "outdated",
            LockedPlugin {
                source: KNOWN_RELEASE_SOURCE.to_string(),
                version: "0.0.0".to_string(),
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );
        lockfile.lock(
            "current",
            LockedPlugin {
                source: KNOWN_RELEASE_SOURCE.to_string(),
                version: latest.clone(),
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );
        lockfile.lock(
            "local-only",
            LockedPlugin {
                source: "not-a-github-source".to_string(),
                version: latest,
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );

        let updates = checker.check_outdated(&lockfile).await.unwrap();

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "outdated");
    }

    #[tokio::test]
    async fn check_outdated_ignores_request_failures_and_keeps_processing() {
        let checker = UpdateChecker::new(PluginInstaller::new(PathBuf::from("plugins")));
        let mut lockfile = Lockfile::default();
        lockfile.lock(
            "greeter",
            LockedPlugin {
                source: "github.com/%zz/sqlc-gen-greeter".to_string(),
                version: "0.0.0".to_string(),
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );
        lockfile.lock(
            "not-github",
            LockedPlugin {
                source: "invalid".to_string(),
                version: "0.0.0".to_string(),
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );

        let updates = checker.check_outdated(&lockfile).await.unwrap();
        assert!(updates.is_empty());
    }

    #[tokio::test]
    async fn check_outdated_warn_branch_handles_invalid_request_urls() {
        let checker = UpdateChecker::new(PluginInstaller::new(PathBuf::from("plugins")));
        let mut lockfile = Lockfile::default();
        lockfile.lock(
            "broken",
            LockedPlugin {
                source: "github.com/owner/bad repo".to_string(),
                version: "0.0.0".to_string(),
                sha256: "hash".to_string(),
                permissions_hash: None,
            },
        );
        lockfile.lock(
            "local-only",
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
