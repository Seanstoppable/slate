use anyhow::{Context, Result};
use extism::{Function, Manifest, Plugin, UserData, Val, Wasm, PTR};
use slate_plugin_sdk::{Permissions, WidgetAction, WidgetConfig, WidgetContent, WidgetMetadata};
use std::path::Path;
use tracing::warn;

use crate::permissions::PermissionGuard;

/// A WASM plugin loaded via Extism.
pub struct WasmPlugin {
    metadata: WidgetMetadata,
    #[allow(dead_code)]
    permissions: PermissionGuard,
    plugin: Plugin,
    config: Option<WidgetConfig>,
}

/// Create the exec_command host function for WASM plugins.
/// Plugins call this with a JSON string: {"cmd": "...", "args": ["..."]}
/// Returns JSON: {"stdout": "...", "stderr": "...", "exit_code": 0}
fn run_exec_request(input: &str) -> Result<String, extism::Error> {
    let request: serde_json::Value = serde_json::from_str(input)
        .map_err(|e| extism::Error::msg(format!("Invalid exec request JSON: {}", e)))?;

    let cmd = request["cmd"].as_str().unwrap_or("");
    let args: Vec<&str> = request["args"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let result = if cmd.is_empty() {
        serde_json::json!({
            "stdout": "",
            "stderr": "exec_command: 'cmd' field is required",
            "exit_code": 1
        })
    } else {
        match std::process::Command::new(cmd).args(&args).output() {
            Ok(out) => serde_json::json!({
                "stdout": String::from_utf8_lossy(&out.stdout).to_string(),
                "stderr": String::from_utf8_lossy(&out.stderr).to_string(),
                "exit_code": out.status.code().unwrap_or(-1)
            }),
            Err(e) => serde_json::json!({
                "stdout": "",
                "stderr": format!("Failed to execute '{}': {}", cmd, e),
                "exit_code": -1
            }),
        }
    };

    Ok(result.to_string())
}

fn make_exec_function() -> Function {
    Function::new(
        "exec_command",
        [PTR],
        [PTR],
        UserData::new(()),
        |plugin: &mut extism::CurrentPlugin, inputs: &[Val], outputs: &mut [Val], _user_data| {
            let input: String = plugin.memory_get_val(&inputs[0])?;
            let result = run_exec_request(&input)?;
            let handle = plugin.memory_new(result)?;
            outputs[0] = plugin.memory_to_val(handle);
            Ok(())
        },
    )
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
        let manifest = Manifest::new([wasm])
            .with_allowed_hosts(["*".to_string()].into_iter())
            .with_timeout(std::time::Duration::from_secs(10));

        let host_functions = [make_exec_function()];
        let mut plugin = Plugin::new(&manifest, host_functions, true)
            .with_context(|| format!("Failed to create WASM plugin: {}", path.display()))?;

        // Try to get metadata from the plugin
        let metadata = match plugin.call::<&str, String>("metadata", "") {
            Ok(json_str) => parse_widget_metadata(&json_str, &name),
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
        let manifest = Manifest::new([wasm])
            .with_allowed_hosts(["*".to_string()].into_iter())
            .with_timeout(std::time::Duration::from_secs(10));
        let host_functions = [make_exec_function()];
        let plugin = Plugin::new(&manifest, host_functions, true)?;

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

fn parse_widget_metadata(json_str: &str, fallback_name: &str) -> WidgetMetadata {
    let Ok(meta) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return default_metadata(fallback_name);
    };

    WidgetMetadata {
        name: meta["name"].as_str().unwrap_or(fallback_name).to_string(),
        description: meta["description"].as_str().unwrap_or("").to_string(),
        version: meta["version"].as_str().unwrap_or("0.1.0").to_string(),
        author: meta["author"].as_str().map(String::from),
        homepage: meta["homepage"].as_str().map(String::from),
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
                            let title = item["title"]
                                .as_str()
                                .or_else(|| item["text"].as_str())
                                .or_else(|| item.as_str())
                                .unwrap_or("");
                            Some(slate_plugin_sdk::ListItem {
                                id: item["id"].as_str().unwrap_or("").to_string(),
                                title: title.to_string(),
                                subtitle: item["subtitle"]
                                    .as_str()
                                    .or_else(|| item["secondary"].as_str())
                                    .map(String::from),
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

/// Format time in a specific timezone. Returns (time, date).
fn format_tz_time(tz_name: &str) -> (String, String) {
    use chrono::Utc;
    if let Ok(tz) = tz_name.parse::<chrono_tz::Tz>() {
        let now = Utc::now().with_timezone(&tz);
        let time = now.format("%H:%M:%S").to_string();
        let date = now.format("%a, %b %d").to_string();
        (time, date)
    } else {
        ("--:--:--".to_string(), tz_name.to_string())
    }
}

fn parse_widget_action(response: &str) -> Option<WidgetAction> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(response) else {
        return None;
    };

    if let Some(url) = val["open_url"].as_str() {
        return Some(WidgetAction::OpenUrl(url.to_string()));
    }
    if let Some(msg) = val["notify"].as_str() {
        return Some(WidgetAction::Notify(msg.to_string()));
    }
    if let Some(detail) = val["show_detail"].as_str() {
        return Some(WidgetAction::ShowDetail(detail.to_string()));
    }

    None
}

fn build_refresh_settings(
    config: Option<&WidgetConfig>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut settings = config
        .map(|c| c.settings.clone())
        .unwrap_or_default()
        .into_iter()
        .collect::<serde_json::Map<String, serde_json::Value>>();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let time_str = format_local_time(secs);
    settings.insert("current_time".to_string(), serde_json::json!(time_str.0));
    settings.insert("current_date".to_string(), serde_json::json!(time_str.1));
    settings.insert("timezone".to_string(), serde_json::json!(time_str.2));

    if let Some(locations) = settings.get("locations").cloned() {
        if let Some(locations_obj) = locations.as_object() {
            let clocks: Vec<serde_json::Value> = locations_obj
                .iter()
                .map(|(label, tz_val)| {
                    let tz_str = tz_val.as_str().unwrap_or("UTC");
                    let (time, date) = format_tz_time(tz_str);
                    serde_json::json!({
                        "label": label,
                        "time": time,
                        "date": date,
                        "zone": tz_str
                    })
                })
                .collect();
            settings.insert("clocks".to_string(), serde_json::json!(clocks));
        }
    }

    settings
}

fn widget_content_from_refresh_result(
    result: Result<String, extism::Error>,
    metadata_name: &str,
) -> WidgetContent {
    match result {
        Ok(json_str) => parse_widget_content(&json_str),
        Err(e) => {
            let msg = e.to_string();
            warn!(plugin = metadata_name, error = %msg, "Plugin refresh failed");
            // Produce a user-friendly message for common errors
            let friendly = if msg.contains("Connection refused")
                || msg.contains("connection refused")
            {
                "Connection refused.\nCheck that the service is running\nand the URL is correct."
                    .to_string()
            } else if msg.contains("timed out")
                || msg.contains("Timeout")
                || msg.contains("deadline has elapsed")
            {
                "Request timed out.\nThe host may be unreachable.".to_string()
            } else if msg.contains("certificate")
                || msg.contains("SSL")
                || msg.contains("tls")
                || msg.contains("ssl")
            {
                "TLS/SSL error.\nTry using http:// instead of https://\nunless the server has a valid certificate.".to_string()
            } else if msg.contains("wasm backtrace") || msg.contains("http::request") {
                "Network request failed.\nCheck that the URL is correct\nand the host is reachable."
                    .to_string()
            } else {
                format!("Error: {}", msg)
            };
            WidgetContent::Text {
                content: format!("[{}] {}", metadata_name, friendly),
                scrollable: false,
                wrap: true,
            }
        }
    }
}

fn widget_action_from_result(result: Result<String, extism::Error>) -> Option<WidgetAction> {
    match result {
        Ok(response) => parse_widget_action(&response),
        Err(_) => None,
    }
}

impl slate_plugin_sdk::Widget for WasmPlugin {
    fn metadata(&self) -> WidgetMetadata {
        self.metadata.clone()
    }

    fn init(&mut self, config: WidgetConfig) {
        self.config = Some(config);
    }

    fn refresh(&mut self) -> WidgetContent {
        let settings = build_refresh_settings(self.config.as_ref());
        let input = serde_json::to_string(&settings).unwrap_or_default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.plugin.call::<&str, String>("refresh", &input)
        }));
        match result {
            Ok(call_result) => {
                widget_content_from_refresh_result(call_result, &self.metadata.name)
            }
            Err(_) => {
                warn!(plugin = %self.metadata.name, "Plugin panicked during refresh");
                WidgetContent::Text {
                    content: format!("[{}] Plugin crashed during refresh", self.metadata.name),
                    scrollable: false,
                    wrap: true,
                }
            }
        }
    }

    fn on_key(&mut self, key: &str, action: &str) {
        let input = serde_json::json!({ "key": key, "action": action }).to_string();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.plugin.call::<&str, String>("on_key", &input)
        }));
    }

    fn on_action(
        &mut self,
        action_id: &str,
        item_id: &str,
    ) -> Option<slate_plugin_sdk::WidgetAction> {
        let input = serde_json::json!({ "action_id": action_id, "item_id": item_id }).to_string();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.plugin.call::<&str, String>("on_action", &input)
        }));
        match result {
            Ok(call_result) => widget_action_from_result(call_result),
            Err(_) => None,
        }
    }

    fn on_focus(&mut self) {}
    fn on_blur(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_plugin_sdk::Widget;
    use std::io::Write;
    use tempfile::tempdir;
    use tempfile::NamedTempFile;

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

    #[test]
    fn parse_widget_content_handles_string_list_items() {
        let content = parse_widget_content(r#"{"type":"list","items":["First","Second"]}"#);

        match content {
            WidgetContent::List {
                items,
                selectable,
                actions,
            } => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].title, "First");
                assert_eq!(items[1].title, "Second");
                assert!(!selectable);
                assert!(actions.is_empty());
            }
            other => panic!("expected list content, got {other:?}"),
        }
    }

    #[test]
    fn parse_widget_metadata_handles_defaults_and_explicit_fields() {
        let metadata = parse_widget_metadata(
            r#"{"name":"Clock","description":"Shows time","version":"1.2.3","author":"Slate","homepage":"https://example.com"}"#,
            "fallback",
        );

        assert_eq!(metadata.name, "Clock");
        assert_eq!(metadata.description, "Shows time");
        assert_eq!(metadata.version, "1.2.3");
        assert_eq!(metadata.author.as_deref(), Some("Slate"));
        assert_eq!(metadata.homepage.as_deref(), Some("https://example.com"));

        let defaults = parse_widget_metadata(r#"{"description":"Only description"}"#, "fallback");
        assert_eq!(defaults.name, "fallback");
        assert_eq!(defaults.description, "Only description");
        assert_eq!(defaults.version, "0.1.0");
        assert_eq!(defaults.author, None);
        assert_eq!(defaults.homepage, None);
    }

    #[test]
    fn parse_widget_metadata_falls_back_for_invalid_json() {
        let metadata = parse_widget_metadata("not json", "fallback");

        assert_eq!(metadata.name, "fallback");
        assert_eq!(metadata.description, "");
        assert_eq!(metadata.version, "0.1.0");
    }

    #[test]
    fn parse_widget_action_handles_known_responses() {
        assert_eq!(
            parse_widget_action(r#"{"open_url":"https://example.com"}"#),
            Some(WidgetAction::OpenUrl("https://example.com".to_string()))
        );
        assert_eq!(
            parse_widget_action(r#"{"notify":"Updated"}"#),
            Some(WidgetAction::Notify("Updated".to_string()))
        );
        assert_eq!(
            parse_widget_action(r#"{"show_detail":"More info"}"#),
            Some(WidgetAction::ShowDetail("More info".to_string()))
        );
        assert_eq!(parse_widget_action(r#"{"noop":true}"#), None);
        assert_eq!(parse_widget_action("not json"), None);
    }

    #[test]
    fn helper_functions_cover_exec_refresh_and_action_paths() {
        let missing_cmd = run_exec_request(r#"{}"#).unwrap();
        assert!(missing_cmd.contains("\"exit_code\":1"));
        assert!(missing_cmd.contains("cmd"));

        let bad_json = run_exec_request("not-json");
        assert!(bad_json.is_err());

        #[cfg(windows)]
        let ok = run_exec_request(r#"{"cmd":"cmd","args":["/c","echo hello"]}"#).unwrap();
        #[cfg(not(windows))]
        let ok = run_exec_request(r#"{"cmd":"echo","args":["hello"]}"#).unwrap();
        assert!(ok.contains("hello"));

        let missing = run_exec_request(r#"{"cmd":"definitely_missing_slate_command"}"#).unwrap();
        assert!(missing.contains("\"exit_code\":-1"));

        match widget_content_from_refresh_result(
            Ok(r#"{"type":"text","content":"ok"}"#.to_string()),
            "demo",
        ) {
            WidgetContent::Text { content, .. } => assert_eq!(content, "ok"),
            other => panic!("expected text content, got {other:?}"),
        }
        match widget_content_from_refresh_result(Err(extism::Error::msg("boom")), "demo") {
            WidgetContent::Text { content, .. } => assert!(content.contains("[demo] Error: boom")),
            other => panic!("expected text error, got {other:?}"),
        }

        assert_eq!(
            widget_action_from_result(Ok(r#"{"notify":"done"}"#.to_string())),
            Some(WidgetAction::Notify("done".to_string()))
        );
        assert_eq!(
            widget_action_from_result(Err(extism::Error::msg("boom"))),
            None
        );
    }

    #[test]
    fn build_refresh_settings_includes_dynamic_time_and_clocks() {
        let config = WidgetConfig {
            position: slate_plugin_sdk::Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: std::collections::HashMap::from([
                ("message".to_string(), serde_json::json!("hello")),
                (
                    "locations".to_string(),
                    serde_json::json!({"UTC": "UTC", "Bad": 42}),
                ),
            ]),
            refresh_interval: None,
        };

        let settings = build_refresh_settings(Some(&config));
        assert_eq!(settings.get("message"), Some(&serde_json::json!("hello")));
        assert!(settings.get("current_time").is_some());
        assert!(settings.get("current_date").is_some());
        assert!(settings.get("timezone").is_some());
        let clocks = settings
            .get("clocks")
            .and_then(serde_json::Value::as_array)
            .expect("clocks array");
        assert_eq!(clocks.len(), 2);
        assert_eq!(clocks[0]["label"].as_str().unwrap_or_default(), "Bad");
        assert_eq!(clocks[0]["zone"].as_str().unwrap_or_default(), "UTC");
    }

    #[test]
    fn build_refresh_settings_handles_missing_config_and_non_object_locations() {
        let empty = build_refresh_settings(None);
        assert!(empty.get("current_time").is_some());
        assert!(empty.get("current_date").is_some());
        assert!(empty.get("timezone").is_some());
        assert!(empty.get("clocks").is_none());

        let config = WidgetConfig {
            position: slate_plugin_sdk::Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: std::collections::HashMap::from([(
                "locations".to_string(),
                serde_json::json!(["UTC", "America/New_York"]),
            )]),
            refresh_interval: None,
        };

        let settings = build_refresh_settings(Some(&config));
        assert!(settings.get("locations").is_some());
        assert!(settings.get("clocks").is_none());
    }

    #[test]
    fn from_file_returns_error_for_nonexistent_file() {
        let path = std::path::Path::new("C:\\definitely-missing-slate-plugin.wasm");
        let err = match WasmPlugin::from_file(path, Permissions::default()) {
            Ok(_) => panic!("expected file load to fail"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("Failed to read WASM file"));
    }

    #[test]
    fn from_file_returns_error_for_invalid_wasm_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"this is not wasm").unwrap();

        let err = match WasmPlugin::from_file(file.path(), Permissions::default()) {
            Ok(_) => panic!("expected plugin creation to fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("Failed to create WASM plugin"));
    }

    #[test]
    fn format_tz_time_returns_time_for_valid_timezone() {
        let (time, date) = format_tz_time("America/New_York");
        // Should look like HH:MM:SS
        assert_eq!(time.len(), 8);
        assert_eq!(&time[2..3], ":");
        assert_eq!(&time[5..6], ":");
        // Date should be non-empty
        assert!(!date.is_empty());
    }

    #[test]
    fn format_tz_time_returns_fallback_for_invalid_timezone() {
        let (time, date) = format_tz_time("Not/A/Real/Zone");
        assert_eq!(time, "--:--:--");
        assert_eq!(date, "Not/A/Real/Zone");
    }

    #[test]
    fn format_local_time_returns_valid_strings() {
        let (time, date, tz) = format_local_time(0);
        assert_eq!(time.len(), 8); // HH:MM:SS
        assert!(!date.is_empty());
        assert!(!tz.is_empty());
    }

    #[test]
    fn from_file_loads_minimal_wasm_with_fallback_metadata_and_refresh_error() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("minimal.wasm");
        std::fs::write(
            &wasm_path,
            wat::parse_str(r#"(module (memory (export "memory") 1))"#).unwrap(),
        )
        .unwrap();

        let mut plugin = WasmPlugin::from_file(&wasm_path, Permissions::default()).unwrap();
        let metadata = plugin.metadata();
        assert_eq!(metadata.name, "minimal");
        assert_eq!(metadata.version, "0.1.0");

        plugin.init(WidgetConfig {
            position: slate_plugin_sdk::Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: std::collections::HashMap::from([
                (
                    "locations".to_string(),
                    serde_json::json!({"NYC": "America/New_York"}),
                ),
                ("message".to_string(), serde_json::json!("hello")),
            ]),
            refresh_interval: Some(30),
        });

        match plugin.refresh() {
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("[minimal] Error:"));
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[test]
    fn from_file_accepts_stub_exports_and_uses_success_paths() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("stub.wasm");
        std::fs::write(
            &wasm_path,
            wat::parse_str(
                r#"
                    (module
                        (memory (export "memory") 1)
                        (func (export "metadata") (result i32) (i32.const 0))
                        (func (export "refresh") (result i32) (i32.const 0))
                        (func (export "on_action") (result i32) (i32.const 0))
                    )
                "#,
            )
            .unwrap(),
        )
        .unwrap();

        let mut plugin = WasmPlugin::from_file(&wasm_path, Permissions::default()).unwrap();
        assert_eq!(plugin.metadata().name, "stub");
        assert_eq!(plugin.metadata().version, "0.1.0");

        match plugin.refresh() {
            WidgetContent::Text { content, .. } => assert_eq!(content, ""),
            other => panic!("expected text content, got {other:?}"),
        }
        assert_eq!(plugin.on_action("open", "item-1"), None);
    }

    #[test]
    fn from_bytes_loads_minimal_wasm_and_returns_none_for_actions() {
        let bytes = wat::parse_str(r#"(module (memory (export "memory") 1))"#).unwrap();
        let mut plugin = WasmPlugin::from_bytes(
            bytes,
            WidgetMetadata {
                name: "bytes".to_string(),
                description: "from bytes".to_string(),
                version: "9.9.9".to_string(),
                author: None,
                homepage: None,
            },
            Permissions::default(),
        )
        .unwrap();

        assert_eq!(plugin.metadata().name, "bytes");
        plugin.on_key("Enter", "press");
        plugin.on_focus();
        plugin.on_blur();
        assert_eq!(plugin.on_action("select", "item-1"), None);
        match plugin.refresh() {
            WidgetContent::Text { content, .. } => assert!(content.contains("[bytes] Error:")),
            other => panic!("expected text content, got {other:?}"),
        }
    }
}
