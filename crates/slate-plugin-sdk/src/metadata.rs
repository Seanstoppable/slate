use serde::{Deserialize, Serialize};

/// Metadata describing a widget plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

/// Configuration passed to a widget at init time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetConfig {
    /// The widget's position in the grid.
    pub position: Position,
    /// Widget-specific key-value settings from the user's config.
    #[serde(default)]
    pub settings: std::collections::HashMap<String, serde_json::Value>,
    /// Refresh interval override (seconds). Falls back to global default.
    #[serde(default)]
    pub refresh_interval: Option<u64>,
}

/// Grid position for a widget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub row: u16,
    pub col: u16,
    #[serde(default = "default_span")]
    pub row_span: u16,
    #[serde(default = "default_span")]
    pub col_span: u16,
}

fn default_span() -> u16 {
    1
}

/// Permissions a plugin declares in plugin.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub network: Vec<String>,
    /// Config fields containing HTTP(S) URLs whose hosts are allowed at runtime.
    #[serde(default)]
    pub network_from_config: Vec<String>,
    #[serde(default)]
    pub exec: Vec<String>,
    #[serde(default)]
    pub system: Vec<String>,
    #[serde(default)]
    pub filesystem_read: Vec<String>,
    #[serde(default)]
    pub storage: bool,
    #[serde(default)]
    pub raw_network: bool,
    #[serde(default)]
    pub secrets: Vec<String>,
}
