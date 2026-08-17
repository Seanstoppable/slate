use slate_plugin_sdk::{Permissions, WidgetConfig};
use url::Url;

/// Resolve user-configured HTTP(S) destinations into a concrete host allowlist.
pub fn resolve_network_permissions(
    mut permissions: Permissions,
    widget_config: &WidgetConfig,
) -> Permissions {
    let mut hosts = permissions
        .network
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();

    for setting in &permissions.network_from_config {
        let urls = match widget_config.settings.get(setting) {
            Some(serde_json::Value::String(url)) => vec![url.as_str()],
            Some(serde_json::Value::Array(urls)) => {
                urls.iter().filter_map(serde_json::Value::as_str).collect()
            }
            Some(_) | None => Vec::new(),
        };

        for url in urls {
            if let Ok(url) = Url::parse(url.trim()) {
                if matches!(url.scheme(), "http" | "https") {
                    if let Some(host) = url.host_str() {
                        hosts.insert(host.to_string());
                    }
                }
            }
        }
    }

    permissions.network = hosts.into_iter().collect();
    permissions.network_from_config.clear();
    permissions
}

/// Guards capability access based on declared permissions.
#[derive(Debug, Clone)]
pub struct PermissionGuard {
    permissions: Permissions,
}

impl PermissionGuard {
    pub fn new(permissions: Permissions) -> Self {
        Self { permissions }
    }

    /// Reject network permission declarations that would grant unrestricted access.
    pub fn validate(permissions: &Permissions) -> Result<(), PermissionError> {
        if permissions.network.iter().any(|host| host == "*") {
            return Err(PermissionError::WildcardNetworkHost);
        }
        Ok(())
    }

    /// Check if HTTP access to a specific host is permitted.
    pub fn check_network(&self, host: &str) -> Result<(), PermissionError> {
        if self.permissions.network.iter().any(|allowed| {
            host == allowed
                || host
                    .strip_suffix(allowed)
                    .is_some_and(|prefix| prefix.ends_with('.'))
        }) {
            Ok(())
        } else {
            Err(PermissionError::NetworkDenied(host.to_string()))
        }
    }

    /// Check if execution of a specific binary is permitted.
    pub fn check_exec(&self, cmd: &str) -> Result<(), PermissionError> {
        if self.permissions.exec.iter().any(|allowed| cmd == allowed) {
            Ok(())
        } else {
            Err(PermissionError::ExecDenied(cmd.to_string()))
        }
    }

    /// Check if a system info category is permitted.
    pub fn check_system(&self, category: &str) -> Result<(), PermissionError> {
        if self
            .permissions
            .system
            .iter()
            .any(|allowed| category == allowed)
        {
            Ok(())
        } else {
            Err(PermissionError::SystemDenied(category.to_string()))
        }
    }

    /// Check if filesystem read access to a path is permitted.
    pub fn check_filesystem_read(&self, path: &str) -> Result<(), PermissionError> {
        if self
            .permissions
            .filesystem_read
            .iter()
            .any(|allowed| path.starts_with(allowed))
        {
            Ok(())
        } else {
            Err(PermissionError::FilesystemDenied(path.to_string()))
        }
    }

    /// Check if storage access is permitted.
    pub fn check_storage(&self) -> Result<(), PermissionError> {
        if self.permissions.storage {
            Ok(())
        } else {
            Err(PermissionError::StorageDenied)
        }
    }

    /// Check if raw network (ICMP/ping) is permitted.
    pub fn check_raw_network(&self) -> Result<(), PermissionError> {
        if self.permissions.raw_network {
            Ok(())
        } else {
            Err(PermissionError::RawNetworkDenied)
        }
    }

