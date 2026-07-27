mod firewall;
mod ipaddresses;
mod power;
mod resource_usage;
mod vcs;
mod welcome;

use anyhow::Result;
use slate_plugin_sdk::WidgetConfig;

pub(crate) use welcome::WelcomeWidget;

pub fn create_builtin(
    name: &str,
    config: WidgetConfig,
) -> Result<Box<dyn slate_plugin_sdk::Widget>> {
    match name {
        "resource_usage" => Ok(Box::new(resource_usage::ResourceUsageWidget::new(config))),
        "power" => Ok(Box::new(power::PowerWidget::new())),
        "firewall" => Ok(Box::new(firewall::FirewallWidget::new())),
        "ipaddresses" => Ok(Box::new(ipaddresses::IpAddressesWidget::new())),
        "vcs" => Ok(Box::new(vcs::VcsWidget::new(config))),
        _ => anyhow::bail!("Unknown builtin widget: {}", name),
    }
}
