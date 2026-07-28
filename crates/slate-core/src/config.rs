use anyhow::{Context, Result};
use serde::Deserialize;
use slate_plugin_sdk::Position;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level slate configuration parsed from slate.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct SlateConfig {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default = "Vec::new")]
    pub widget: Vec<WidgetEntry>,
    #[serde(default)]
    pub updates: UpdateConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_refresh")]
    pub refresh_interval: u64,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            refresh_interval: default_refresh(),
        }
    }
}

fn default_refresh() -> u64 {
    300
}

#[derive(Debug, Clone, Deserialize)]
pub struct LayoutConfig {
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default = "default_cols")]
    pub cols: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            rows: default_rows(),
            cols: default_cols(),
        }
    }
}

fn default_rows() -> u16 {
    2
}

fn default_cols() -> u16 {
    2
}

/// A widget entry in the config file.
#[derive(Debug, Clone, Deserialize)]
pub struct WidgetEntry {
    /// Widget type identifier: "builtin:name", "github.com/owner/repo", or "lua:path"
    #[serde(rename = "type")]
    pub widget_type: String,
    pub position: Position,
    #[serde(default)]
    pub refresh_interval: Option<u64>,
    /// All extra keys become widget-specific settings.
    #[serde(flatten)]
    pub settings: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "default_check_interval")]
    pub check_interval: String,
    #[serde(default = "default_true")]
    pub notify: bool,
    #[serde(default)]
    pub auto_update: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            check_interval: default_check_interval(),
            notify: true,
            auto_update: false,
        }
    }
}

fn default_check_interval() -> String {
    "daily".to_string()
}

fn default_true() -> bool {
    true
}

impl SlateConfig {
    /// Load config from the default path (~/.config/slate/slate.toml)
    pub fn load_default() -> Result<Self> {
        let path = Self::default_path()?;
        Self::load_default_from_path(&path)
    }

    fn load_default_from_path(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load_from(path)
        } else {
            Ok(Self::default())
        }
    }

    /// Load config from a specific file path.
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        Self::parse(&content)
    }

    /// Parse config from a TOML string.
    pub fn parse(content: &str) -> Result<Self> {
        let mut config: SlateConfig =
            toml::from_str(content).context("Failed to parse slate.toml")?;
        // Interpolate environment variables in settings
        for widget in &mut config.widget {
            interpolate_env_vars(&mut widget.settings);
        }
        Ok(config)
    }

    /// Default config file path.
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not determine config directory")?;
        Ok(config_dir.join("slate").join("slate.toml"))
    }
}

impl Default for SlateConfig {
    fn default() -> Self {
        Self {
            global: GlobalConfig::default(),
            layout: LayoutConfig::default(),
            widget: vec![],
            updates: UpdateConfig::default(),
        }
    }
}

/// Interpolate ${ENV_VAR} patterns in TOML values.
fn interpolate_env_vars(settings: &mut HashMap<String, toml::Value>) {
    for value in settings.values_mut() {
        interpolate_value(value);
    }
}

fn interpolate_value(value: &mut toml::Value) {
    match value {
        toml::Value::String(s) => {
            *s = interpolate_string(s);
        }
        toml::Value::Array(arr) => {
            for item in arr {
                interpolate_value(item);
            }
        }
        toml::Value::Table(table) => {
            let keys: Vec<String> = table.keys().cloned().collect();
            for key in keys {
                if let Some(v) = table.get_mut(&key) {
                    interpolate_value(v);
                }
            }
        }
        _ => {}
    }
}

