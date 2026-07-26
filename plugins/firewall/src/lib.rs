use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
struct RefreshInput {
    #[serde(default)]
    platform: String,
    enabled: Option<bool>,
    #[serde(default)]
    rules: Vec<FirewallRule>,
}

#[derive(Deserialize)]
struct FirewallRule {
    #[serde(default)]
    name: String,
    #[serde(default)]
    direction: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    port: String,
    #[serde(default)]
    protocol: String,
}

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    Ok(json!({
        "name": "Firewall",
        "description": "Firewall status and rules",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    })
    .to_string())
}

#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: RefreshInput = serde_json::from_str(&input).unwrap_or_default();

    if settings.enabled.is_none() && settings.rules.is_empty() {
        return Ok(json!({
            "type": "text",
            "content": "Firewall data unavailable - configure permissions",
            "scrollable": false,
            "wrap": true
        })
        .to_string());
    }

    let enabled = settings.enabled.unwrap_or(false);
    let platform = if settings.platform.trim().is_empty() {
        "unknown".to_string()
    } else {
        settings.platform.trim().to_string()
    };

    let mut items = vec![json!({
        "id": "firewall-status",
        "title": format!("Firewall: {}", if enabled { "Enabled" } else { "Disabled" }),
        "subtitle": format!("Platform: {}", platform)
    })];

    for (index, rule) in settings.rules.iter().enumerate() {
        let action = if rule.action.eq_ignore_ascii_case("block") {
            "BLOCK"
        } else {
            "ALLOW"
        };
        let direction = if rule.direction.eq_ignore_ascii_case("out") {
            "OUT"
        } else {
            "IN"
        };
        let protocol = lower_or_default(&rule.protocol, "any");
        let port = trim_or_default(&rule.port, "any");
        let name = trim_or_default(&rule.name, "Unnamed rule");

        items.push(json!({
            "id": format!("rule-{index}"),
            "title": format!("{action} {direction} {protocol}/{port} - {name}"),
            "subtitle": format!("Platform: {}", platform)
        }));
    }

    Ok(json!({
        "type": "list",
        "items": items,
        "selectable": true
    })
    .to_string())
}

fn trim_or_default(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn lower_or_default(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}