    /// Check if a secret is declared and accessible.
    pub fn check_secret(&self, name: &str) -> Result<(), PermissionError> {
        if self.permissions.secrets.iter().any(|s| s == name) {
            Ok(())
        } else {
            Err(PermissionError::SecretDenied(name.to_string()))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("wildcard network permissions are not supported")]
    WildcardNetworkHost,
    #[error("network access denied for host: {0}")]
    NetworkDenied(String),
    #[error("exec access denied for command: {0}")]
    ExecDenied(String),
    #[error("system info access denied for category: {0}")]
    SystemDenied(String),
    #[error("filesystem read access denied for path: {0}")]
    FilesystemDenied(String),
    #[error("storage access denied")]
    StorageDenied,
    #[error("raw network access denied")]
    RawNetworkDenied,
    #[error("secret access denied: {0}")]
    SecretDenied(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::Position;

    fn widget_config(
        settings: std::collections::HashMap<String, serde_json::Value>,
    ) -> WidgetConfig {
        WidgetConfig {
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings,
            refresh_interval: None,
        }
    }

    #[test]
    fn resolves_network_hosts_from_declared_config_fields() {
        let permissions = resolve_network_permissions(
            Permissions {
                network: vec!["api.example.test".to_string()],
                network_from_config: vec!["api_url".to_string(), "urls".to_string()],
                ..Default::default()
            },
            &widget_config(std::collections::HashMap::from([
                (
                    "api_url".to_string(),
                    serde_json::json!("https://pihole.example.test/admin/api.php"),
                ),
                (
                    "urls".to_string(),
                    serde_json::json!([
                        "https://status.example.test",
                        "http://status.example.test/health",
                        "ftp://ignored.example.test",
                        "not a URL"
                    ]),
                ),
            ])),
        );

        assert_eq!(
            permissions.network,
            vec![
                "api.example.test".to_string(),
                "pihole.example.test".to_string(),
                "status.example.test".to_string(),
            ]
        );
        assert!(permissions.network_from_config.is_empty());
    }

    #[test]
    fn test_network_permission() {
        let perms = Permissions {
            network: vec!["api.github.com".to_string()],
            ..Default::default()
        };
        let guard = PermissionGuard::new(perms);
        assert!(guard.check_network("api.github.com").is_ok());
        assert!(guard.check_network("evil.com").is_err());
    }

    #[test]
    fn test_storage_permission() {
        let guard = PermissionGuard::new(Permissions::default());
        assert!(guard.check_storage().is_err());

        let perms = Permissions {
            storage: true,
            ..Default::default()
        };
        let guard = PermissionGuard::new(perms);
        assert!(guard.check_storage().is_ok());
    }

    #[test]
    fn test_exec_permission() {
        let guard = PermissionGuard::new(Permissions {
            exec: vec!["git".to_string(), "cargo".to_string()],
            ..Default::default()
        });

        assert!(guard.check_exec("git").is_ok());
        assert!(matches!(
            guard.check_exec("bash"),
            Err(PermissionError::ExecDenied(cmd)) if cmd == "bash"
        ));
    }

    #[test]
    fn test_filesystem_read_permission() {
        let guard = PermissionGuard::new(Permissions {
            filesystem_read: vec!["C:\\allowed".to_string()],
            ..Default::default()
        });

        assert!(guard.check_filesystem_read("C:\\allowed\\file.txt").is_ok());
        assert!(matches!(
            guard.check_filesystem_read("C:\\blocked\\file.txt"),
            Err(PermissionError::FilesystemDenied(path)) if path == "C:\\blocked\\file.txt"
        ));
    }

    #[test]
    fn test_secret_permission() {
        let guard = PermissionGuard::new(Permissions {
            secrets: vec!["GITHUB_TOKEN".to_string()],
            ..Default::default()
        });

        assert!(guard.check_secret("GITHUB_TOKEN").is_ok());
        assert!(matches!(
            guard.check_secret("API_KEY"),
            Err(PermissionError::SecretDenied(name)) if name == "API_KEY"
        ));
    }

    #[test]
    fn test_raw_network_permission() {
        let denied = PermissionGuard::new(Permissions::default());
        assert!(matches!(
            denied.check_raw_network(),
            Err(PermissionError::RawNetworkDenied)
        ));

        let allowed = PermissionGuard::new(Permissions {
            raw_network: true,
            ..Default::default()
        });
        assert!(allowed.check_raw_network().is_ok());
    }

    #[test]
    fn test_allowed_and_denied_hosts() {
        let guard = PermissionGuard::new(Permissions {
            network: vec!["github.com".to_string(), "api.internal".to_string()],
            ..Default::default()
        });

        assert!(guard.check_network("github.com").is_ok());
        assert!(guard.check_network("service.api.internal").is_ok());
        assert!(matches!(
            guard.check_network("gitlab.com"),
            Err(PermissionError::NetworkDenied(host)) if host == "gitlab.com"
        ));
    }

    #[test]
    fn test_network_permission_rejects_partial_host_matches() {
        let guard = PermissionGuard::new(Permissions {
            network: vec!["github.com".to_string()],
            ..Default::default()
        });

        assert!(guard.check_network("api.github.com").is_ok());
        assert!(matches!(
            guard.check_network("evilgithub.com"),
            Err(PermissionError::NetworkDenied(host)) if host == "evilgithub.com"
        ));
    }

    #[test]
    fn network_permission_validation_rejects_wildcard_host() {
        let permissions = Permissions {
            network: vec!["*".to_string()],
            ..Default::default()
        };

        assert!(matches!(
            PermissionGuard::validate(&permissions),
            Err(PermissionError::WildcardNetworkHost)
        ));
    }

    #[test]
    fn test_system_permission_requires_exact_category_match() {
        let guard = PermissionGuard::new(Permissions {
            system: vec!["cpu".to_string(), "memory".to_string()],
            ..Default::default()
        });

        assert!(guard.check_system("cpu").is_ok());
        assert!(matches!(
            guard.check_system("disk"),
            Err(PermissionError::SystemDenied(category)) if category == "disk"
        ));
    }

    #[test]
    fn test_permission_errors_render_readable_messages() {
        assert_eq!(
            PermissionError::NetworkDenied("example.com".to_string()).to_string(),
            "network access denied for host: example.com"
        );
        assert_eq!(
            PermissionError::SystemDenied("cpu".to_string()).to_string(),
            "system info access denied for category: cpu"
        );
    }

    #[test]
    fn test_empty_permissions_deny_all_restricted_access() {
        let guard = PermissionGuard::new(Permissions::default());

        assert!(guard.check_network("example.com").is_err());
        assert!(guard.check_exec("git").is_err());
        assert!(guard.check_filesystem_read("C:\\file.txt").is_err());
        assert!(guard.check_secret("TOKEN").is_err());
        assert!(guard.check_raw_network().is_err());
        assert!(guard.check_storage().is_err());
    }
}
