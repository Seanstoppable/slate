mod firewall;
mod ipaddresses;
mod logfile;
mod power;
mod resource_usage;
mod welcome;

use anyhow::Result;
use slate_plugin_sdk::WidgetConfig;

pub(crate) use welcome::WelcomeWidget;

pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "firewall" | "ipaddresses" | "logfile" | "power" | "resource_usage"
    )
}

pub fn create_builtin(
    name: &str,
    config: WidgetConfig,
) -> Result<Box<dyn slate_plugin_sdk::Widget>> {
    match name {
        "resource_usage" => Ok(Box::new(resource_usage::ResourceUsageWidget::new(config))),
        "power" => Ok(Box::new(power::PowerWidget::new())),
        "firewall" => Ok(Box::new(firewall::FirewallWidget::new())),
        "ipaddresses" => Ok(Box::new(ipaddresses::IpAddressesWidget::new())),
        "logfile" => Ok(Box::new(logfile::LogfileWidget::new(config))),
        _ => anyhow::bail!("Unknown builtin widget: {}", name),
    }
}

#[cfg(test)]
mod tests {
    use super::{create_builtin, is_builtin};
    use slate_plugin_sdk::{Position, WidgetConfig};

    fn test_widget_config() -> WidgetConfig {
        WidgetConfig {
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            refresh_interval: None,
        }
    }

    #[test]
    fn create_builtin_returns_widgets_for_known_names() {
        let cases = [
            ("resource_usage", "Resources"),
            ("power", "Power"),
            ("firewall", "Firewall"),
            ("ipaddresses", "IP Addresses"),
            ("logfile", "Log File"),
        ];

        for (name, expected_metadata) in cases {
            let widget = create_builtin(name, test_widget_config()).unwrap();
            assert_eq!(widget.metadata().name, expected_metadata);
        }
    }

    #[test]
    fn create_builtin_returns_error_for_unknown_name() {
        let err = match create_builtin("unknown-widget", test_widget_config()) {
            Ok(_) => panic!("expected unknown builtin to fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("Unknown builtin widget"));
    }

    #[test]
    fn is_builtin_matches_the_widget_registry() {
        for name in [
            "firewall",
            "ipaddresses",
            "logfile",
            "power",
            "resource_usage",
        ] {
            assert!(is_builtin(name));
            assert!(create_builtin(name, test_widget_config()).is_ok());
        }
        assert!(!is_builtin("unknown-widget"));
    }
}
