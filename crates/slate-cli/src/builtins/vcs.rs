use std::process::Command;

use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};

pub(crate) struct VcsWidget {
    engine: String,
    repo_path: String,
}

impl VcsWidget {
    pub(crate) fn new(config: WidgetConfig) -> Self {
        let engine = config
            .settings
            .get("engine")
            .and_then(|v| v.as_str())
            .unwrap_or("git")
            .to_string();
        let repo_path = config
            .settings
            .get("repo_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();
        Self { engine, repo_path }
    }
}

impl slate_plugin_sdk::Widget for VcsWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: format!("VCS ({})", self.engine),
            description: "Version control status".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, config: WidgetConfig) {
        if let Some(e) = config.settings.get("engine").and_then(|v| v.as_str()) {
            self.engine = e.to_string();
        }
        if let Some(p) = config.settings.get("repo_path").and_then(|v| v.as_str()) {
            self.repo_path = p.to_string();
        }
    }

    fn refresh(&mut self) -> WidgetContent {
        if self.repo_path.trim().is_empty() || self.repo_path == "." {
            return WidgetContent::Text {
                content: "Configure repo_path in settings".to_string(),
                scrollable: false,
                wrap: true,
            };
        }

        let path = std::path::Path::new(&self.repo_path);
        if !path.exists() {
            return WidgetContent::Text {
                content: format!("Repo path not found: {}", self.repo_path),
                scrollable: false,
                wrap: true,
            };
        }

        let (branch, status_entries, log_entries) = match self.engine.as_str() {
            "hg" => get_hg_info(&self.repo_path),
            _ => get_git_info(&self.repo_path),
        };

        build_vcs_content(&self.engine, branch, status_entries, log_entries)
    }
}

