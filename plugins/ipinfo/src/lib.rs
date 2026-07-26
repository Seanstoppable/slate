use extism_pdk::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct IpInfoResponse {
    ip: String,
    #[serde(default)]
    city: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    org: String,
    #[serde(default)]
    timezone: String,
}

#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "IP Info",
        "description": "Shows public IP and geolocation",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[plugin_fn]
pub fn refresh(_input: String) -> FnResult<String> {
    // Make HTTP request to ipinfo.io via host function
    let req = HttpRequest::new("https://ipinfo.io/json")
        .with_header("Accept", "application/json");

    let response = http::request::<String>(&req, None)?;
    let body = response.body();
    let body_str = std::str::from_utf8(&body).unwrap_or("{}");

    let content = match serde_json::from_str::<IpInfoResponse>(body_str) {
        Ok(info) => {
            json!({
                "type": "key_value",
                "pairs": [
                    ["IP", {"text": info.ip}],
                    ["Location", {"text": format!("{}, {}, {}", info.city, info.region, info.country)}],
                    ["Org", {"text": info.org}],
                    ["Timezone", {"text": info.timezone}]
                ]
            })
        }
        Err(e) => {
            json!({
                "type": "text",
                "content": format!("Error parsing response: {}", e),
                "scrollable": false,
                "wrap": true
            })
        }
    };

    Ok(content.to_string())
}

#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}
