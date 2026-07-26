use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
struct Settings {
    #[serde(default)]
    interfaces: Vec<NetworkInterface>,
}

#[derive(Deserialize, Default)]
struct NetworkInterface {
    #[serde(default)]
    name: String,
    #[serde(default)]
    ipv4: String,
    #[serde(default)]
    ipv6: String,
    #[serde(default)]
    mac: String,
}

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "IP Addresses",
        "description": "Displays host-provided network interface addresses",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: Settings = serde_json::from_str(&input).unwrap_or_default();

    if settings.interfaces.is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "No network interfaces provided by host",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    let pairs: Vec<_> = settings
        .interfaces
        .into_iter()
        .map(|interface| {
            let mut parts = Vec::new();
            if !interface.ipv4.trim().is_empty() {
                parts.push(format!("IPv4: {}", interface.ipv4.trim()));
            }
            if !interface.ipv6.trim().is_empty() {
                parts.push(format!("IPv6: {}", interface.ipv6.trim()));
            }
            if !interface.mac.trim().is_empty() {
                parts.push(format!("MAC: {}", interface.mac.trim()));
            }
            let key = if interface.name.trim().is_empty() {
                "interface".to_string()
            } else {
                interface.name.trim().to_string()
            };
            let value = if parts.is_empty() {
                "No addresses".to_string()
            } else {
                parts.join(" | ")
            };

            json!({
                "key": key,
                "value": value
            })
        })
        .collect();

    Ok(json!({
        "type": "key_value",
        "pairs": pairs
    })
    .to_string())
}

#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}
