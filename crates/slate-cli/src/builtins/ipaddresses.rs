use std::process::Command;

use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};

pub(crate) struct IpAddressesWidget;

impl IpAddressesWidget {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl slate_plugin_sdk::Widget for IpAddressesWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "IP Addresses".to_string(),
            description: "Network interface addresses".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        let interfaces = get_network_interfaces();

        if interfaces.is_empty() {
            return WidgetContent::Text {
                content: "No network interfaces found".to_string(),
                scrollable: false,
                wrap: true,
            };
        }

        let pairs: Vec<(String, slate_plugin_sdk::Cell)> = interfaces
            .into_iter()
            .map(|(name, ip)| {
                let display = if ip.is_empty() { "—".to_string() } else { ip };
                (name, slate_plugin_sdk::Cell::plain(display))
            })
            .collect();

        WidgetContent::KeyValue { pairs }
    }
}

fn get_network_interfaces() -> Vec<(String, String)> {
    let networks = sysinfo::Networks::new_with_refreshed_list();
    let mut results: Vec<(String, String)> = Vec::new();

    for (name, _data) in networks.iter() {
        let ip = get_interface_ip(name);
        results.push((name.clone(), ip));
    }

    results
}

fn get_interface_ip(interface_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("netsh")
            .args(["interface", "ip", "show", "addresses", interface_name])
            .output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("IP Address:") || trimmed.starts_with("IP") {
                    if let Some(ip) = trimmed.split_whitespace().last() {
                        if ip.contains('.') {
                            return ip.to_string();
                        }
                    }
                }
            }
        }
        String::new()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("ip")
            .args(["-4", "addr", "show", interface_name])
            .output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("inet ") {
                    if let Some(addr) = trimmed.split_whitespace().nth(1) {
                        return addr.split('/').next().unwrap_or("").to_string();
                    }
                }
            }
        }
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::IpAddressesWidget;
    use slate_plugin_sdk::{Widget, WidgetContent};

    #[test]
    fn ip_addresses_widget_returns_text_or_key_value_content() {
        let mut widget = IpAddressesWidget::new();
        let metadata = widget.metadata();
        assert_eq!(metadata.name, "IP Addresses");
        assert_eq!(metadata.description, "Network interface addresses");

        match widget.refresh() {
            WidgetContent::KeyValue { pairs } => {
                assert!(pairs.iter().all(|(key, _)| !key.is_empty()));
                assert!(pairs.iter().all(|(_, cell)| !cell.text.is_empty()));
            }
            WidgetContent::Text { content, .. } => {
                assert!(content.contains("No network interfaces found"));
            }
            other => panic!("expected text or key-value content, got {other:?}"),
        }
    }
}
