use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};
use std::path::PathBuf;

pub(crate) struct LogfileWidget {
    file_path: Option<PathBuf>,
    config_error: Option<String>,
    max_lines: usize,
}

impl LogfileWidget {
    pub(crate) fn new(config: WidgetConfig) -> Self {
        let raw_path = config.settings.get("filePath").and_then(|v| v.as_str());

        let (file_path, config_error) = match raw_path.map(expand_path) {
            None => (None, None),
            Some(Ok(path)) => (Some(path), None),
            Some(Err(e)) => (None, Some(format!("{e:#}"))),
        };

        let max_lines = config
            .settings
            .get("maxLines")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        Self {
            file_path,
            config_error,
            max_lines,
        }
    }
}

impl slate_plugin_sdk::Widget for LogfileWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Log File".to_string(),
            description: "Tails a log file".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        if let Some(err) = &self.config_error {
            return WidgetContent::Text {
                content: err.clone(),
                scrollable: false,
                wrap: true,
            };
        }

        let Some(path) = &self.file_path else {
            return WidgetContent::Text {
                content: "No filePath configured".to_string(),
                scrollable: false,
                wrap: true,
            };
        };

        match tail_file(path, self.max_lines) {
            Ok(content) => WidgetContent::Text {
                content,
                scrollable: true,
                wrap: false,
            },
            Err(e) => WidgetContent::Text {
                content: format!("Error reading {}: {}", path.display(), e),
                scrollable: false,
                wrap: true,
            },
        }
    }
}

/// Read the last N lines from a file.
fn tail_file(path: &PathBuf, max_lines: usize) -> std::io::Result<String> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].join("\n"))
}

/// Expand ~ to home directory and environment variables.
///
/// Returns an error if a referenced environment variable is unset.
fn expand_path(path: &str) -> anyhow::Result<PathBuf> {
    let expanded = if path.starts_with('~') {
        if let Some(home) = dirs_next_home() {
            path.replacen('~', &home, 1)
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };

    // Expand ${VAR} patterns (shared, non-recursive implementation)
    Ok(PathBuf::from(slate_core::config::interpolate_string(
        &expanded,
    )?))
}

fn dirs_next_home() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok()
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::{Position, Widget};
    use std::collections::HashMap;
    use std::io::Write;

    fn make_config(settings: HashMap<String, serde_json::Value>) -> WidgetConfig {
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
    fn tail_file_returns_last_n_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.log");
        let mut f = std::fs::File::create(&file_path).unwrap();
        for i in 1..=100 {
            writeln!(f, "line {}", i).unwrap();
        }
        drop(f);

        let result = tail_file(&file_path, 5).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0], "line 96");
        assert_eq!(lines[4], "line 100");
    }

    #[test]
    fn tail_file_returns_all_when_fewer_than_max() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("short.log");
        std::fs::write(&file_path, "a\nb\nc").unwrap();

        let result = tail_file(&file_path, 50).unwrap();
        assert_eq!(result, "a\nb\nc");
    }

    #[test]
    fn tail_file_error_on_missing_file() {
        let result = tail_file(&PathBuf::from("/nonexistent/file.log"), 10);
        assert!(result.is_err());
    }

    #[test]
    fn expand_path_tilde() {
        let expanded = expand_path("~/logs/app.log").unwrap();
        let home = dirs_next_home().unwrap_or_default();
        assert!(expanded.to_string_lossy().starts_with(&home));
        assert!(
            expanded.to_string_lossy().ends_with("logs/app.log")
                || expanded.to_string_lossy().ends_with("logs\\app.log")
        );
    }

    #[test]
    fn expand_path_env_var() {
        std::env::set_var("SLATE_TEST_DIR", "/tmp/logs");
        let expanded = expand_path("${SLATE_TEST_DIR}/app.log").unwrap();
        assert_eq!(expanded, PathBuf::from("/tmp/logs/app.log"));
        std::env::remove_var("SLATE_TEST_DIR");
    }

    #[test]
    fn expand_path_unset_env_var_errors() {
        std::env::remove_var("SLATE_TEST_UNSET_DIR");
        let err = expand_path("${SLATE_TEST_UNSET_DIR}/app.log").unwrap_err();
        assert!(format!("{err:#}").contains("SLATE_TEST_UNSET_DIR"));
    }

    #[test]
    fn widget_surfaces_config_error_for_unset_env_var() {
        std::env::remove_var("SLATE_TEST_UNSET_DIR2");
        let mut settings = HashMap::new();
        settings.insert(
            "filePath".to_string(),
            serde_json::Value::String("${SLATE_TEST_UNSET_DIR2}/app.log".to_string()),
        );
        let mut widget = LogfileWidget::new(make_config(settings));
        match widget.refresh() {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("SLATE_TEST_UNSET_DIR2"));
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn widget_no_filepath_configured() {
        let config = make_config(HashMap::new());
        let mut widget = LogfileWidget::new(config);
        let content = widget.refresh();
        match content {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("No filePath configured"));
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn widget_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("app.log");
        std::fs::write(&file_path, "hello\nworld").unwrap();

        let mut settings = HashMap::new();
        settings.insert(
            "filePath".to_string(),
            serde_json::Value::String(file_path.to_string_lossy().to_string()),
        );
        let config = make_config(settings);
        let mut widget = LogfileWidget::new(config);
        let content = widget.refresh();
        match content {
            WidgetContent::Text {
                content,
                scrollable,
                ..
            } => {
                assert_eq!(content, "hello\nworld");
                assert!(scrollable);
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn widget_max_lines_config() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("big.log");
        let mut f = std::fs::File::create(&file_path).unwrap();
        for i in 1..=20 {
            writeln!(f, "line {}", i).unwrap();
        }
        drop(f);

        let mut settings = HashMap::new();
        settings.insert(
            "filePath".to_string(),
            serde_json::Value::String(file_path.to_string_lossy().to_string()),
        );
        settings.insert("maxLines".to_string(), serde_json::json!(3));
        let config = make_config(settings);
        let mut widget = LogfileWidget::new(config);
        let content = widget.refresh();
        match content {
            WidgetContent::Text { content, .. } => {
                let lines: Vec<&str> = content.lines().collect();
                assert_eq!(lines.len(), 3);
                assert_eq!(lines[0], "line 18");
            }
            _ => panic!("Expected Text content"),
        }
    }
}
