use anyhow::{Context, Result};
use serde::Deserialize;
use slate_plugin_sdk::Position;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level slate configuration parsed from slate.toml.
#[derive(Debug, Clone, Deserialize, Default)]
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

/// A non-fatal configuration problem worth surfacing to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigWarning {
    /// Widget sits outside the declared grid and will not be rendered.
    OutOfBounds {
        index: usize,
        widget_type: String,
        row: u16,
        col: u16,
        rows: u16,
        cols: u16,
    },
    /// Widget declared a zero span, which is clamped to 1.
    ZeroSpan {
        index: usize,
        widget_type: String,
        row_span: u16,
        col_span: u16,
    },
    /// Widget spans past the grid edge and will be truncated.
    SpanOverflow {
        index: usize,
        widget_type: String,
        row: u16,
        col: u16,
        row_span: u16,
        col_span: u16,
        rows: u16,
        cols: u16,
    },
}

impl ConfigWarning {
    /// Zero-based index of the offending `[[widget]]` entry.
    pub fn widget_index(&self) -> usize {
        match self {
            Self::OutOfBounds { index, .. }
            | Self::ZeroSpan { index, .. }
            | Self::SpanOverflow { index, .. } => *index,
        }
    }
}

impl std::fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfBounds {
                widget_type,
                row,
                col,
                rows,
                cols,
                ..
            } => write!(
                f,
                "'{widget_type}' at position ({row},{col}) is outside the {rows}x{cols} grid and will not be rendered"
            ),
            Self::ZeroSpan {
                widget_type,
                row_span,
                col_span,
                ..
            } => write!(
                f,
                "'{widget_type}' declares row_span = {row_span}, col_span = {col_span}; zero spans are treated as 1"
            ),
            Self::SpanOverflow {
                widget_type,
                row,
                col,
                row_span,
                col_span,
                rows,
                cols,
                ..
            } => write!(
                f,
                "'{widget_type}' at ({row},{col}) spans {row_span}x{col_span}, past the {rows}x{cols} grid edge; it will be truncated"
            ),
        }
    }
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
        config.validate()?;
        Ok(config)
    }

    /// Validate structural invariants that the type system can't express.
    pub fn validate(&self) -> Result<()> {
        if self.layout.rows < 1 {
            anyhow::bail!(
                "Invalid config: [layout] rows must be at least 1 (got {})",
                self.layout.rows
            );
        }
        if self.layout.cols < 1 {
            anyhow::bail!(
                "Invalid config: [layout] cols must be at least 1 (got {})",
                self.layout.cols
            );
        }
        Ok(())
    }

    /// Non-fatal configuration problems, in widget declaration order.
    ///
    /// These describe widgets that will still load but won't render as written,
    /// so they are surfaced to the user rather than failing the config outright.
    pub fn warnings(&self) -> Vec<ConfigWarning> {
        let mut warnings = Vec::new();
        for (index, widget) in self.widget.iter().enumerate() {
            let pos = &widget.position;
            let widget_type = widget.widget_type.clone();

            if pos.row >= self.layout.rows || pos.col >= self.layout.cols {
                warnings.push(ConfigWarning::OutOfBounds {
                    index,
                    widget_type: widget_type.clone(),
                    row: pos.row,
                    col: pos.col,
                    rows: self.layout.rows,
                    cols: self.layout.cols,
                });
                // Span warnings would be noise for a widget that can't be placed.
                continue;
            }

            if pos.row_span == 0 || pos.col_span == 0 {
                warnings.push(ConfigWarning::ZeroSpan {
                    index,
                    widget_type: widget_type.clone(),
                    row_span: pos.row_span,
                    col_span: pos.col_span,
                });
            }

            let row_span = pos.row_span.max(1);
            let col_span = pos.col_span.max(1);
            if pos.row.saturating_add(row_span) > self.layout.rows
                || pos.col.saturating_add(col_span) > self.layout.cols
            {
                warnings.push(ConfigWarning::SpanOverflow {
                    index,
                    widget_type,
                    row: pos.row,
                    col: pos.col,
                    row_span,
                    col_span,
                    rows: self.layout.rows,
                    cols: self.layout.cols,
                });
            }
        }
        warnings
    }

    /// Default config file path.
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = default_config_dir()?;
        Ok(config_dir.join("slate").join("slate.toml"))
    }
}

#[cfg(target_os = "windows")]
fn default_config_dir() -> Result<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .context("Could not determine APPDATA")
}

