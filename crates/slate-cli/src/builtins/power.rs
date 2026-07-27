use std::process::Command;

use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};

pub(crate) struct PowerWidget;

impl PowerWidget {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl slate_plugin_sdk::Widget for PowerWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Power".to_string(),
            description: "Battery and power status".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        let (has_battery, state, percent) = get_power_info();
        build_power_content(has_battery, &state, percent)
    }
}

fn build_power_content(has_battery: bool, state: &str, percent: u64) -> WidgetContent {
    let state_color = match state {
        "Charging" => slate_plugin_sdk::Color::Green,
        "Discharging" => {
            if percent < 20 {
                slate_plugin_sdk::Color::Red
            } else if percent < 50 {
                slate_plugin_sdk::Color::Yellow
            } else {
                slate_plugin_sdk::Color::Green
            }
        }
        "Critical" | "Low" => slate_plugin_sdk::Color::Red,
        _ => slate_plugin_sdk::Color::White,
    };

    let mut pairs = vec![(
        "Status".to_string(),
        slate_plugin_sdk::Cell::colored(state.to_string(), state_color),
    )];

    if has_battery {
        let pct_color = if percent < 20 {
            slate_plugin_sdk::Color::Red
        } else if percent < 50 {
            slate_plugin_sdk::Color::Yellow
        } else {
            slate_plugin_sdk::Color::Green
        };
        pairs.push((
            "Battery".to_string(),
            slate_plugin_sdk::Cell::colored(format!("{percent}%"), pct_color),
        ));
    }

    pairs.push((
        "Source".to_string(),
        slate_plugin_sdk::Cell::plain(if has_battery && state == "Discharging" {
            "Battery".to_string()
        } else {
            "AC Power".to_string()
        }),
    ));

    WidgetContent::KeyValue { pairs }
}

fn get_power_info() -> (bool, String, u64) {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Battery | Select-Object EstimatedChargeRemaining, BatteryStatus | ConvertTo-Json) 2>$null; if (-not $?) { Write-Output '{\"ac_power\": true}' }",
            ])
            .output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                if val.get("ac_power").is_some() {
                    return (false, "AC Power".to_string(), 100);
                }
                let percent = val["EstimatedChargeRemaining"].as_u64().unwrap_or(0);
                let status = match val["BatteryStatus"].as_u64().unwrap_or(0) {
                    1 => "Discharging",
                    2 => "AC Power",
                    3 => "Fully Charged",
                    4 => "Low",
                    5 => "Critical",
                    6..=8 => "Charging",
                    _ => "Unknown",
                };
                return (true, status.to_string(), percent);
            }
        }
        (false, "AC Power".to_string(), 100)
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("pmset").args(["-g", "batt"]).output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.contains("InternalBattery") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    let percent = parts
                        .iter()
                        .find(|p| p.ends_with("%;"))
                        .map(|p| p.trim_end_matches("%;").parse::<u64>().unwrap_or(0))
                        .unwrap_or(0);
                    let state = if line.contains("charging") {
                        "Charging"
                    } else if line.contains("discharging") {
                        "Discharging"
                    } else {
                        "Fully Charged"
                    };
                    return (true, state.to_string(), percent);
                }
            }
        }
        (false, "AC Power".to_string(), 100)
    }
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("cat")
            .arg("/sys/class/power_supply/BAT0/capacity")
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                let percent: u64 = String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .parse()
                    .unwrap_or(0);
                let status_out = Command::new("cat")
                    .arg("/sys/class/power_supply/BAT0/status")
                    .output();
                let state = status_out
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|_| "Unknown".to_string());
                return (true, state, percent);
            }
        }
        (false, "AC Power".to_string(), 100)
    }
}

#[cfg(test)]
mod tests {
    use super::{build_power_content, PowerWidget};
    use slate_plugin_sdk::{Widget, WidgetContent};
    use std::collections::HashMap;

    #[test]
    fn power_widget_returns_status_pairs() {
        let mut widget = PowerWidget::new();
        let metadata = widget.metadata();
        assert_eq!(metadata.name, "Power");
        assert_eq!(metadata.description, "Battery and power status");

        match widget.refresh() {
            WidgetContent::KeyValue { pairs } => {
                let keys: Vec<&str> = pairs.iter().map(|(key, _)| key.as_str()).collect();
                assert!(keys.contains(&"Status"));
                assert!(keys.contains(&"Source"));
                assert!(pairs.iter().all(|(_, cell)| !cell.text.is_empty()));
            }
            other => panic!("expected key-value content, got {other:?}"),
        }
    }

    #[test]
    fn power_widget_metadata_matches_expected_values() {
        let widget = PowerWidget::new();
        let metadata = widget.metadata();
        assert_eq!(metadata.name, "Power");
        assert_eq!(metadata.description, "Battery and power status");
        assert_eq!(metadata.version, "0.1.0");
    }

    #[test]
    fn build_power_content_reports_battery_source_when_discharging() {
        let content = build_power_content(true, "Discharging", 15);

        match content {
            WidgetContent::KeyValue { pairs } => {
                let map: HashMap<_, _> =
                    pairs.into_iter().map(|(key, value)| (key, value.text)).collect();
                assert_eq!(map.get("Status").map(String::as_str), Some("Discharging"));
                assert_eq!(map.get("Battery").map(String::as_str), Some("15%"));
                assert_eq!(map.get("Source").map(String::as_str), Some("Battery"));
            }
            other => panic!("expected key-value content, got {other:?}"),
        }
    }

    #[test]
    fn build_power_content_reports_ac_power_without_battery() {
        let content = build_power_content(false, "AC Power", 100);

        match content {
            WidgetContent::KeyValue { pairs } => {
                let map: HashMap<_, _> =
                    pairs.into_iter().map(|(key, value)| (key, value.text)).collect();
                assert_eq!(map.get("Status").map(String::as_str), Some("AC Power"));
                assert!(!map.contains_key("Battery"));
                assert_eq!(map.get("Source").map(String::as_str), Some("AC Power"));
            }
            other => panic!("expected key-value content, got {other:?}"),
        }
    }
}
