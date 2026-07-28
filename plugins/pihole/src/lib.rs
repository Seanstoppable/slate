#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

use serde::Deserialize;
use serde_json::json;

/// Pi-hole summaryRaw API response.
#[derive(Deserialize, Debug, Clone, Default)]
struct PiholeSummary {
    #[serde(default)]
    domains_being_blocked: u64,
    #[serde(default)]
    dns_queries_today: u64,
    #[serde(default)]
    ads_blocked_today: u64,
    #[serde(default)]
    ads_percentage_today: f64,
    #[serde(default)]
    unique_domains: u64,
    #[serde(default)]
    queries_forwarded: u64,
    #[serde(default)]
    queries_cached: u64,
    #[serde(default)]
    clients_ever_seen: u64,
    #[serde(default)]
    unique_clients: u64,
    #[serde(default)]
    dns_queries_all_types: u64,
    #[serde(default)]
    reply_NODATA: u64,
    #[serde(default)]
    reply_NXDOMAIN: u64,
    #[serde(default)]
    reply_CNAME: u64,
    #[serde(default)]
    reply_IP: u64,
    #[serde(default)]
    status: String,
    #[serde(default)]
    gravity_last_updated: Option<GravityUpdate>,
}

#[derive(Deserialize, Debug, Clone, Default)]
struct GravityUpdate {
    #[serde(default)]
    relative: Option<GravityRelative>,
}

#[derive(Deserialize, Debug, Clone, Default)]
struct GravityRelative {
    #[serde(default)]
    days: u64,
    #[serde(default)]
    hours: u64,
    #[serde(default)]
    minutes: u64,
}

/// Top items API response (requires auth token).
#[derive(Deserialize, Debug, Clone, Default)]
struct TopItems {
    #[serde(default)]
    top_queries: std::collections::HashMap<String, u64>,
    #[serde(default)]
    top_ads: std::collections::HashMap<String, u64>,
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "Pi-hole",
        "description": "Pi-hole DNS filtering statistics",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let base_url = settings["apiUrl"]
        .as_str()
        .unwrap_or("http://pi.hole/admin/api.php");

    let auth_token = settings["authToken"].as_str().unwrap_or("");

    let url = if auth_token.is_empty() {
        format!("{}?summaryRaw", base_url)
    } else {
        format!("{}?summaryRaw&auth={}", base_url, auth_token)
    };

    let req = HttpRequest::new(&url)
        .with_header("Accept", "application/json");

    // Note: if the host is unreachable, extism HTTP will trap (WASM abort).
    // The host catches this via catch_unwind and shows an error in the widget.
    let response = http::request::<Vec<u8>>(&req, None)?;

