use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Tracks update check state and available updates for the status bar.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateNotifications {
    /// Available updates discovered on last check.
    #[serde(default)]
    pub available_updates: Vec<UpdateInfo>,
    /// When we last checked for updates.
    #[serde(default)]
    pub last_check: Option<u64>,
    /// Whether notifications are dismissed until next check.
    #[serde(default)]
    pub dismissed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub name: String,
    pub current_version: String,
    pub latest_version: String,
}

impl UpdateNotifications {
    /// Load cached notification state from disk.
    pub fn load() -> Self {
        Self::cache_path()
            .and_then(|path| std::fs::read_to_string(&path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Save notification state to disk.
    pub fn save(&self) -> Result<()> {
        if let Some(path) = Self::cache_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string(self)?;
            std::fs::write(&path, content)?;
        }
        Ok(())
    }

    /// Check if enough time has passed since last check.
    pub fn should_check(&self, interval: &str) -> bool {
        let interval_secs = match interval {
            "hourly" => 3600,
            "daily" => 86400,
            "weekly" => 604800,
            _ => 86400,
        };

        match self.last_check {
            None => true,
            Some(last) => {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                now - last >= interval_secs
            }
        }
    }

    /// Mark that we just checked.
    pub fn mark_checked(&mut self) {
        self.last_check = Some(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        self.dismissed = false;
    }

    /// Store discovered updates.
    pub fn set_updates(&mut self, updates: Vec<UpdateInfo>) {
        self.available_updates = updates;
    }

    /// Dismiss notifications until next check.
    pub fn dismiss(&mut self) {
        self.dismissed = true;
    }

    /// Get the status bar message (if any).
    pub fn status_message(&self) -> Option<String> {
        if self.dismissed || self.available_updates.is_empty() {
            return None;
        }
        let count = self.available_updates.len();
        Some(format!(
            "│ 📦 {} update{} available ",
            count,
            if count == 1 { "" } else { "s" }
        ))
    }

    fn cache_path() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("slate").join("update-notifications.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_update(name: &str) -> UpdateInfo {
        UpdateInfo {
            name: name.to_string(),
            current_version: "1.0.0".to_string(),
            latest_version: "2.0.0".to_string(),
        }
    }

    #[test]
    fn test_should_check_never_checked() {
        let notifs = UpdateNotifications::default();
        assert!(notifs.should_check("daily"));
    }

    #[test]
    fn test_status_message_empty() {
        let notifs = UpdateNotifications::default();
        assert!(notifs.status_message().is_none());
    }

    #[test]
    fn test_status_message_with_updates() {
        let mut notifs = UpdateNotifications::default();
        notifs.set_updates(vec![sample_update("test")]);
        assert!(notifs.status_message().unwrap().contains("1 update"));
    }

    #[test]
    fn test_dismiss() {
        let mut notifs = UpdateNotifications::default();
        notifs.set_updates(vec![sample_update("test")]);
        notifs.dismiss();
        assert!(notifs.status_message().is_none());
    }

    #[test]
    fn test_mark_checked_sets_timestamp_and_clears_dismissed() {
        let mut notifs = UpdateNotifications {
            dismissed: true,
            ..Default::default()
        };

        notifs.mark_checked();

        assert!(notifs.last_check.is_some());
        assert!(!notifs.dismissed);
    }

    #[test]
    fn test_should_check_with_recent_timestamp_is_false() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let notifs = UpdateNotifications {
            last_check: Some(now - 60),
            ..Default::default()
        };

        assert!(!notifs.should_check("daily"));
    }

    #[test]
    fn test_should_check_with_old_timestamp_is_true() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let notifs = UpdateNotifications {
            last_check: Some(now - 172800),
            ..Default::default()
        };

        assert!(notifs.should_check("daily"));
    }

    #[test]
    fn test_should_check_uses_requested_intervals() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let hourly = UpdateNotifications {
            last_check: Some(now - 3599),
            ..Default::default()
        };
        assert!(!hourly.should_check("hourly"));
        let hourly_old = UpdateNotifications {
            last_check: Some(now - 3600),
            ..Default::default()
        };
        assert!(hourly_old.should_check("hourly"));

        let daily = UpdateNotifications {
            last_check: Some(now - 86399),
            ..Default::default()
        };
        assert!(!daily.should_check("daily"));
        let daily_old = UpdateNotifications {
            last_check: Some(now - 86400),
            ..Default::default()
        };
        assert!(daily_old.should_check("daily"));

        let weekly = UpdateNotifications {
            last_check: Some(now - 604799),
            ..Default::default()
        };
        assert!(!weekly.should_check("weekly"));
        let weekly_old = UpdateNotifications {
            last_check: Some(now - 604800),
            ..Default::default()
        };
        assert!(weekly_old.should_check("weekly"));

        let unknown = UpdateNotifications {
            last_check: Some(now - 86399),
            ..Default::default()
        };
        assert!(!unknown.should_check("something-else"));
        let unknown_old = UpdateNotifications {
            last_check: Some(now - 86400),
            ..Default::default()
        };
        assert!(unknown_old.should_check("something-else"));
    }

    #[test]
    fn test_set_updates_stores_updates() {
        let mut notifs = UpdateNotifications::default();
        let updates = vec![sample_update("github"), sample_update("weather")];

        notifs.set_updates(updates.clone());

        assert_eq!(notifs.available_updates.len(), 2);
        assert_eq!(notifs.available_updates[0].name, updates[0].name);
        assert_eq!(notifs.available_updates[1].name, updates[1].name);
    }

    #[test]
    fn test_status_message_pluralizes_multiple_updates() {
        let mut notifs = UpdateNotifications::default();
        notifs.set_updates(vec![sample_update("github"), sample_update("weather")]);

        assert_eq!(
            notifs.status_message().as_deref(),
            Some("│ 📦 2 updates available ")
        );
    }

    #[test]
    fn test_dismiss_hides_status_message() {
        let mut notifs = UpdateNotifications::default();
        notifs.set_updates(vec![sample_update("github"), sample_update("weather")]);

        notifs.dismiss();

        assert_eq!(notifs.status_message(), None);
    }

    #[test]
    fn test_save_and_load_round_trip_via_serde() {
        let mut notifs = UpdateNotifications::default();
        notifs.mark_checked();
        notifs.set_updates(vec![sample_update("github"), sample_update("weather")]);

        let serialized = serde_json::to_string(&notifs).unwrap();
        let loaded: UpdateNotifications = serde_json::from_str(&serialized).unwrap();

        assert_eq!(loaded.available_updates.len(), 2);
        assert_eq!(loaded.available_updates[0].name, "github");
        assert_eq!(loaded.available_updates[1].name, "weather");
        assert_eq!(loaded.last_check, notifs.last_check);
        assert_eq!(loaded.dismissed, notifs.dismissed);
    }
}
