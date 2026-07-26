use slate_plugin_sdk::Permissions;

/// Guards capability access based on declared permissions.
#[derive(Debug, Clone)]
pub struct PermissionGuard {
    permissions: Permissions,
}

impl PermissionGuard {
    pub fn new(permissions: Permissions) -> Self {
        Self { permissions }
    }

    /// Check if HTTP access to a specific host is permitted.
    pub fn check_network(&self, host: &str) -> Result<(), PermissionError> {
        if self.permissions.network.iter().any(|allowed| host.contains(allowed)) {
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
        if self.permissions.system.iter().any(|allowed| category == allowed) {
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
}
