use std::process::Command;

use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};

pub(crate) struct FirewallWidget;

impl FirewallWidget {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl slate_plugin_sdk::Widget for FirewallWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Firewall".to_string(),
            description: "Firewall status and rules".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        let (platform, enabled, rules) = get_firewall_info();

        let status_color = if enabled {
            slate_plugin_sdk::Color::Green
        } else {
            slate_plugin_sdk::Color::Red
        };
        let mut items = vec![slate_plugin_sdk::ListItem {
            id: "status".to_string(),
            title: format!("Firewall: {}", if enabled { "Enabled" } else { "Disabled" }),
            subtitle: Some(format!("Platform: {}", platform)),
            icon: None,
            style: slate_plugin_sdk::CellStyle {
                fg: Some(status_color),
                ..Default::default()
            },
        }];

        for (i, rule) in rules.iter().enumerate() {
            items.push(slate_plugin_sdk::ListItem {
                id: format!("rule-{}", i),
                title: rule.clone(),
                subtitle: None,
                icon: None,
                style: Default::default(),
            });
        }

        WidgetContent::List {
            items,
            selectable: true,
            actions: vec![],
        }
    }
}

fn get_firewall_info() -> (String, bool, Vec<String>) {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("netsh")
            .args(["advfirewall", "show", "allprofiles", "state"])
            .output();
        let enabled = if let Ok(out) = &output {
            String::from_utf8_lossy(&out.stdout).contains("ON")
        } else {
            false
        };

        let rules_output = Command::new("netsh")
            .args([
                "advfirewall",
                "firewall",
                "show",
                "rule",
                "name=all",
                "dir=in",
            ])
            .output();
        let mut rules = Vec::new();
        if let Ok(out) = rules_output {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut name = String::new();
            let mut action = String::new();
            let mut port = String::new();
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("Rule Name:") {
                    if !name.is_empty() {
                        rules.push(format!("{} {} IN/{}", action.to_uppercase(), name, port));
                    }
                    name = trimmed.trim_start_matches("Rule Name:").trim().to_string();
                    action = "Allow".to_string();
                    port = "Any".to_string();
                } else if trimmed.starts_with("Action:") {
                    action = trimmed.trim_start_matches("Action:").trim().to_string();
                } else if trimmed.starts_with("LocalPort:") {
                    port = trimmed.trim_start_matches("LocalPort:").trim().to_string();
                }
                if rules.len() >= 15 {
                    break;
                }
            }
            if !name.is_empty() && rules.len() < 15 {
                rules.push(format!("{} {} IN/{}", action.to_uppercase(), name, port));
            }
        }

        ("Windows".to_string(), enabled, rules)
    }
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("ufw").args(["status"]).output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            let enabled = text.contains("Status: active");
            let rules: Vec<String> = text
                .lines()
                .skip(4)
                .take(15)
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            return ("Linux (ufw)".to_string(), enabled, rules);
        }
        let ipt = Command::new("iptables")
            .args(["-L", "-n", "--line-numbers"])
            .output();
        if let Ok(out) = ipt {
            let text = String::from_utf8_lossy(&out.stdout);
            let rules: Vec<String> = text
                .lines()
                .skip(2)
                .take(15)
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            return ("Linux (iptables)".to_string(), true, rules);
        }
        ("Linux".to_string(), false, Vec::new())
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("pfctl").args(["-sr"]).output();
        let rules: Vec<String> = if let Ok(out) = output {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .take(15)
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        } else {
            Vec::new()
        };
        ("macOS (pf)".to_string(), !rules.is_empty(), rules)
    }
}

#[cfg(test)]
mod tests {
    use super::FirewallWidget;
    use slate_plugin_sdk::{Widget, WidgetContent};

    #[test]
    fn firewall_widget_returns_status_list() {
        let mut widget = FirewallWidget::new();
        let metadata = widget.metadata();
        assert_eq!(metadata.name, "Firewall");
        assert_eq!(metadata.description, "Firewall status and rules");

        match widget.refresh() {
            WidgetContent::List {
                items,
                selectable,
                actions,
            } => {
                assert!(selectable);
                assert!(actions.is_empty());
                assert!(!items.is_empty());
                assert_eq!(items[0].id, "status");
                assert!(items[0].title.contains("Firewall:"));
            }
            other => panic!("expected list content, got {other:?}"),
        }
    }
}
