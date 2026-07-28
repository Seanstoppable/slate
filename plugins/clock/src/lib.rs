#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

#[cfg(target_arch = "wasm32")]
use serde_json::json;

#[cfg(target_arch = "wasm32")]
const CLOCK_ICON: &str = "\u{1F550}";

#[cfg(target_arch = "wasm32")]
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

#[cfg(target_arch = "wasm32")]
fn get_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn format_time_and_date_from_unix_secs(secs: u64) -> (String, String) {
    let s = (secs % 60) as u8;
    let m = ((secs / 60) % 60) as u8;
    let h = ((secs / 3600) % 24) as u8;
    let time_str = format!("{:02}:{:02}:{:02}", h, m, s);

    let days = (secs / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let weekday = weekday_from_days(days);
    let date_str = format!("{}, {} {:02}, {}", weekday, month_name(month), day, year);

    (time_str, date_str)
}

/// Apply a UTC offset (in seconds) to a unix timestamp, handling underflow.
fn apply_offset(secs: u64, offset_secs: i64) -> u64 {
    if offset_secs >= 0 {
        secs + offset_secs as u64
    } else {
        secs.saturating_sub((-offset_secs) as u64)
    }
}

/// Parse an offset string like "+5:30", "-8", "5.5" into seconds.
fn parse_offset_to_secs(s: &str) -> i64 {
    let s = s.trim();
    let (sign, rest) = if let Some(stripped) = s.strip_prefix('-') {
        (-1i64, stripped)
    } else if let Some(stripped) = s.strip_prefix('+') {
        (1i64, stripped)
    } else {
        (1i64, s)
    };

    // Try "H:MM" format first
    if let Some((h_str, m_str)) = rest.split_once(':') {
        let hours: i64 = h_str.parse().unwrap_or(0);
        let mins: i64 = m_str.parse().unwrap_or(0);
        return sign * (hours * 3600 + mins * 60);
    }

    // Try decimal hours (e.g., "5.5" = 5h30m)
    if let Ok(hours_f) = rest.parse::<f64>() {
        return sign * (hours_f * 3600.0) as i64;
    }

    0
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
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
    let w = ((days % 7) + 4) % 7;
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

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let utc_secs = get_unix_secs();

    // wtfutil-style: "locations" is an object mapping label -> UTC offset string
    // e.g. { "New York": "-5", "London": "0", "Tokyo": "+9", "Mumbai": "+5:30" }
    // Also supports "clocks" array: [{ "label": "...", "utc_offset": "..." }]
    let content = if let Some(locations) = settings.get("locations").and_then(|v| v.as_object()) {
        let label_width = locations
            .keys()
            .map(|k| k.len())
            .max()
            .unwrap_or(12)
            .max(12);

        let mut lines = Vec::new();
        for (label, offset_val) in locations {
            let offset_secs = if let Some(ss) = offset_val.as_str() {
                parse_offset_to_secs(ss)
            } else if let Some(n) = offset_val.as_i64() {
                n * 3600
            } else if let Some(n) = offset_val.as_f64() {
                (n * 3600.0) as i64
            } else {
                0
            };
            let adjusted = apply_offset(utc_secs, offset_secs);
            let (time, date) = format_time_and_date_from_unix_secs(adjusted);
            lines.push(format!(
                "  {:<width$}  {}  {}",
                label, time, date, width = label_width
            ));
        }

        json!({
            "type": "text",
            "content": lines.join("\n"),
            "scrollable": false,
            "wrap": false
        })
    } else if let Some(clocks) = settings.get("clocks").and_then(|v| v.as_array()) {
        if clocks.is_empty() {
            let (time, date) = format_time_and_date_from_unix_secs(utc_secs);
            json!({
                "type": "text",
                "content": format!("  {}  {}\n  {}\n  UTC", CLOCK_ICON, time, date),
                "scrollable": false,
                "wrap": false
            })
        } else {
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
                let offset_secs = if let Some(s) = clock["utc_offset"].as_str() {
                    parse_offset_to_secs(s)
                } else if let Some(n) = clock["utc_offset"].as_i64() {
                    n * 3600
                } else if let Some(n) = clock["utc_offset"].as_f64() {
                    (n * 3600.0) as i64
                } else {
                    0
                };
                let adjusted = apply_offset(utc_secs, offset_secs);
                let (time, date) = format_time_and_date_from_unix_secs(adjusted);
                lines.push(format!(
                    "  {:<width$}  {}  {}",
                    label, time, date, width = label_width
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
        // Single clock, default UTC
        let tz_display = settings["timezone"].as_str().unwrap_or("UTC");
        let offset_secs = if let Some(s) = settings.get("utc_offset") {
            if let Some(ss) = s.as_str() {
                parse_offset_to_secs(ss)
            } else if let Some(n) = s.as_i64() {
                n * 3600
            } else {
                0
            }
        } else {
            0
        };
        let adjusted = apply_offset(utc_secs, offset_secs);
        let (time, date) = format_time_and_date_from_unix_secs(adjusted);

        json!({
            "type": "text",
            "content": format!("  {}  {}\n  {}\n  {}", CLOCK_ICON, time, date, tz_display),
            "scrollable": false,
            "wrap": false
        })
    };

    Ok(content.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(18_628), (2021, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(-365), (1969, 1, 1));
    }

    #[test]
    fn test_weekday_from_days_known_values() {
        assert_eq!(weekday_from_days(0), "Thu");
        assert_eq!(weekday_from_days(1), "Fri");
        assert_eq!(weekday_from_days(2), "Sat");
        assert_eq!(weekday_from_days(3), "Sun");
        assert_eq!(weekday_from_days(-1), "Wed");
    }

    #[test]
    fn test_month_name_all_values() {
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        for (index, expected) in months.iter().enumerate() {
            assert_eq!(month_name(index as u32 + 1), *expected);
        }
        assert_eq!(month_name(0), "???");
        assert_eq!(month_name(13), "???");
    }

    #[test]
    fn test_format_time_and_date_from_unix_secs_epoch() {
        let (time, date) = format_time_and_date_from_unix_secs(0);
        assert_eq!(time, "00:00:00");
        assert_eq!(date, "Thu, Jan 01, 1970");
    }

    #[test]
    fn test_format_time_and_date_from_unix_secs_rollover() {
        let (time, date) = format_time_and_date_from_unix_secs(86_399);
        assert_eq!(time, "23:59:59");
        assert_eq!(date, "Thu, Jan 01, 1970");

        let (time, date) = format_time_and_date_from_unix_secs(86_400 + 3_661);
        assert_eq!(time, "01:01:01");
        assert_eq!(date, "Fri, Jan 02, 1970");
    }

    #[test]
    fn test_parse_offset_to_secs() {
        assert_eq!(parse_offset_to_secs("0"), 0);
        assert_eq!(parse_offset_to_secs("+5"), 18000);
        assert_eq!(parse_offset_to_secs("-5"), -18000);
        assert_eq!(parse_offset_to_secs("+5:30"), 19800);
        assert_eq!(parse_offset_to_secs("-5:30"), -19800);
        assert_eq!(parse_offset_to_secs("5.5"), 19800);
        assert_eq!(parse_offset_to_secs("-9:30"), -34200);
    }

    #[test]
    fn test_apply_offset() {
        assert_eq!(apply_offset(1000, 3600), 4600);
        assert_eq!(apply_offset(1000, -500), 500);
        assert_eq!(apply_offset(100, -200), 0); // saturating
    }

    #[test]
    fn test_multi_timezone_display() {
        // At epoch + 12 hours UTC, New York (-5) should be 07:00
        let utc_secs = 43200; // 12:00:00 UTC
        let ny_secs = apply_offset(utc_secs, -5 * 3600);
        let (time, _) = format_time_and_date_from_unix_secs(ny_secs);
        assert_eq!(time, "07:00:00");

        // Tokyo (+9) should be 21:00
        let tokyo_secs = apply_offset(utc_secs, 9 * 3600);
        let (time, _) = format_time_and_date_from_unix_secs(tokyo_secs);
        assert_eq!(time, "21:00:00");

        // India (+5:30) should be 17:30
        let india_secs = apply_offset(utc_secs, parse_offset_to_secs("+5:30"));
        let (time, _) = format_time_and_date_from_unix_secs(india_secs);
        assert_eq!(time, "17:30:00");
    }
}