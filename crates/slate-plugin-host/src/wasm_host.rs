use anyhow::{Context, Result};
use extism::{Function, Manifest, Plugin, UserData, Val, Wasm, PTR};
use slate_plugin_sdk::{
    Action, Permissions, WidgetAction, WidgetConfig, WidgetContent, WidgetMetadata,
};
use std::path::Path;
use tracing::warn;

use crate::host_functions::PluginStore;
use crate::permissions::PermissionGuard;

#[derive(Debug)]
struct HostState {
    config: Option<WidgetConfig>,
    permissions: PermissionGuard,
    store: PluginStore,
}

/// A WASM plugin loaded via Extism.
pub struct WasmPlugin {
    metadata: WidgetMetadata,
    host_state: UserData<HostState>,
    plugin: Plugin,
    config: Option<WidgetConfig>,
}

/// Create the exec_command host function for WASM plugins.
/// Plugins call this with a JSON string: {"cmd": "...", "args": ["..."]}
/// Returns JSON: {"stdout": "...", "stderr": "...", "exit_code": 0}
fn run_exec_request(guard: &PermissionGuard, input: &str) -> Result<String, extism::Error> {
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
        guard
            .check_exec(cmd)
            .map_err(|e| extism::Error::msg(e.to_string()))?;
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

fn make_exec_function(host_state: UserData<HostState>) -> Function {
    Function::new(
        "exec_command",
        [PTR],
        [PTR],
        host_state,
        |plugin: &mut extism::CurrentPlugin, inputs: &[Val], outputs: &mut [Val], user_data| {
            let input: String = plugin.memory_get_val(&inputs[0])?;
            let state = user_data
                .get()
                .map_err(|e| extism::Error::msg(e.to_string()))?;
            let state = state
                .lock()
                .map_err(|_| extism::Error::msg("Failed to lock plugin host state"))?;
            let result = run_exec_request(&state.permissions, &input)?;
            let handle = plugin.memory_new(result)?;
            outputs[0] = plugin.memory_to_val(handle);
            Ok(())
        },
    )
}

/// Run a "safe" HTTP request that never traps: unlike Extism's built-in
/// `http_request` (which aborts the whole plugin call on DNS failures,
/// connection refused, TLS errors, or timeouts), this always returns a
/// JSON result to the caller so plugins can report per-request failures
/// (e.g. urlcheck-style widgets checking many URLs in one `refresh()`).
///
/// Input JSON: {"url": "...", "method": "HEAD"|"GET"|..., "headers": {...}}
/// Success:    {"ok": true, "status": 200}
/// Failure:    {"ok": false, "error": "..."}
fn run_safe_http_request(input: &str) -> Result<String, extism::Error> {
    let request: serde_json::Value = serde_json::from_str(input)
        .map_err(|e| extism::Error::msg(format!("Invalid http request JSON: {}", e)))?;

    let url = request["url"].as_str().unwrap_or("");
    if url.is_empty() {
        return Ok(
            serde_json::json!({"ok": false, "error": "safe_http_request: 'url' field is required"})
                .to_string(),
        );
    }

    let method = request["method"].as_str().unwrap_or("GET").to_uppercase();
    let headers: Vec<(String, String)> = request["headers"]
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let result = match run_safe_http_request_sync(url, &method, &headers) {
        Ok(status) => serde_json::json!({"ok": true, "status": status}),
        Err(e) => serde_json::json!({"ok": false, "error": e}),
    };

    Ok(result.to_string())
}

/// Perform the actual HTTP request synchronously, returning the status
/// code on success or a human-readable error string on failure. Split
/// out from `run_safe_http_request` so the network call itself is
/// testable without going through the WASM host function plumbing.
fn run_safe_http_request_sync(
    url: &str,
    method: &str,
    headers: &[(String, String)],
) -> Result<u16, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(8)))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);

    let mut builder = ureq::http::request::Builder::new().method(method).uri(url);
    for (key, value) in headers {
        builder = builder.header(key, value);
    }
    let request = builder.body(()).map_err(|e| e.to_string())?;

    agent
        .run(request)
        .map(|res| res.status().as_u16())
        .map_err(|e| e.to_string())
}

