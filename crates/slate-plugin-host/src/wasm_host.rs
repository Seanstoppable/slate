use anyhow::{Context, Result};
use extism::{Manifest, Plugin, Wasm};
use slate_plugin_sdk::{
    Permissions, WidgetConfig, WidgetContent, WidgetMetadata,
};
use std::path::Path;

use crate::permissions::PermissionGuard;

/// A WASM plugin loaded via Extism.
pub struct WasmPlugin {
    metadata: WidgetMetadata,
    #[allow(dead_code)]
    permissions: PermissionGuard,
    plugin: Plugin,
    config: Option<WidgetConfig>,
}

impl WasmPlugin {
    /// Load a WASM plugin from a file path.
    pub fn from_file(path: &Path, permissions: Permissions) -> Result<Self> {
        let wasm_bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read WASM file: {}", path.display()))?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let wasm = Wasm::data(wasm_bytes);
        let manifest = Manifest::new([wasm]).with_allowed_hosts(["*".to_string()].into_iter());
        let mut plugin = Plugin::new(&manifest, [], true)
            .with_context(|| format!("Failed to create WASM plugin: {}", path.display()))?;

        // Try to get metadata from the plugin
        let metadata = match plugin.call::<&str, String>("metadata", "") {
            Ok(json_str) => {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    WidgetMetadata {
                        name: meta["name"].as_str().unwrap_or(&name).to_string(),
                        description: meta["description"].as_str().unwrap_or("").to_string(),
                        version: meta["version"].as_str().unwrap_or("0.1.0").to_string(),
                        author: meta["author"].as_str().map(String::from),
                        homepage: meta["homepage"].as_str().map(String::from),
                    }
                } else {
                    default_metadata(&name)
                }
            }
            Err(_) => default_metadata(&name),
        };

        Ok(Self {
            metadata,
            permissions: PermissionGuard::new(permissions),
            plugin,
            config: None,
        })
    }

    /// Load a WASM plugin from raw bytes.
    pub fn from_bytes(
        bytes: Vec<u8>,
        metadata: WidgetMetadata,
        permissions: Permissions,
    ) -> Result<Self> {
        let wasm = Wasm::data(bytes);
        let manifest = Manifest::new([wasm]).with_allowed_hosts(["*".to_string()].into_iter());
        let plugin = Plugin::new(&manifest, [], true)?;

        Ok(Self {
            metadata,
            permissions: PermissionGuard::new(permissions),
            plugin,
            config: None,
        })
    }
}

fn default_metadata(name: &str) -> WidgetMetadata {
    WidgetMetadata {
        name: name.to_string(),
        description: String::new(),
        version: "0.1.0".to_string(),
        author: None,
        homepage: None,
    }
}