fn interpolate_string(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let replacement = std::env::var(var_name).unwrap_or_default();
            result = format!(
                "{}{}{}",
                &result[..start],
                replacement,
                &result[start + end + 1..]
            );
        } else {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let config = SlateConfig::parse("").unwrap();
        assert_eq!(config.global.refresh_interval, 300);
        assert_eq!(config.layout.rows, 2);
        assert_eq!(config.layout.cols, 2);
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
[global]
refresh_interval = 60

[layout]
rows = 3
cols = 3

[[widget]]
type = "builtin:resource_usage"
position = { row = 0, col = 0 }

[[widget]]
type = "github.com/slate-community/slate-github"
position = { row = 0, col = 1 }
token = "test-token"
repos = ["myorg/myrepo"]
"#;
        let config = SlateConfig::parse(toml).unwrap();
        assert_eq!(config.global.refresh_interval, 60);
        assert_eq!(config.widget.len(), 2);
        assert_eq!(config.widget[0].widget_type, "builtin:resource_usage");
        assert_eq!(
            config.widget[1].widget_type,
            "github.com/slate-community/slate-github"
        );
    }

    #[test]
    fn test_env_interpolation() {
        std::env::set_var("SLATE_TEST_VAR", "hello");
        let toml = r#"
[[widget]]
type = "test"
position = { row = 0, col = 0 }
value = "${SLATE_TEST_VAR}"
"#;
        let config = SlateConfig::parse(toml).unwrap();
        let val = config.widget[0].settings.get("value").unwrap();
        assert_eq!(val.as_str().unwrap(), "hello");
        std::env::remove_var("SLATE_TEST_VAR");
    }

    #[test]
    fn test_invalid_toml_returns_error() {
        let err = SlateConfig::parse("[global\nrefresh_interval = 60").unwrap_err();
        assert!(err.to_string().contains("Failed to parse slate.toml"));
    }

    #[test]
    fn test_missing_required_widget_fields_return_error() {
        let missing_type = r#"
[[widget]]
position = { row = 0, col = 0 }
"#;
        assert!(SlateConfig::parse(missing_type).is_err());

        let missing_position = r#"
[[widget]]
type = "builtin:resource_usage"
"#;
        assert!(SlateConfig::parse(missing_position).is_err());
    }

    #[test]
    fn test_position_parsing_uses_explicit_and_default_spans() {
        let toml = r#"
[[widget]]
type = "builtin:resource_usage"
position = { row = 1, col = 2, row_span = 3, col_span = 4 }

[[widget]]
type = "builtin:resource_usage"
position = { row = 0, col = 1 }
"#;
        let config = SlateConfig::parse(toml).unwrap();

        assert_eq!(config.widget[0].position.row, 1);
        assert_eq!(config.widget[0].position.col, 2);
        assert_eq!(config.widget[0].position.row_span, 3);
        assert_eq!(config.widget[0].position.col_span, 4);

        assert_eq!(config.widget[1].position.row_span, 1);
        assert_eq!(config.widget[1].position.col_span, 1);
    }

    #[test]
    fn test_widget_settings_collect_extra_keys() {
        let toml = r#"
[[widget]]
type = "builtin:resource_usage"
position = { row = 0, col = 0 }
title = "Resources"
show_swap = true
thresholds = [50, 80]
nested = { color = "green", compact = false }
"#;
        let config = SlateConfig::parse(toml).unwrap();
        let settings = &config.widget[0].settings;

        assert_eq!(
            settings.get("title").and_then(toml::Value::as_str),
            Some("Resources")
        );
        assert_eq!(
            settings.get("show_swap").and_then(toml::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            settings
                .get("thresholds")
                .and_then(toml::Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            settings
                .get("nested")
                .and_then(toml::Value::as_table)
                .and_then(|table| table.get("color"))
                .and_then(toml::Value::as_str),
            Some("green")
        );
    }

    #[test]
    fn test_widget_refresh_interval_override_is_parsed() {
        let toml = r#"
[global]
refresh_interval = 300

[[widget]]
type = "builtin:resource_usage"
position = { row = 0, col = 0 }
refresh_interval = 30

[[widget]]
type = "builtin:resource_usage"
position = { row = 0, col = 1 }
"#;
        let config = SlateConfig::parse(toml).unwrap();

        assert_eq!(config.global.refresh_interval, 300);
        assert_eq!(config.widget[0].refresh_interval, Some(30));
        assert_eq!(config.widget[1].refresh_interval, None);
    }

    #[test]
    fn test_interpolate_string_handles_multiple_variables_and_unclosed_patterns() {
        std::env::set_var("SLATE_MULTI_ONE", "alpha");
        std::env::set_var("SLATE_MULTI_TWO", "beta");

        assert_eq!(
            interpolate_string("${SLATE_MULTI_ONE}-${SLATE_MULTI_TWO}-${MISSING}"),
            "alpha-beta-"
        );
        assert_eq!(
            interpolate_string("prefix ${SLATE_MULTI_ONE"),
            "prefix ${SLATE_MULTI_ONE"
        );

        std::env::remove_var("SLATE_MULTI_ONE");
        std::env::remove_var("SLATE_MULTI_TWO");
    }

    #[test]
    fn test_default_path_and_load_default_use_redirected_config_directory() {
        let default_path = SlateConfig::default_path().unwrap();
        assert!(default_path.ends_with(std::path::Path::new("slate").join("slate.toml")));
        assert!(SlateConfig::load_default().is_ok());
    }

    #[test]
    fn test_load_default_from_path_uses_default_when_file_is_missing() {
        let missing = std::env::temp_dir()
            .join(format!("slate-config-missing-{}", std::process::id()))
            .join("slate.toml");

        let config = SlateConfig::load_default_from_path(&missing).unwrap();

        assert_eq!(config.global.refresh_interval, 300);
        assert!(config.updates.notify);
    }

    #[test]
    fn test_load_default_from_path_reads_existing_file() {
        let dir =
            std::env::temp_dir().join(format!("slate-config-existing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("slate.toml");
        std::fs::write(
            &path,
            r#"
[global]
refresh_interval = 42
"#,
        )
        .unwrap();

        let config = SlateConfig::load_default_from_path(&path).unwrap();

        assert_eq!(config.global.refresh_interval, 42);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_update_config_defaults_notify_when_omitted() {
        let config = SlateConfig::parse(
            r#"
[updates]
check_interval = "weekly"
"#,
        )
        .unwrap();

        assert_eq!(config.updates.check_interval, "weekly");
        assert!(config.updates.notify);
        assert!(!config.updates.auto_update);
    }
}