    let body_bytes = response.body();
    let body_str = std::str::from_utf8(&body_bytes).unwrap_or("{}");
    let summary: PiholeSummary = serde_json::from_str(body_str).unwrap_or_default();
    let content = build_summary_content(&summary);
    Ok(content.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(input: String) -> FnResult<String> {
    #[derive(Deserialize)]
    struct KeyInput {
        #[serde(default)]
        action: String,
    }
    if let Ok(key_input) = serde_json::from_str::<KeyInput>(&input) {
        if key_input.action == "open" {
            let result = json!({"open_url": "http://pi.hole/admin"});
            return Ok(result.to_string());
        }
    }
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(_input: String) -> FnResult<String> {
    Ok(String::new())
}

// --- Pure logic (testable on native) ---

/// Format a large number with comma separators.
fn format_number(n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Build widget content from Pi-hole summary data.
fn build_summary_content(summary: &PiholeSummary) -> serde_json::Value {
    let status_icon = if summary.status == "enabled" { "🟢" } else { "🔴" };

    let mut pairs: Vec<serde_json::Value> = vec![
        json!({"key": "Status", "value": format!("{} {}", status_icon, summary.status)}),
        json!({"key": "Queries Today", "value": format_number(summary.dns_queries_today)}),
        json!({"key": "Blocked Today", "value": format!(
            "{} ({:.1}%)",
            format_number(summary.ads_blocked_today),
            summary.ads_percentage_today
        )}),
        json!({"key": "Domains on Blocklist", "value": format_number(summary.domains_being_blocked)}),
    ];

    if summary.unique_domains > 0 {
        pairs.push(json!({"key": "Unique Domains", "value": format_number(summary.unique_domains)}));
    }

    if summary.queries_forwarded > 0 || summary.queries_cached > 0 {
        pairs.push(json!({"key": "Forwarded", "value": format_number(summary.queries_forwarded)}));
        pairs.push(json!({"key": "Cached", "value": format_number(summary.queries_cached)}));
    }

    if summary.unique_clients > 0 {
        pairs.push(json!({"key": "Clients", "value": format_number(summary.unique_clients)}));
    }

    if let Some(ref gravity) = summary.gravity_last_updated {
        if let Some(ref rel) = gravity.relative {
            let gravity_str = if rel.days > 0 {
                format!("{}d {}h ago", rel.days, rel.hours)
            } else if rel.hours > 0 {
                format!("{}h {}m ago", rel.hours, rel.minutes)
            } else {
                format!("{}m ago", rel.minutes)
            };
            pairs.push(json!({"key": "Gravity Updated", "value": gravity_str}));
        }
    }

    json!({
        "type": "key_value",
        "pairs": pairs
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number_zero() {
        assert_eq!(format_number(0), "0");
    }

    #[test]
    fn test_format_number_small() {
        assert_eq!(format_number(42), "42");
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn test_format_number_thousands() {
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(99867), "99,867");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_build_summary_enabled() {
        let summary = PiholeSummary {
            domains_being_blocked: 99867,
            dns_queries_today: 2275,
            ads_blocked_today: 422,
            ads_percentage_today: 18.55,
            status: "enabled".to_string(),
            ..Default::default()
        };
        let content = build_summary_content(&summary);
        assert_eq!(content["type"], "key_value");
        let pairs = content["pairs"].as_array().unwrap();
        assert!(pairs.len() >= 4);
        assert!(pairs[0]["value"].as_str().unwrap().contains("🟢"));
        assert!(pairs[0]["value"].as_str().unwrap().contains("enabled"));
        assert_eq!(pairs[1]["value"], "2,275");
        assert!(pairs[2]["value"].as_str().unwrap().contains("422"));
        assert!(pairs[2]["value"].as_str().unwrap().contains("18.6"));
        assert_eq!(pairs[3]["value"], "99,867");
    }

    #[test]
    fn test_build_summary_disabled() {
        let summary = PiholeSummary {
            status: "disabled".to_string(),
            ..Default::default()
        };
        let content = build_summary_content(&summary);
        let pairs = content["pairs"].as_array().unwrap();
        assert!(pairs[0]["value"].as_str().unwrap().contains("🔴"));
    }

    #[test]
    fn test_build_summary_with_clients() {
        let summary = PiholeSummary {
            dns_queries_today: 5000,
            ads_blocked_today: 1000,
            ads_percentage_today: 20.0,
            domains_being_blocked: 50000,
            unique_clients: 12,
            status: "enabled".to_string(),
            ..Default::default()
        };
        let content = build_summary_content(&summary);
        let pairs = content["pairs"].as_array().unwrap();
        let has_clients = pairs.iter().any(|p| p["key"] == "Clients" && p["value"] == "12");
        assert!(has_clients);
    }

    #[test]
    fn test_build_summary_with_forwarded_cached() {
        let summary = PiholeSummary {
            dns_queries_today: 10000,
            ads_blocked_today: 2000,
            ads_percentage_today: 20.0,
            domains_being_blocked: 100000,
            queries_forwarded: 6000,
            queries_cached: 2000,
            status: "enabled".to_string(),
            ..Default::default()
        };
        let content = build_summary_content(&summary);
        let pairs = content["pairs"].as_array().unwrap();
        let has_forwarded = pairs.iter().any(|p| p["key"] == "Forwarded" && p["value"] == "6,000");
        let has_cached = pairs.iter().any(|p| p["key"] == "Cached" && p["value"] == "2,000");
        assert!(has_forwarded);
        assert!(has_cached);
    }

    #[test]
    fn test_build_summary_with_gravity_days() {
        let summary = PiholeSummary {
            status: "enabled".to_string(),
            gravity_last_updated: Some(GravityUpdate {
                relative: Some(GravityRelative { days: 2, hours: 5, minutes: 30 }),
            }),
            ..Default::default()
        };
        let content = build_summary_content(&summary);
        let pairs = content["pairs"].as_array().unwrap();
        let gravity = pairs.iter().find(|p| p["key"] == "Gravity Updated").unwrap();
        assert_eq!(gravity["value"], "2d 5h ago");
    }

    #[test]
    fn test_build_summary_with_gravity_hours() {
        let summary = PiholeSummary {
            status: "enabled".to_string(),
            gravity_last_updated: Some(GravityUpdate {
                relative: Some(GravityRelative { days: 0, hours: 3, minutes: 15 }),
            }),
            ..Default::default()
        };
        let content = build_summary_content(&summary);
        let pairs = content["pairs"].as_array().unwrap();
        let gravity = pairs.iter().find(|p| p["key"] == "Gravity Updated").unwrap();
        assert_eq!(gravity["value"], "3h 15m ago");
    }

    #[test]
    fn test_build_summary_with_gravity_minutes() {
        let summary = PiholeSummary {
            status: "enabled".to_string(),
            gravity_last_updated: Some(GravityUpdate {
                relative: Some(GravityRelative { days: 0, hours: 0, minutes: 42 }),
            }),
            ..Default::default()
        };
        let content = build_summary_content(&summary);
        let pairs = content["pairs"].as_array().unwrap();
        let gravity = pairs.iter().find(|p| p["key"] == "Gravity Updated").unwrap();
        assert_eq!(gravity["value"], "42m ago");
    }

    #[test]
    fn test_parse_pihole_json() {
        let json_str = r#"{
            "domains_being_blocked": 99867,
            "dns_queries_today": 2275,
            "ads_blocked_today": 422,
            "ads_percentage_today": 18.549450,
            "unique_domains": 1500,
            "queries_forwarded": 1200,
            "queries_cached": 653,
            "clients_ever_seen": 15,
            "unique_clients": 8,
            "dns_queries_all_types": 2275,
            "reply_NODATA": 50,
            "reply_NXDOMAIN": 30,
            "reply_CNAME": 100,
            "reply_IP": 1095,
            "status": "enabled"
        }"#;
        let summary: PiholeSummary = serde_json::from_str(json_str).unwrap();
        assert_eq!(summary.domains_being_blocked, 99867);
        assert_eq!(summary.dns_queries_today, 2275);
        assert_eq!(summary.ads_blocked_today, 422);
        assert!((summary.ads_percentage_today - 18.549450).abs() < 0.001);
        assert_eq!(summary.status, "enabled");
        assert_eq!(summary.unique_clients, 8);
    }

    #[test]
    fn test_build_summary_with_unique_domains() {
        let summary = PiholeSummary {
            dns_queries_today: 5000,
            ads_blocked_today: 1000,
            ads_percentage_today: 20.0,
            domains_being_blocked: 50000,
            unique_domains: 1234,
            status: "enabled".to_string(),
            ..Default::default()
        };
        let content = build_summary_content(&summary);
        let pairs = content["pairs"].as_array().unwrap();
        let has_unique = pairs.iter().any(|p| p["key"] == "Unique Domains" && p["value"] == "1,234");
        assert!(has_unique);
    }
}
