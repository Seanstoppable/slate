use extism_pdk::*;
use serde_json::{json, Value};

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "Power",
        "description": "Shows host-provided battery and power information",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: Value = serde_json::from_str(&input).unwrap_or(Value::Null);

    let battery_percent = value_to_string(settings.get("battery_percent"));
    let battery_state = value_to_string(settings.get("battery_state"));
    let time_remaining = value_to_string(settings.get("time_remaining"));

    if battery_percent.is_none() && battery_state.is_none() && time_remaining.is_none() {
        return Ok(json!({
            "type": "text",
            "content": "No battery detected",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    Ok(json!({
        "type": "key_value",
        "pairs": [
            {"key": "Battery %", "value": battery_percent.unwrap_or_else(|| "--".to_string())},
            {"key": "State", "value": battery_state.unwrap_or_else(|| "Unknown".to_string())},
            {"key": "Time Remaining", "value": time_remaining.unwrap_or_else(|| "Unknown".to_string())}
        ]
    })
    .to_string())
}

#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

fn value_to_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::Bool(b)) => Some(b.to_string()),
        _ => None,
    }
}