fn parse_widget_content(json_str: &str) -> WidgetContent {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return WidgetContent::Text {
            content: json_str.to_string(),
            scrollable: false,
            wrap: true,
        };
    };

    let content_type = val["type"].as_str().unwrap_or("text");
    match content_type {
        "text" => WidgetContent::Text {
            content: val["content"].as_str().unwrap_or("").to_string(),
            scrollable: val["scrollable"].as_bool().unwrap_or(false),
            wrap: val["wrap"].as_bool().unwrap_or(true),
        },
        "key_value" => {
            let pairs = val["pairs"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let key = item["key"].as_str()?;
                            let value = item["value"].as_str().unwrap_or("");
                            Some((
                                key.to_string(),
                                slate_plugin_sdk::Cell::plain(value.to_string()),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            WidgetContent::KeyValue { pairs }
        }
        "list" => {
            let items = val["items"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let title = item["title"].as_str().or_else(|| item["text"].as_str()).or_else(|| item.as_str()).unwrap_or("");
                            Some(slate_plugin_sdk::ListItem {
                                id: item["id"].as_str().unwrap_or("").to_string(),
                                title: title.to_string(),
                                subtitle: item["subtitle"].as_str().or_else(|| item["secondary"].as_str()).map(String::from),
                                icon: item["icon"].as_str().map(String::from),
                                style: Default::default(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            WidgetContent::List {
                items,
                selectable: val["selectable"].as_bool().unwrap_or(false),
                actions: vec![],
            }
        }
        _ => WidgetContent::Text {
            content: val["content"].as_str().unwrap_or(json_str).to_string(),
            scrollable: false,
            wrap: true,
        },
    }
}

/// Format local time from unix timestamp. Returns (time, date, timezone).
fn format_local_time(_epoch_secs: i64) -> (String, String, String) {
    use chrono::Local;
    let now = Local::now();
    let time = now.format("%H:%M:%S").to_string();
    let date = now.format("%A, %B %d, %Y").to_string();
    let tz = now.format("%Z (UTC%:z)").to_string();
    (time, date, tz)
}

impl slate_plugin_sdk::Widget for WasmPlugin {
    fn metadata(&self) -> WidgetMetadata {
        self.metadata.clone()
    }

    fn init(&mut self, config: WidgetConfig) {
        self.config = Some(config);
    }

    fn refresh(&mut self) -> WidgetContent {
        // Build input from config settings, injecting host-provided values
        let mut settings = self
            .config
            .as_ref()
            .map(|c| c.settings.clone())
            .unwrap_or_default();

        // Inject current time for plugins that need it
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs() as i64;

        // Format time using simple calculation (UTC offset handled by chrono in host)
        let time_str = format_local_time(secs);
        settings.insert("current_time".to_string(), serde_json::json!(time_str.0));
        settings.insert("current_date".to_string(), serde_json::json!(time_str.1));
        settings.insert("timezone".to_string(), serde_json::json!(time_str.2));

        let input = serde_json::to_string(&settings).unwrap_or_default();

        match self.plugin.call::<&str, String>("refresh", &input) {
            Ok(json_str) => parse_widget_content(&json_str),
            Err(e) => WidgetContent::Text {
                content: format!("[{}] Error: {}", self.metadata.name, e),
                scrollable: false,
                wrap: true,
            },
        }
    }

    fn on_key(&mut self, key: &str, action: &str) {
        let input = serde_json::json!({ "key": key, "action": action }).to_string();
        let _ = self.plugin.call::<&str, String>("on_key", &input);
    }

    fn on_action(&mut self, action_id: &str, item_id: &str) -> Option<slate_plugin_sdk::WidgetAction> {
        let input = serde_json::json!({ "action_id": action_id, "item_id": item_id }).to_string();
        match self.plugin.call::<&str, String>("on_action", &input) {
            Ok(response) => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&response) {
                    if let Some(url) = val["open_url"].as_str() {
                        return Some(slate_plugin_sdk::WidgetAction::OpenUrl(url.to_string()));
                    }
                    if let Some(msg) = val["notify"].as_str() {
                        return Some(slate_plugin_sdk::WidgetAction::Notify(msg.to_string()));
                    }
                }
                None
            }
            Err(_) => None,
        }
    }

    fn on_focus(&mut self) {}
    fn on_blur(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_widget_content_handles_text_content() {
        let content = parse_widget_content(
            r#"{"type":"text","content":"hello","scrollable":true,"wrap":false}"#,
        );

        match content {
            WidgetContent::Text {
                content,
                scrollable,
                wrap,
            } => {
                assert_eq!(content, "hello");
                assert!(scrollable);
                assert!(!wrap);
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn parse_widget_content_handles_key_value_content() {
        let content = parse_widget_content(
            r#"{"type":"key_value","pairs":[{"key":"CPU","value":"12%"},{"key":"Memory","value":"4 GB"}]}"#,
        );

        match content {
            WidgetContent::KeyValue { pairs } => {
                assert_eq!(pairs.len(), 2);
                assert_eq!(pairs[0].0, "CPU");
                assert_eq!(pairs[0].1.text, "12%");
                assert_eq!(pairs[1].0, "Memory");
                assert_eq!(pairs[1].1.text, "4 GB");
            }
            other => panic!("expected key_value content, got {other:?}"),
        }
    }

    #[test]
    fn parse_widget_content_handles_list_content() {
        let content = parse_widget_content(
            r#"{"type":"list","selectable":true,"items":[{"id":"1","title":"Issue 1","subtitle":"open","icon":"!"},{"id":"2","text":"Fallback title","secondary":"secondary"}]}"#,
        );

        match content {
            WidgetContent::List {
                items,
                selectable,
                actions,
            } => {
                assert!(selectable);
                assert!(actions.is_empty());
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].id, "1");
                assert_eq!(items[0].title, "Issue 1");
                assert_eq!(items[0].subtitle.as_deref(), Some("open"));
                assert_eq!(items[0].icon.as_deref(), Some("!"));
                assert_eq!(items[1].title, "Fallback title");
                assert_eq!(items[1].subtitle.as_deref(), Some("secondary"));
            }
            other => panic!("expected list content, got {other:?}"),
        }
    }

    #[test]
    fn parse_widget_content_handles_unknown_content_type() {
        let content = parse_widget_content(r#"{"type":"custom","content":"raw value"}"#);

        match content {
            WidgetContent::Text {
                content,
                scrollable,
                wrap,
            } => {
                assert_eq!(content, "raw value");
                assert!(!scrollable);
                assert!(wrap);
            }
            other => panic!("expected text fallback, got {other:?}"),
        }
    }

    #[test]
    fn parse_widget_content_handles_empty_json() {
        let content = parse_widget_content("{}");

        match content {
            WidgetContent::Text {
                content,
                scrollable,
                wrap,
            } => {
                assert_eq!(content, "");
                assert!(!scrollable);
                assert!(wrap);
            }
            other => panic!("expected empty text fallback, got {other:?}"),
        }
    }

    #[test]
    fn parse_widget_content_handles_malformed_json() {
        let content = parse_widget_content("not valid json");

        match content {
            WidgetContent::Text {
                content,
                scrollable,
                wrap,
            } => {
                assert_eq!(content, "not valid json");
                assert!(!scrollable);
                assert!(wrap);
            }
            other => panic!("expected raw text fallback, got {other:?}"),
        }
    }

    #[test]
    fn parse_widget_content_handles_missing_fields() {
        let key_value = parse_widget_content(r#"{"type":"key_value"}"#);
        match key_value {
            WidgetContent::KeyValue { pairs } => assert!(pairs.is_empty()),
            other => panic!("expected key_value content, got {other:?}"),
        }

        let list = parse_widget_content(r#"{"type":"list","items":[{}]}"#);
        match list {
            WidgetContent::List {
                items,
                selectable,
                actions,
            } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].id, "");
                assert_eq!(items[0].title, "");
                assert_eq!(items[0].subtitle, None);
                assert_eq!(items[0].icon, None);
                assert!(!selectable);
                assert!(actions.is_empty());
            }
            other => panic!("expected list content, got {other:?}"),
        }
    }
}