fn make_safe_http_function() -> Function {
    Function::new(
        "safe_http_request",
        [PTR],
        [PTR],
        UserData::new(()),
        |plugin: &mut extism::CurrentPlugin, inputs: &[Val], outputs: &mut [Val], _user_data| {
            let input: String = plugin.memory_get_val(&inputs[0])?;
            let result = run_safe_http_request(&input)?;
            let handle = plugin.memory_new(result)?;
            outputs[0] = plugin.memory_to_val(handle);
            Ok(())
        },
    )
}

/// Handle a `store_get` request against the plugin host state.
/// Input JSON: {"key": "..."}; returns JSON {"found": bool, "value": string|null}
fn run_store_get_request(state: &HostState, input: &str) -> Result<String, extism::Error> {
    let request: serde_json::Value = serde_json::from_str(input)
        .map_err(|e| extism::Error::msg(format!("Invalid store_get request JSON: {}", e)))?;
    let key = request["key"].as_str().unwrap_or("");

    state
        .permissions
        .check_storage()
        .map_err(|e| extism::Error::msg(e.to_string()))?;

    let response = if key.is_empty() {
        serde_json::json!({ "found": false, "value": null })
    } else {
        let value = state
            .store
            .get(key)
            .map(|bytes| String::from_utf8_lossy(bytes).to_string());
        serde_json::json!({ "found": value.is_some(), "value": value })
    };

    Ok(response.to_string())
}

fn make_store_get_function(host_state: UserData<HostState>) -> Function {
    Function::new(
        "store_get",
        [PTR],
        [PTR],
        host_state,
        |plugin: &mut extism::CurrentPlugin, inputs: &[Val], outputs: &mut [Val], user_data| {
            let input: String = plugin.memory_get_val(&inputs[0])?;
            let state = user_data
                .get()
                .map_err(|e| extism::Error::msg(e.to_string()))?;
            let state = state
                .lock()
                .map_err(|_| extism::Error::msg("Failed to lock plugin host state"))?;
            let response = run_store_get_request(&state, &input)?;
            let handle = plugin.memory_new(response)?;
            outputs[0] = plugin.memory_to_val(handle);
            Ok(())
        },
    )
}

/// Handle a `store_set` request against the plugin host state.
/// Input JSON: {"key": "...", "value": "..."}; returns JSON {"ok": true}
fn run_store_set_request(state: &mut HostState, input: &str) -> Result<String, extism::Error> {
    let request: serde_json::Value = serde_json::from_str(input)
        .map_err(|e| extism::Error::msg(format!("Invalid store_set request JSON: {}", e)))?;
    let key = request["key"].as_str().unwrap_or("");
    let value = request["value"].as_str().unwrap_or("");

    state
        .permissions
        .check_storage()
        .map_err(|e| extism::Error::msg(e.to_string()))?;

    if key.is_empty() {
        return Err(extism::Error::msg("store_set: 'key' field is required"));
    }

    state
        .store
        .set(key, value.as_bytes().to_vec())
        .map_err(|e| extism::Error::msg(e.to_string()))?;

    Ok(serde_json::json!({ "ok": true }).to_string())
}

fn make_store_set_function(host_state: UserData<HostState>) -> Function {
    Function::new(
        "store_set",
        [PTR],
        [PTR],
        host_state,
        |plugin: &mut extism::CurrentPlugin, inputs: &[Val], outputs: &mut [Val], user_data| {
            let input: String = plugin.memory_get_val(&inputs[0])?;
            let state = user_data
                .get()
                .map_err(|e| extism::Error::msg(e.to_string()))?;
            let mut state = state
                .lock()
                .map_err(|_| extism::Error::msg("Failed to lock plugin host state"))?;
            let response = run_store_set_request(&mut state, &input)?;
            let handle = plugin.memory_new(response)?;
            outputs[0] = plugin.memory_to_val(handle);
            Ok(())
        },
    )
}

