use extism_pdk::*;
use serde_json::json;

/// Return plugin metadata.
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "Clock",
        "description": "Displays the current UTC time",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

/// Render current time as text content.
#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String> {
    // Get current time from host via config (plugins are sandboxed)
    // For now, use a simple epoch-based approach
    let config_time = config::get("current_time").ok().flatten();

    let display = match config_time {
        Some(t) => t,
        None => {
            // Fallback: show a static message prompting host integration
            "Clock plugin loaded - awaiting host time".to_string()
        }
    };

    let content = json!({
        "type": "text",
        "content": format!("🕐 {}", display),
        "scrollable": false,
        "wrap": false
    });
    Ok(content.to_string())
}

/// Handle key events (no-op for clock).
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}
