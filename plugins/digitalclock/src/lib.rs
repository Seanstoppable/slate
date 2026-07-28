#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

use serde_json::json;

// Each digit is 5 lines tall, 5 chars wide
const DIGITS: [&[&str; 5]; 10] = [
    // 0
    &[" ███ ", "█   █", "█   █", "█   █", " ███ "],
    // 1
    &["  █  ", " ██  ", "  █  ", "  █  ", " ███ "],
    // 2
    &[" ███ ", "█   █", "  ██ ", " █   ", "█████"],
    // 3
    &[" ███ ", "█   █", "  ██ ", "█   █", " ███ "],
    // 4
    &["█  █ ", "█  █ ", "█████", "   █ ", "   █ "],
    // 5
    &["█████", "█    ", "████ ", "    █", "████ "],
    // 6
    &[" ███ ", "█    ", "████ ", "█   █", " ███ "],
    // 7
    &["█████", "   █ ", "  █  ", " █   ", " █   "],
    // 8
    &[" ███ ", "█   █", " ███ ", "█   █", " ███ "],
    // 9
    &[" ███ ", "█   █", " ████", "    █", " ███ "],
];

const COLON: [&str; 5] = ["     ", "  █  ", "     ", "  █  ", "     "];

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "Digital Clock",
        "description": "Large ASCII art time display",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let utc_offset = settings["utcOffset"].as_i64().unwrap_or(0);
    let adjusted = (secs as i64 + utc_offset * 3600) as u64;

    let hours = ((adjusted % 86400) / 3600) as u8;
    let minutes = ((adjusted % 3600) / 60) as u8;
    let seconds = (adjusted % 60) as u8;

    let show_seconds = settings["showSeconds"].as_bool().unwrap_or(true);
    let use_24h = settings["use24HourFormat"].as_bool().unwrap_or(true);

    let display_hours = if use_24h {
        hours
    } else {
        let h = hours % 12;
        if h == 0 { 12 } else { h }
    };

    let content = render_time(display_hours, minutes, seconds, show_seconds);

    let mut text = content;
    if !use_24h {
        let period = if hours < 12 { "AM" } else { "PM" };
        text.push_str(&format!("\n{:>width$}", period, width = if show_seconds { 29 } else { 19 }));
    }

    let result = json!({
        "type": "text",
        "content": text,
        "scrollable": false,
        "wrap": false
    });
    Ok(result.to_string())
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

// --- Pure logic (testable on native) ---

/// Render a time as ASCII art.
fn render_time(hours: u8, minutes: u8, seconds: u8, show_seconds: bool) -> String {
    let h1 = (hours / 10) as usize;
    let h2 = (hours % 10) as usize;
    let m1 = (minutes / 10) as usize;
    let m2 = (minutes % 10) as usize;
    let s1 = (seconds / 10) as usize;
    let s2 = (seconds % 10) as usize;

    let mut lines = Vec::with_capacity(5);
    for row in 0..5 {
        let mut line = String::new();
        line.push_str(DIGITS[h1][row]);
        line.push(' ');
        line.push_str(DIGITS[h2][row]);
        line.push(' ');
        line.push_str(COLON[row]);
        line.push(' ');
        line.push_str(DIGITS[m1][row]);
        line.push(' ');
        line.push_str(DIGITS[m2][row]);

        if show_seconds {
            line.push(' ');
            line.push_str(COLON[row]);
            line.push(' ');
            line.push_str(DIGITS[s1][row]);
            line.push(' ');
            line.push_str(DIGITS[s2][row]);
        }
        lines.push(line);
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_time_with_seconds() {
        let output = render_time(12, 34, 56, true);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5);
        // All lines should be the same display width (chars count)
        let width = lines[0].chars().count();
        for line in &lines {
            assert_eq!(line.chars().count(), width);
        }
    }

    #[test]
    fn test_render_time_without_seconds() {
        let output = render_time(9, 5, 0, false);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5);
        // Without seconds, lines should be shorter
        let with_secs = render_time(9, 5, 0, true);
        assert!(output.lines().next().unwrap().chars().count() < with_secs.lines().next().unwrap().chars().count());
    }

    #[test]
    fn test_render_time_midnight() {
        let output = render_time(0, 0, 0, true);
        assert!(output.contains("███")); // Should have digits
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_render_time_max() {
        let output = render_time(23, 59, 59, true);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_digit_art_dimensions() {
        // Each digit pattern should be 5 rows of 5 display chars
        for (i, digit) in DIGITS.iter().enumerate() {
            assert_eq!(digit.len(), 5, "Digit {} should have 5 rows", i);
            for (row, line) in digit.iter().enumerate() {
                assert_eq!(line.chars().count(), 5, "Digit {} row {} should be 5 chars wide", i, row);
            }
        }
    }

    #[test]
    fn test_colon_dimensions() {
        assert_eq!(COLON.len(), 5);
        for (row, line) in COLON.iter().enumerate() {
            assert_eq!(line.chars().count(), 5, "Colon row {} should be 5 chars wide", row);
        }
    }

    #[test]
    fn test_render_time_12h_display() {
        // 12-hour display: hours=12 should work
        let output = render_time(12, 0, 0, false);
        assert!(!output.is_empty());
    }

    #[test]
    fn test_render_time_single_digit_hour() {
        // Single digit like 1 → renders as 01
        let output = render_time(1, 0, 0, false);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5);
    }
}