/// Return the plugin's dedicated data directory path.
/// Requires `storage = true` in the plugin's permissions.
/// Returns JSON: {"path": "/absolute/path/to/dir"}
fn run_get_data_dir_request(state: &HostState, plugin_name: &str) -> Result<String, extism::Error> {
    state
        .permissions
        .check_storage()
        .map_err(|e| extism::Error::msg(e.to_string()))?;
    let dir = crate::host_functions::plugin_data_dir(plugin_name)
        .map_err(|e| extism::Error::msg(e.to_string()))?;
    Ok(serde_json::json!({ "path": dir.to_string_lossy() }).to_string())
}

fn make_get_data_dir_function(host_state: UserData<HostState>, plugin_name: String) -> Function {
    Function::new(
        "get_data_dir",
        [PTR],
        [PTR],
        host_state,
        move |plugin, _inputs, outputs, user_data| {
            let state = user_data
                .get()
                .map_err(|e| extism::Error::msg(e.to_string()))?;
            let state = state
                .lock()
                .map_err(|_| extism::Error::msg("Failed to lock plugin host state"))?;
            let response = run_get_data_dir_request(&state, &plugin_name)?;
            let handle = plugin.memory_new(response)?;
            outputs[0] = plugin.memory_to_val(handle);
            Ok(())
        },
    )
}

/// Serialize the plugin's configured settings for the `get_config` host function.
fn run_get_config_request(state: &HostState) -> Result<String, extism::Error> {
    serde_json::to_string(
        &state
            .config
            .as_ref()
            .map(|config| &config.settings)
            .cloned()
            .unwrap_or_default(),
    )
    .map_err(|e| extism::Error::msg(e.to_string()))
}