#[cfg(not(target_os = "windows"))]
fn default_config_dir() -> Result<PathBuf> {
    dirs::config_dir().context("Could not determine config directory")
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
    fn test_warnings_flags_out_of_bounds_widget() {
        let config = SlateConfig::parse(
            "[layout]\nrows = 2\ncols = 2\n\n[[widget]]\ntype = \"builtin:clock\"\nposition = { row = 5, col = 0 }\n",
        )
        .unwrap();
        let warnings = config.warnings();
        assert_eq!(warnings.len(), 1);
        assert!(matches!(
            warnings[0],
            ConfigWarning::OutOfBounds { row: 5, .. }
        ));
        assert_eq!(warnings[0].widget_index(), 0);
        assert!(warnings[0].to_string().contains("outside the 2x2 grid"));
    }

    #[test]
    fn test_warnings_flags_zero_span() {
        let config = SlateConfig::parse(
            "[layout]\nrows = 2\ncols = 2\n\n[[widget]]\ntype = \"builtin:clock\"\nposition = { row = 0, col = 0, row_span = 0, col_span = 1 }\n",
        )
        .unwrap();
        let warnings = config.warnings();
        assert_eq!(warnings.len(), 1);
        assert!(matches!(warnings[0], ConfigWarning::ZeroSpan { .. }));
        assert!(warnings[0].to_string().contains("treated as 1"));
    }

    #[test]
    fn test_warnings_flags_span_overflow() {
        let config = SlateConfig::parse(
            "[layout]\nrows = 2\ncols = 2\n\n[[widget]]\ntype = \"builtin:clock\"\nposition = { row = 1, col = 0, row_span = 4, col_span = 1 }\n",
        )
        .unwrap();
        let warnings = config.warnings();
        assert_eq!(warnings.len(), 1);
        assert!(matches!(warnings[0], ConfigWarning::SpanOverflow { .. }));
        assert!(warnings[0].to_string().contains("truncated"));
    }

    #[test]
    fn test_warnings_skips_span_checks_for_out_of_bounds_widget() {
        // An unplaceable widget shouldn't also emit span noise.
        let config = SlateConfig::parse(
            "[layout]\nrows = 2\ncols = 2\n\n[[widget]]\ntype = \"builtin:clock\"\nposition = { row = 9, col = 9, row_span = 0, col_span = 0 }\n",
        )
        .unwrap();
        let warnings = config.warnings();
        assert_eq!(warnings.len(), 1);
        assert!(matches!(warnings[0], ConfigWarning::OutOfBounds { .. }));
    }

    #[test]
    fn test_warnings_empty_for_valid_layout() {
        let config = SlateConfig::parse(
            "[layout]\nrows = 2\ncols = 2\n\n[[widget]]\ntype = \"builtin:clock\"\nposition = { row = 0, col = 0, row_span = 2, col_span = 2 }\n",
        )
        .unwrap();
        assert!(config.warnings().is_empty());
    }

    #[test]
    fn test_warnings_reports_index_of_each_offending_widget() {
        let config = SlateConfig::parse(
            "[layout]\nrows = 2\ncols = 2\n\n[[widget]]\ntype = \"builtin:clock\"\nposition = { row = 0, col = 0 }\n\n[[widget]]\ntype = \"builtin:power\"\nposition = { row = 3, col = 0 }\n",
        )
        .unwrap();
        let warnings = config.warnings();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].widget_index(), 1);
    }

    #[test]
    fn test_warnings_span_overflow_saturates_on_huge_span() {
        let config = SlateConfig::parse(
            "[layout]\nrows = 2\ncols = 2\n\n[[widget]]\ntype = \"builtin:clock\"\nposition = { row = 1, col = 1, row_span = 65535, col_span = 65535 }\n",
        )
        .unwrap();
        assert_eq!(config.warnings().len(), 1);
    }

    #[test]
    fn test_parse_rejects_zero_rows() {
        let err = SlateConfig::parse("[layout]\nrows = 0\ncols = 3\n").unwrap_err();
        assert!(
            err.to_string().contains("rows must be at least 1"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_rejects_zero_cols() {
        let err = SlateConfig::parse("[layout]\nrows = 3\ncols = 0\n").unwrap_err();
        assert!(
            err.to_string().contains("cols must be at least 1"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_accepts_single_cell_layout() {
        let config = SlateConfig::parse("[layout]\nrows = 1\ncols = 1\n").unwrap();
        assert_eq!(config.layout.rows, 1);
        assert_eq!(config.layout.cols, 1);
        assert!(config.validate().is_ok());
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

    #[cfg(target_os = "windows")]
    #[test]
    fn default_config_dir_uses_appdata_environment_variable() {
        assert_eq!(
            default_config_dir().unwrap(),
            PathBuf::from(std::env::var_os("APPDATA").unwrap())
        );
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