fn build_vcs_content(
    engine: &str,
    branch: String,
    status_entries: Vec<(String, String)>,
    log_entries: Vec<(String, String, String, String)>,
) -> WidgetContent {
        let mut modified = 0usize;
        let mut added = 0usize;
        let mut deleted = 0usize;
        let mut untracked = 0usize;
        for (state, _) in &status_entries {
            match state.as_str() {
                "modified" => modified += 1,
                "added" => added += 1,
                "deleted" => deleted += 1,
                "untracked" => untracked += 1,
                _ => {}
            }
        }

        let mut summary_parts = Vec::new();
        if modified > 0 {
            summary_parts.push(format!("{modified} modified"));
        }
        if added > 0 {
            summary_parts.push(format!("{added} added"));
        }
        if deleted > 0 {
            summary_parts.push(format!("{deleted} deleted"));
        }
        if untracked > 0 {
            summary_parts.push(format!("{untracked} untracked"));
        }

        let status_summary = if summary_parts.is_empty() {
            "clean".to_string()
        } else {
            summary_parts.join(", ")
        };

        let status_color = if summary_parts.is_empty() {
            slate_plugin_sdk::Color::Green
        } else {
            slate_plugin_sdk::Color::Yellow
        };

        let mut pairs = vec![
            (
                "Engine".to_string(),
                slate_plugin_sdk::Cell::plain(engine.to_string()),
            ),
            (
                "Branch".to_string(),
                slate_plugin_sdk::Cell::plain(if branch.is_empty() {
                    "(detached)".to_string()
                } else {
                    branch
                }),
            ),
            (
                "Status".to_string(),
                slate_plugin_sdk::Cell::colored(status_summary, status_color),
            ),
        ];

        for (i, (hash, message, author, date)) in log_entries.iter().take(5).enumerate() {
            let key = if i == 0 {
                "Last commit".to_string()
            } else {
                format!("Recent {}", i + 1)
            };
            let mut val = format!("{} {}", hash, message);
            if !author.is_empty() || !date.is_empty() {
                let extra: Vec<&str> = [author.as_str(), date.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect();
                val.push_str(&format!(" ({})", extra.join(" • ")));
            }
            pairs.push((key, slate_plugin_sdk::Cell::plain(val)));
        }

        if log_entries.is_empty() {
            pairs.push((
                "Last commit".to_string(),
                slate_plugin_sdk::Cell::plain("No commits available".to_string()),
            ));
        }

        WidgetContent::KeyValue { pairs }
}

fn get_git_info(
    repo_path: &str,
) -> (
    String,
    Vec<(String, String)>,
    Vec<(String, String, String, String)>,
) {
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let status: Vec<(String, String)> = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| l.len() >= 3)
                .map(|line| {
                    let state = match &line[..2] {
                        " M" | "M " | "MM" => "modified",
                        "A " | "AM" => "added",
                        " D" | "D " => "deleted",
                        "??" => "untracked",
                        _ => "other",
                    };
                    (state.to_string(), line[3..].to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    let log: Vec<(String, String, String, String)> = Command::new("git")
        .args(["log", "--oneline", "-10", "--format=%h|%s|%an|%ar"])
        .current_dir(repo_path)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|line| {
                    let parts: Vec<&str> = line.splitn(4, '|').collect();
                    (
                        parts.first().unwrap_or(&"").to_string(),
                        parts.get(1).unwrap_or(&"").to_string(),
                        parts.get(2).unwrap_or(&"").to_string(),
                        parts.get(3).unwrap_or(&"").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    (branch, status, log)
}

fn get_hg_info(
    repo_path: &str,
) -> (
    String,
    Vec<(String, String)>,
    Vec<(String, String, String, String)>,
) {
    let branch = Command::new("hg")
        .args(["branch"])
        .current_dir(repo_path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "default".to_string());

    let status: Vec<(String, String)> = Command::new("hg")
        .args(["status"])
        .current_dir(repo_path)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| l.len() >= 2)
                .map(|line| {
                    let state = match line.chars().next().unwrap_or(' ') {
                        'M' => "modified",
                        'A' => "added",
                        'R' => "deleted",
                        '?' => "untracked",
                        _ => "other",
                    };
                    (state.to_string(), line.get(2..).unwrap_or("").to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    let log: Vec<(String, String, String, String)> = Command::new("hg")
        .args([
            "log",
            "-l",
            "10",
            "--template",
            "{short(node)}|{desc|firstline}|{author|user}|{date|age}\n",
        ])
        .current_dir(repo_path)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|line| {
                    let parts: Vec<&str> = line.splitn(4, '|').collect();
                    (
                        parts.first().unwrap_or(&"").to_string(),
                        parts.get(1).unwrap_or(&"").to_string(),
                        parts.get(2).unwrap_or(&"").to_string(),
                        parts.get(3).unwrap_or(&"").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    (branch, status, log)
}

#[cfg(test)]
mod tests {
    use super::{build_vcs_content, VcsWidget};
    use slate_plugin_sdk::{Position, Widget, WidgetConfig, WidgetContent};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn test_widget_config() -> WidgetConfig {
        WidgetConfig {
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            refresh_interval: None,
        }
    }

    fn test_widget_config_with(settings: HashMap<String, serde_json::Value>) -> WidgetConfig {
        WidgetConfig {
            settings,
            ..test_widget_config()
        }
    }

    #[test]
    fn vcs_widget_returns_configuration_message_for_default_repo_path() {
        let mut widget = VcsWidget::new(test_widget_config());
        let metadata = widget.metadata();
        assert_eq!(metadata.name, "VCS (git)");
        assert_eq!(metadata.description, "Version control status");

        match widget.refresh() {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("Configure repo_path in settings"));
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn vcs_widget_new_uses_configured_engine_and_repo_path() {
        let widget = VcsWidget::new(test_widget_config_with(HashMap::from([
            ("engine".to_string(), serde_json::json!("hg")),
            ("repo_path".to_string(), serde_json::json!("C:\\repo")),
        ])));

        assert_eq!(widget.engine, "hg");
        assert_eq!(widget.repo_path, "C:\\repo");
    }

    #[test]
    fn vcs_widget_metadata_uses_engine_name() {
        let widget = VcsWidget::new(test_widget_config_with(HashMap::from([(
            "engine".to_string(),
            serde_json::json!("hg"),
        )])));

        assert_eq!(widget.metadata().name, "VCS (hg)");
    }

    #[test]
    fn vcs_widget_returns_configuration_message_for_empty_repo_path() {
        let mut widget = VcsWidget::new(test_widget_config_with(HashMap::from([(
            "repo_path".to_string(),
            serde_json::json!("   "),
        )])));

        match widget.refresh() {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("Configure repo_path in settings"));
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn vcs_widget_returns_error_for_nonexistent_repo_path() {
        let temp = tempdir().unwrap();
        let missing_path = temp.path().join("missing-repo");
        let mut widget = VcsWidget::new(test_widget_config_with(HashMap::from([(
            "repo_path".to_string(),
            serde_json::json!(missing_path.to_string_lossy().to_string()),
        )])));

        match widget.refresh() {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("Repo path not found"));
                assert!(content.contains("missing-repo"));
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn vcs_widget_init_updates_engine_and_repo_path() {
        let mut widget = VcsWidget::new(test_widget_config());
        widget.init(test_widget_config_with(HashMap::from([
            ("engine".to_string(), serde_json::json!("hg")),
            ("repo_path".to_string(), serde_json::json!("C:\\repos\\project")),
        ])));

        assert_eq!(widget.engine, "hg");
        assert_eq!(widget.repo_path, "C:\\repos\\project");
    }

    #[test]
    fn build_vcs_content_summarizes_status_and_recent_commits() {
        let content = build_vcs_content(
            "git",
            String::new(),
            vec![
                ("modified".to_string(), "src/main.rs".to_string()),
                ("added".to_string(), "src/lib.rs".to_string()),
                ("deleted".to_string(), "README.md".to_string()),
                ("untracked".to_string(), "notes.txt".to_string()),
            ],
            vec![
                (
                    "abc123".to_string(),
                    "Fix widget".to_string(),
                    "Sean".to_string(),
                    "2 hours ago".to_string(),
                ),
                (
                    "def456".to_string(),
                    "Add tests".to_string(),
                    String::new(),
                    String::new(),
                ),
            ],
        );

        match content {
            WidgetContent::KeyValue { pairs } => {
                let map: HashMap<_, _> = pairs
                    .into_iter()
                    .map(|(key, value)| (key, value.text))
                    .collect();
                assert_eq!(map.get("Engine").map(String::as_str), Some("git"));
                assert_eq!(map.get("Branch").map(String::as_str), Some("(detached)"));
                assert_eq!(
                    map.get("Status").map(String::as_str),
                    Some("1 modified, 1 added, 1 deleted, 1 untracked")
                );
                assert_eq!(
                    map.get("Last commit").map(String::as_str),
                    Some("abc123 Fix widget (Sean • 2 hours ago)")
                );
                assert_eq!(map.get("Recent 2").map(String::as_str), Some("def456 Add tests"));
            }
            other => panic!("expected key-value content, got {other:?}"),
        }
    }

    #[test]
    fn build_vcs_content_handles_clean_repo_without_commits() {
        let content = build_vcs_content("hg", "default".to_string(), vec![], vec![]);

        match content {
            WidgetContent::KeyValue { pairs } => {
                let map: HashMap<_, _> = pairs
                    .into_iter()
                    .map(|(key, value)| (key, value.text))
                    .collect();
                assert_eq!(map.get("Engine").map(String::as_str), Some("hg"));
                assert_eq!(map.get("Branch").map(String::as_str), Some("default"));
                assert_eq!(map.get("Status").map(String::as_str), Some("clean"));
                assert_eq!(
                    map.get("Last commit").map(String::as_str),
                    Some("No commits available")
                );
            }
            other => panic!("expected key-value content, got {other:?}"),
        }
    }
}
