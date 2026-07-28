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
        build_firewall_content(platform, enabled, rules)
    }
}

fn build_firewall_content(platform: String, enabled: bool, rules: Vec<String>) -> WidgetContent {
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

#[cfg(target_os = "windows")]
fn parse_windows_firewall_enabled(output: std::io::Result<std::process::Output>) -> bool {
    if let Ok(out) = &output {
        String::from_utf8_lossy(&out.stdout).contains("ON")
    } else {
        false
    }
}

#[cfg(target_os = "windows")]
fn parse_windows_firewall_rules(text: &str) -> Vec<String> {
    let mut rules = Vec::new();
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

    rules
}

#[cfg(target_os = "windows")]
fn parse_windows_firewall_rules_output(
    rules_output: std::io::Result<std::process::Output>,
) -> Vec<String> {
    if let Ok(out) = rules_output {
        let text = String::from_utf8_lossy(&out.stdout);
        parse_windows_firewall_rules(&text)
    } else {
        Vec::new()
    }
}

fn get_firewall_info() -> (String, bool, Vec<String>) {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("netsh")
            .args(["advfirewall", "show", "allprofiles", "state"])
            .output();
        let enabled = parse_windows_firewall_enabled(output);

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
        (
            "Windows".to_string(),
            enabled,
            parse_windows_firewall_rules_output(rules_output),
        )
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
    use slate_plugin_sdk::{Position, Widget, WidgetConfig, WidgetContent};

    #[test]
    fn firewall_widget_returns_status_list() {
        let mut widget = FirewallWidget::new();
        widget.init(WidgetConfig {
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            refresh_interval: None,
        });
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

    #[test]
    fn build_firewall_content_shows_disabled_status_and_rules() {
        match super::build_firewall_content(
            "Windows".to_string(),
            false,
            vec!["ALLOW SSH IN/22".to_string()],
        ) {
            WidgetContent::List { items, .. } => {
                assert_eq!(items[0].title, "Firewall: Disabled");
                assert_eq!(items[0].subtitle.as_deref(), Some("Platform: Windows"));
                assert_eq!(items[1].title, "ALLOW SSH IN/22");
            }
            other => panic!("expected list content, got {other:?}"),
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_windows_helpers_cover_disabled_and_final_rule_paths() {
        use std::os::windows::process::ExitStatusExt;
        use std::process::Output;

        assert!(!super::parse_windows_firewall_enabled(Err(
            std::io::Error::new(std::io::ErrorKind::NotFound, "missing netsh",)
        )));

        let rules = super::parse_windows_firewall_rules(
            r#"
Rule Name: Example Rule
Action: Block
LocalPort: 443
"#,
        );

        assert_eq!(rules, vec!["BLOCK Example Rule IN/443"]);
        assert!(
            super::parse_windows_firewall_rules_output(Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing rules",
            )))
            .is_empty()
        );

        let enabled = super::parse_windows_firewall_enabled(Ok(Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: b"State ON".to_vec(),
            stderr: Vec::new(),
        }));
        assert!(enabled);
    }
}
