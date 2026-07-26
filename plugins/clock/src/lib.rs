use extism_pdk::*;
use serde_json::json;

/// Return plugin metadata.
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "Clock",
        "description": "Displays current time with timezone",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

/// Render current time as text content.
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    // Host passes current_time and timezone in the input JSON
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let time_display = settings["current_time"]
        .as_str()
        .unwrap_or("--:--:--");
    let date_display = settings["current_date"]
        .as_str()
        .unwrap_or("---");
    let tz_display = settings["timezone"]
        .as_str()
        .unwrap_or("UTC");

    let content = json!({
        "type": "text",
        "content": format!("\n  🕐  {}\n\n  {}\n  {}", time_display, date_display, tz_display),
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
