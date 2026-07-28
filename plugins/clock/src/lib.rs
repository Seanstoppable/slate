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

/// Get the current local time using WASI clock access.
/// Returns (time_str, date_str) formatted for display.
fn get_local_time() -> (String, String) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Format as HH:MM:SS (UTC since WASI only provides UTC clock)
    let s = (secs % 60) as u8;
    let m = ((secs / 60) % 60) as u8;
    let h = ((secs / 3600) % 24) as u8;

    let time_str = format!("{:02}:{:02}:{:02}", h, m, s);

    // Compute date from epoch days
    let days = (secs / 86400) as i64;
    let (year, month, day) = civil_from_days(days);
    let weekday = weekday_from_days(days);
    let date_str = format!("{}, {} {:02}, {}", weekday, month_name(month), day, year);

    (time_str, date_str)
}

/// Convert days since Unix epoch to (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn weekday_from_days(days: i64) -> &'static str {
    let w = ((days % 7) + 4) % 7; // 0=Sun
    match w {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "???",
    }
}

fn month_name(m: u32) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

/// Render clocks as a list of locations with times.
///
/// The host injects a "clocks" array for multi-timezone support, each entry containing:
///     { "label": "New York", "time": "15:04:05", "date": "Mon, Jul 26", "zone": "EDT" }
///
/// If no locations are configured, uses WASI clock to show local (UTC) time directly.
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let clocks = settings["clocks"].as_array();

    let content = if let Some(clocks) = clocks {
        if clocks.is_empty() {
            // No locations configured — use native WASI time
            let (time, date) = get_local_time();
            json!({
                "type": "text",
                "content": format!("  🕐  {}\n  {}\n  UTC", time, date),
                "scrollable": false,
                "wrap": false
            })
        } else {
            // Compute column width for alignment (like wtfutil)
            let label_width = clocks
                .iter()
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
                lines.push(format!(
                    "  {:<width$}  {}  {}",
                    label,
                    time,
                    date,
                    width = label_width
                ));
            }

            json!({
                "type": "text",
                "content": lines.join("\n"),
                "scrollable": false,
                "wrap": false
            })
        }
    } else {
        // No clocks array at all — use WASI native time
        let (time, date) = get_local_time();
        let tz_display = settings["timezone"].as_str().unwrap_or("UTC");

        json!({
            "type": "text",
            "content": format!("  🕐  {}\n  {}\n  {}", time, date, tz_display),
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

/// Handle actions (no-op for clock).
#[plugin_fn]
pub fn on_action(_input: String) -> FnResult<String> {
    Ok(String::new())
}
