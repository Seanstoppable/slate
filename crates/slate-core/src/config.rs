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
        if path.exists() {
            Self::load_from(&path)
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
            result = format!("{}{}{}", &result[..start], replacement, &result[start + end + 1..]);
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
        assert_eq!(config.widget[1].widget_type, "github.com/slate-community/slate-github");
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
}
