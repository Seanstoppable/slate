use extism_pdk::*;
use serde_json::json;

/// Return plugin metadata.
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "Clocks",
        "description": "Displays multiple world clocks as a location list (wtfutil-style)",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

/// Render clocks as a list of locations with times.
///
/// The host injects a "clocks" array into the settings, each entry containing:
///   { "label": "New York", "time": "15:04:05", "date": "Mon, Jul 26", "zone": "EDT" }
///
/// If no locations are configured, falls back to a single local time display.
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let clocks = settings["clocks"].as_array();

    let content = if let Some(clocks) = clocks {
        if clocks.is_empty() {
            json!({
                "type": "text",
                "content": " no timezone data available",
                "scrollable": false,
                "wrap": false
            })
        } else {
            // Compute column width for alignment (like wtfutil)
            let label_width = clocks.iter()
                .filter_map(|c| c["label"].as_str())
                .map(|l| l.len())
                .max()
                .unwrap_or(12)
                .max(12);

            let mut lines = Vec::new();
            for clock in clocks {
                let label = clock["label"].as_str().unwrap_or("???");
                let time = clock["time"].as_str().unwrap_or("--:--");
                let date = clock["date"].as_str().unwrap_or("---");
                lines.push(format!(" {:<width$}  {}  {}", label, time, date, width = label_width));
            }

            json!({
                "type": "text",
                "content": lines.join("\n"),
                "scrollable": false,
                "wrap": false
            })
        }
    } else {
        // Legacy fallback: single clock from host-injected fields
        let time_display = settings["current_time"].as_str().unwrap_or("--:--:--");
        let date_display = settings["current_date"].as_str().unwrap_or("---");
        let tz_display = settings["timezone"].as_str().unwrap_or("UTC");

        json!({
            "type": "text",
            "content": format!(" 🕐  {}\n {}\n {}", time_display, date_display, tz_display),
            "scrollable": false,
            "wrap": false
        })
    };

    Ok(content.to_string())
}

/// Handle key events (no-op for clock).
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}