fn make_get_config_function(host_state: UserData<HostState>) -> Function {
    Function::new(
        "get_config",
        [PTR],
        [PTR],
        host_state,
        |plugin: &mut extism::CurrentPlugin, _inputs: &[Val], outputs: &mut [Val], user_data| {
            let state = user_data
                .get()
                .map_err(|e| extism::Error::msg(e.to_string()))?;
            let state = state
                .lock()
                .map_err(|_| extism::Error::msg("Failed to lock plugin host state"))?;
            let config_json = run_get_config_request(&state)?;
            let handle = plugin.memory_new(config_json)?;
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
            .with_allowed_hosts(permissions.network.clone().into_iter())
            .with_timeout(std::time::Duration::from_secs(10));

        let host_state = UserData::new(HostState {
            config: None,
            permissions: PermissionGuard::new(permissions),
            store: PluginStore::for_plugin(&name)?,
        });
        let host_functions = [
            make_exec_function(host_state.clone()),
            make_safe_http_function(),
            make_store_get_function(host_state.clone()),
            make_store_set_function(host_state.clone()),
            make_get_config_function(host_state.clone()),
            make_get_data_dir_function(host_state.clone(), name.clone()),
        ];
        let mut plugin = Plugin::new(&manifest, host_functions, true)
            .with_context(|| format!("Failed to create WASM plugin: {}", path.display()))?;

        // Try to get metadata from the plugin
        let metadata = match plugin.call::<&str, String>("metadata", "") {
            Ok(json_str) => parse_widget_metadata(&json_str, &name),
            Err(_) => default_metadata(&name),
        };

        Ok(Self {
            metadata,
            host_state,
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
            .with_allowed_hosts(permissions.network.clone().into_iter())
            .with_timeout(std::time::Duration::from_secs(10));
        let host_state = UserData::new(HostState {
            config: None,
            permissions: PermissionGuard::new(permissions),
            store: PluginStore::for_plugin(&metadata.name)?,
        });
        let host_functions = [
            make_exec_function(host_state.clone()),
            make_safe_http_function(),
            make_store_get_function(host_state.clone()),
            make_store_set_function(host_state.clone()),
            make_get_config_function(host_state.clone()),
            make_get_data_dir_function(host_state.clone(), metadata.name.clone()),
        ];
        let plugin = Plugin::new(&manifest, host_functions, true)?;

        Ok(Self {
            metadata,
            host_state,
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

/// Parse a plugin's raw JSON `refresh()` output into a `WidgetContent`.
///
/// This is the canonical "wire format" parser that real WASM/Lua plugins'
/// JSON output is decoded through. It is exposed publicly so callers (e.g.
/// `slate docs`) can render realistic mock snapshots for plugins that would
/// otherwise require live credentials/network access, by parsing a static
/// fixture file through the exact same code path used at runtime.
pub fn parse_widget_content(json_str: &str) -> WidgetContent {
    if let Ok(content) = serde_json::from_str::<WidgetContent>(json_str) {
        return content;
    }

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
                            if let Some(pair) = item.as_array() {
                                let key = pair.first()?.as_str()?;
                                let cell = pair
                                    .get(1)
                                    .cloned()
                                    .and_then(|value| serde_json::from_value(value).ok())
                                    .unwrap_or_else(|| {
                                        slate_plugin_sdk::Cell::plain(
                                            pair.get(1)
                                                .and_then(|value| value.as_str())
                                                .unwrap_or(""),
                                        )
                                    });
                                return Some((key.to_string(), cell));
                            }

                            let key = item["key"].as_str()?;
                            let cell = item
                                .get("value")
                                .cloned()
                                .and_then(|value| serde_json::from_value(value).ok())
                                .unwrap_or_else(|| {
                                    slate_plugin_sdk::Cell::plain(
                                        item["value"].as_str().unwrap_or(""),
                                    )
                                });
                            Some((key.to_string(), cell))
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
                        .map(|item| {
                            let title = item["title"]
                                .as_str()
                                .or_else(|| item["text"].as_str())
                                .or_else(|| item.as_str())
                                .unwrap_or("");
                            slate_plugin_sdk::ListItem {
                                id: item["id"].as_str().unwrap_or("").to_string(),
                                title: title.to_string(),
                                subtitle: item["subtitle"]
                                    .as_str()
                                    .or_else(|| item["secondary"].as_str())
                                    .map(String::from),
                                icon: item["icon"].as_str().map(String::from),
                                style: Default::default(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            WidgetContent::List {
                items,
                selectable: val["selectable"].as_bool().unwrap_or(false),
                actions: parse_list_actions(val.get("actions")),
            }
        }
        "table" => {
            let headers: Vec<String> = val["headers"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|h| h.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let rows: Vec<Vec<slate_plugin_sdk::Cell>> = val["rows"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .map(|row| {
                            row.as_array()
                                .map(|cells| cells.iter().map(parse_cell).collect())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            WidgetContent::Table {
                headers,
                rows,
                selectable: val["selectable"].as_bool().unwrap_or(false),
            }
        }
        _ => WidgetContent::Text {
            content: val["content"].as_str().unwrap_or(json_str).to_string(),
            scrollable: false,
            wrap: true,
        },
    }
}

/// Parse a single table cell, which may be a plain string or an object with
/// `text` and an optional `style` (`fg`/`bg`/`bold`/`italic`).
fn parse_cell(val: &serde_json::Value) -> slate_plugin_sdk::Cell {
    if let Some(text) = val.as_str() {
        return slate_plugin_sdk::Cell::plain(text.to_string());
    }

    let text = val["text"].as_str().unwrap_or("").to_string();
    let style = slate_plugin_sdk::CellStyle {
        fg: val["style"]["fg"].as_str().and_then(parse_color),
        bg: val["style"]["bg"].as_str().and_then(parse_color),
        bold: val["style"]["bold"].as_bool().unwrap_or(false),
        italic: val["style"]["italic"].as_bool().unwrap_or(false),
    };
    slate_plugin_sdk::Cell { text, style }
}

/// Parse a color name into a `Color`. Supports the named palette
/// (`red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `gray`).
fn parse_color(name: &str) -> Option<slate_plugin_sdk::Color> {
    use slate_plugin_sdk::Color;
    match name.to_ascii_lowercase().as_str() {
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        _ => None,
    }
}

fn parse_list_actions(actions: Option<&serde_json::Value>) -> Vec<Action> {
    actions
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| serde_json::from_value::<Action>(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
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

    if val["action"] == "prompt_input" {
        let prompt = val["prompt"].as_str().unwrap_or("Input:").to_string();
        let action_id = val["action_id"].as_str().unwrap_or("").to_string();
        return Some(WidgetAction::PromptInput { prompt, action_id });
    }
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
        if let Ok(state) = self.host_state.get() {
            if let Ok(mut state) = state.lock() {
                state.config = Some(config.clone());
            }
        }
        self.config = Some(config);
    }

    fn refresh(&mut self) -> WidgetContent {
        let settings = build_refresh_settings(self.config.as_ref());
        let input = serde_json::to_string(&settings).unwrap_or_default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.plugin.call::<&str, String>("refresh", &input)
        }));
        match result {
            Ok(call_result) => widget_content_from_refresh_result(call_result, &self.metadata.name),
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
        call_optional_export(&mut self.plugin, "on_key", &input);
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

    fn on_focus(&mut self) {
        call_optional_export(&mut self.plugin, "on_focus", "");
    }

    fn on_blur(&mut self) {
        call_optional_export(&mut self.plugin, "on_blur", "");
    }
}

fn call_optional_export(plugin: &mut Plugin, export_name: &str, input: &str) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        plugin.call::<&str, String>(export_name, input)
    }));
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
    fn parse_color_maps_all_named_colors_case_insensitively() {
        use slate_plugin_sdk::Color;

        assert!(matches!(parse_color("red"), Some(Color::Red)));
        assert!(matches!(parse_color("GREEN"), Some(Color::Green)));
        assert!(matches!(parse_color("Yellow"), Some(Color::Yellow)));
        assert!(matches!(parse_color("blue"), Some(Color::Blue)));
        assert!(matches!(parse_color("magenta"), Some(Color::Magenta)));
        assert!(matches!(parse_color("cyan"), Some(Color::Cyan)));
        assert!(matches!(parse_color("white"), Some(Color::White)));
        assert!(matches!(parse_color("gray"), Some(Color::Gray)));
        assert!(matches!(parse_color("grey"), Some(Color::Gray)));
        assert!(parse_color("not-a-color").is_none());
    }

    #[test]
    fn parse_cell_handles_bg_and_italic_style_and_missing_style() {
        let cell = parse_cell(&serde_json::json!({
            "text": "warn",
            "style": {"bg": "red", "italic": true}
        }));
        assert_eq!(cell.text, "warn");
        assert!(matches!(cell.style.bg, Some(slate_plugin_sdk::Color::Red)));
        assert!(cell.style.italic);
        assert!(!cell.style.bold);
        assert!(cell.style.fg.is_none());

        let plain = parse_cell(&serde_json::json!({"text": "no style"}));
        assert_eq!(plain.text, "no style");
        assert!(plain.style.fg.is_none());
        assert!(plain.style.bg.is_none());
    }

    #[test]
    fn parse_widget_content_handles_table_content() {
        let content = parse_widget_content(
            r#"{"type":"table","headers":["Symbol","Price"],"rows":[[{"text":"AAPL","style":{"bold":true}},{"text":"+1.96%","style":{"fg":"green"}}],["Plain","Row"]],"selectable":true}"#,
        );

        match content {
            WidgetContent::Table {
                headers,
                rows,
                selectable,
            } => {
                assert_eq!(headers, vec!["Symbol", "Price"]);
                assert!(selectable);
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0][0].text, "AAPL");
                assert!(rows[0][0].style.bold);
                assert_eq!(rows[0][1].text, "+1.96%");
                assert!(matches!(
                    rows[0][1].style.fg,
                    Some(slate_plugin_sdk::Color::Green)
                ));
                assert_eq!(rows[1][0].text, "Plain");
                assert_eq!(rows[1][1].text, "Row");
            }
            other => panic!("expected table content, got {other:?}"),
        }
    }

    #[test]
    fn parse_widget_content_handles_array_key_values_and_cells() {
        let content = parse_widget_content(
            r#"{"type":"key_value","pairs":[["CPU",{"text":"12%","style":{"fg":"green"}}],["Memory",42]]}"#,
        );

        match content {
            WidgetContent::KeyValue { pairs } => {
                assert_eq!(pairs.len(), 2);
                assert_eq!(pairs[0].0, "CPU");
                assert_eq!(pairs[0].1.text, "12%");
                assert_eq!(pairs[1].0, "Memory");
                assert_eq!(pairs[1].1.text, "");
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
    fn parse_widget_content_handles_list_actions() {
        let content = parse_widget_content(
            r#"{"type":"list","items":[{"id":"1","title":"Issue 1"}],"actions":[{"id":"open","label":"Open","key":"o","confirm":false},{"id":42}]}"#,
        );

        match content {
            WidgetContent::List { actions, .. } => {
                assert_eq!(actions.len(), 1);
                assert_eq!(actions[0].id, "open");
                assert_eq!(actions[0].label, "Open");
                assert_eq!(actions[0].key.as_deref(), Some("o"));
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
    fn parse_widget_action_handles_prompt_input() {
        assert_eq!(
            parse_widget_action(
                r#"{"action":"prompt_input","prompt":"New todo","action_id":"add"}"#
            ),
            Some(WidgetAction::PromptInput {
                prompt: "New todo".to_string(),
                action_id: "add".to_string(),
            })
        );
        // Missing prompt defaults to "Input:"
        assert_eq!(
            parse_widget_action(r#"{"action":"prompt_input","action_id":"save"}"#),
            Some(WidgetAction::PromptInput {
                prompt: "Input:".to_string(),
                action_id: "save".to_string(),
            })
        );
    }

    #[test]
    fn helper_functions_cover_exec_refresh_and_action_paths() {
        let guard = PermissionGuard::new(Permissions {
            exec: vec![
                "cmd".to_string(),
                "definitely_missing_slate_command".to_string(),
                "echo".to_string(),
            ],
            ..Default::default()
        });

        let missing_cmd = run_exec_request(&guard, r#"{}"#).unwrap();
        assert!(missing_cmd.contains("\"exit_code\":1"));
        assert!(missing_cmd.contains("cmd"));

        let bad_json = run_exec_request(&guard, "not-json");
        assert!(bad_json.is_err());

        #[cfg(windows)]
        let ok = run_exec_request(&guard, r#"{"cmd":"cmd","args":["/c","echo hello"]}"#).unwrap();
        #[cfg(not(windows))]
        let ok = run_exec_request(&guard, r#"{"cmd":"echo","args":["hello"]}"#).unwrap();
        assert!(ok.contains("hello"));

        let missing =
            run_exec_request(&guard, r#"{"cmd":"definitely_missing_slate_command"}"#).unwrap();
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
    fn safe_http_request_never_traps_on_missing_url() {
        let result = run_safe_http_request(r#"{}"#).unwrap();
        assert!(result.contains("\"ok\":false"));
        assert!(result.contains("'url' field is required"));
    }

    #[test]
    fn safe_http_request_returns_ok_on_invalid_json_input() {
        // Even malformed input should not panic; parsing errors surface as a
        // regular `Err` from the JSON parse step, not a WASM trap (the WASM
        // trap only happens with Extism's *built-in* http_request, which
        // this function replaces).
        assert!(run_safe_http_request("not-json").is_err());
    }

    #[test]
    fn safe_http_request_sync_reports_connection_failures_without_panicking() {
        // A reserved, non-routable address should fail fast with a
        // connection error rather than trap or hang indefinitely.
        let result = run_safe_http_request_sync("http://127.0.0.1:1", "HEAD", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn safe_http_request_wraps_connection_failures_as_ok_false() {
        let input = r#"{"url":"http://127.0.0.1:1","method":"HEAD"}"#;
        let result = run_safe_http_request(input).unwrap();
        assert!(result.contains("\"ok\":false"));
        assert!(result.contains("\"error\""));
    }

    #[test]
    fn safe_http_request_sync_reports_status_and_sends_headers_to_local_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let response = "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 2048];
            let read = stream.read(&mut buf).unwrap();
            let request_text = String::from_utf8_lossy(&buf[..read]).to_string();
            stream.write_all(response.as_bytes()).unwrap();
            request_text
        });

        let headers = vec![("x-demo".to_string(), "yes".to_string())];
        let status =
            run_safe_http_request_sync(&format!("http://{addr}/status"), "HEAD", &headers).unwrap();
        assert_eq!(status, 404);

        let request_text = handle.join().unwrap();
        assert!(request_text.contains("HEAD /status HTTP/1.1"));
        assert!(request_text.to_lowercase().contains("x-demo: yes"));
    }

    #[test]
    fn safe_http_request_reports_ok_true_for_a_real_http_status() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let response = "HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 2048];
            let _ = stream.read(&mut buf).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });

        let input =
            format!(r#"{{"url":"http://{addr}/ok","method":"HEAD","headers":{{"x-demo":"yes"}}}}"#);
        let result = run_safe_http_request(&input).unwrap();
        assert!(result.contains("\"ok\":true"));
        assert!(result.contains("\"status\":200"));
    }

    #[test]
    #[ignore] // manual verification only: hits the network and a real built artifact
    fn urlcheck_plugin_reports_per_url_results_without_trapping_on_unreachable_host() {
        let wasm_path =
            Path::new("../../plugins/urlcheck/target/wasm32-wasip1/release/slate_urlcheck.wasm");
        let mut plugin =
            WasmPlugin::from_file(wasm_path, Permissions::default()).expect("load plugin");
        plugin.init(WidgetConfig {
            position: slate_plugin_sdk::Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: std::collections::HashMap::from([(
                "urls".to_string(),
                serde_json::json!([
                    "https://github.com",
                    "https://example.com",
                    "https://httpbin.org/status/500",
                    "not-a-valid-url",
                    "https://this-domain-does-not-exist-12345.example"
                ]),
            )]),
            refresh_interval: None,
        });

        let content = plugin.refresh();
        match content {
            WidgetContent::List { items, .. } => {
                assert_eq!(
                    items.len(),
                    5,
                    "expected all 5 URLs to be reported, got {items:?}"
                );
            }
            other => panic!("expected list content with per-url results, got {other:?}"),
        }
    }

    #[test]
    fn refresh_result_errors_use_friendly_messages() {
        let cases = [
            ("Connection refused by host", "Connection refused"),
            ("request timed out", "Request timed out"),
            ("certificate verify failed", "TLS/SSL error"),
            (
                "wasm backtrace: http::request failed",
                "Network request failed",
            ),
        ];

        for (error, expected) in cases {
            match widget_content_from_refresh_result(Err(extism::Error::msg(error)), "demo") {
                WidgetContent::Text { content, .. } => {
                    assert!(content.contains("[demo]"));
                    assert!(content.contains(expected));
                }
                other => panic!("expected text content, got {other:?}"),
            }
        }
    }

    fn host_state_with_storage(storage: bool) -> HostState {
        HostState {
            config: None,
            permissions: PermissionGuard::new(Permissions {
                storage,
                ..Default::default()
            }),
            store: PluginStore::new(),
        }
    }

    #[test]
    fn run_store_set_and_get_round_trip_value() {
        let mut state = host_state_with_storage(true);

        let set = run_store_set_request(&mut state, r#"{"key":"count","value":"7"}"#).unwrap();
        assert_eq!(set, r#"{"ok":true}"#);

        let got = run_store_get_request(&state, r#"{"key":"count"}"#).unwrap();
        let got: serde_json::Value = serde_json::from_str(&got).unwrap();
        assert_eq!(got["found"], serde_json::json!(true));
        assert_eq!(got["value"], serde_json::json!("7"));
    }

    #[test]
    fn run_store_get_reports_missing_and_empty_keys() {
        let state = host_state_with_storage(true);

        let missing: serde_json::Value =
            serde_json::from_str(&run_store_get_request(&state, r#"{"key":"nope"}"#).unwrap())
                .unwrap();
        assert_eq!(missing["found"], serde_json::json!(false));
        assert_eq!(missing["value"], serde_json::Value::Null);

        let empty: serde_json::Value =
            serde_json::from_str(&run_store_get_request(&state, r#"{"key":""}"#).unwrap()).unwrap();
        assert_eq!(empty["found"], serde_json::json!(false));
    }

    #[test]
    fn run_store_requests_reject_invalid_json_and_missing_key() {
        let mut state = host_state_with_storage(true);

        let err = run_store_get_request(&state, "not json").unwrap_err();
        assert!(err.to_string().contains("Invalid store_get request JSON"));

        let err = run_store_set_request(&mut state, "not json").unwrap_err();
        assert!(err.to_string().contains("Invalid store_set request JSON"));

        let err = run_store_set_request(&mut state, r#"{"value":"x"}"#).unwrap_err();
        assert!(err.to_string().contains("'key' field is required"));
    }

    #[test]
    fn run_store_requests_are_denied_without_storage_permission() {
        let mut state = host_state_with_storage(false);

        assert!(run_store_get_request(&state, r#"{"key":"count"}"#).is_err());
        assert!(run_store_set_request(&mut state, r#"{"key":"count","value":"7"}"#).is_err());
    }

    #[test]
    fn run_get_config_request_returns_settings_or_empty_object() {
        let mut state = host_state_with_storage(true);
        assert_eq!(run_get_config_request(&state).unwrap(), "{}");

        state.config = Some(WidgetConfig {
            position: slate_plugin_sdk::Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: std::collections::HashMap::from([(
                "work_minutes".to_string(),
                serde_json::json!(25),
            )]),
            refresh_interval: None,
        });

        let config: serde_json::Value =
            serde_json::from_str(&run_get_config_request(&state).unwrap()).unwrap();
        assert_eq!(config["work_minutes"], serde_json::json!(25));
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

    #[test]
    fn run_get_data_dir_request_denied_without_storage_permission() {
        let state = host_state_with_storage(false);
        let err = run_get_data_dir_request(&state, "my-plugin").unwrap_err();
        assert!(
            err.to_string().contains("storage"),
            "expected storage error, got: {err}"
        );
    }

    #[test]
    fn run_get_data_dir_request_creates_dir_and_returns_path() {
        let state = host_state_with_storage(true);
        let result = run_get_data_dir_request(&state, "test-todo-plugin").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let path_str = parsed["path"].as_str().expect("path key missing");
        assert!(!path_str.is_empty(), "path should not be empty");
        let path = std::path::Path::new(path_str);
        assert!(
            path.exists(),
            "plugin data directory should have been created: {path_str}"
        );
        assert!(path.is_dir(), "path should be a directory: {path_str}");
    }
}
