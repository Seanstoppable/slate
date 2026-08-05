#[cfg(target_arch = "wasm32")]
use extism_pdk::*;

#[cfg(target_arch = "wasm32")]
use serde::Deserialize;
use serde_json::json;

#[cfg(target_arch = "wasm32")]
#[host_fn("extism:host/user")]
extern "ExtismHost" {
    fn safe_http_request(input: String) -> String;
}

/// Sentinel status code used when a URL could not be checked at all
/// (invalid URL, connection error, or timeout). Mirrors wtfutil's
/// `InvalidResultCode`.
const INVALID_STATUS_CODE: u16 = 999;

/// The result of checking a single URL.
#[derive(Debug, Clone, PartialEq)]
struct UrlCheckResult {
    url: String,
    is_valid: bool,
    status_code: u16,
    message: String,
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn metadata(_input: String) -> FnResult<String> {
    let meta = json!({
        "name": "URL Check",
        "description": "Checks the reachability of a list of URLs",
        "version": env!("CARGO_PKG_VERSION"),
        "author": "Slate Community"
    });
    Ok(meta.to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn refresh(input: String) -> FnResult<String> {
    let settings: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let urls: Vec<String> = settings["urls"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let results: Vec<UrlCheckResult> = urls.iter().map(|url| check_url(url)).collect();
    let content = build_content(&results);
    Ok(content.to_string())
}

#[cfg(target_arch = "wasm32")]
fn check_url(url: &str) -> UrlCheckResult {
    if !is_valid_url(url) {
        return UrlCheckResult {
            url: url.to_string(),
            is_valid: false,
            status_code: INVALID_STATUS_CODE,
            message: "Invalid URL".to_string(),
        };
    }

    match call_safe_http_request(url, "HEAD") {
        Ok(SafeHttpResponse { ok: true, status }) => {
            let status_code = status.unwrap_or(INVALID_STATUS_CODE);
            let message = if status_code < 400 {
                "OK".to_string()
            } else {
                format!("HTTP {}", status_code)
            };
            UrlCheckResult {
                url: url.to_string(),
                is_valid: true,
                status_code,
                message,
            }
        }
        Ok(SafeHttpResponse { ok: false, .. }) | Err(_) => UrlCheckResult {
            url: url.to_string(),
            is_valid: true,
            status_code: INVALID_STATUS_CODE,
            message: "Unreachable".to_string(),
        },
    }
}

/// Result of the host's `safe_http_request` call, which never traps:
/// network failures (DNS, connection refused, TLS, timeout) come back as
/// `{"ok": false, "error": "..."}` instead of aborting the whole plugin
/// call the way Extism's built-in `http_request` host function does.
#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
struct SafeHttpResponse {
    ok: bool,
    status: Option<u16>,
}

/// Call the host-provided `safe_http_request` function, which performs
/// the HTTP request outside the WASM sandbox and always returns a JSON
/// result rather than trapping the plugin call on network failure.
#[cfg(target_arch = "wasm32")]
fn call_safe_http_request(url: &str, method: &str) -> Result<SafeHttpResponse, Error> {
    let request = json!({
        "url": url,
        "method": method
    })
    .to_string();

    let response = unsafe { safe_http_request(request)? };
    serde_json::from_str(&response)
        .map_err(|e| Error::msg(format!("Failed to parse safe_http_request result: {}", e)))
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_key(_input: String) -> FnResult<String> {
    Ok(String::new())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_action(input: String) -> FnResult<String> {
    #[derive(Deserialize)]
    struct ActionInput {
        action_id: String,
        item_id: String,
    }

    if let Ok(action) = serde_json::from_str::<ActionInput>(&input) {
        if let Some(url) = build_open_action(&action.action_id, &action.item_id) {
            return Ok(json!({"open_url": url}).to_string());
        }
    }
    Ok(String::new())
}

// --- Pure logic (testable on native) ---

/// A URL is considered valid if it declares an `http://` or `https://`
/// scheme and has a non-empty host component.
fn is_valid_url(url: &str) -> bool {
    let rest = if let Some(stripped) = url.strip_prefix("https://") {
        stripped
    } else if let Some(stripped) = url.strip_prefix("http://") {
        stripped
    } else {
        return false;
    };
    !rest.is_empty()
}

/// Determine the status icon for a check result, matching wtfutil's
/// green (2xx-4xx) / red (invalid or 5xx+) coloring.
fn status_icon(result: &UrlCheckResult) -> &'static str {
    if result.is_valid && result.status_code < 500 {
        "🟢"
    } else {
        "🔴"
    }
}

/// Format the status code for display, using `---` for the invalid
/// sentinel (mirrors wtfutil's template behavior).
fn format_status_code(status_code: u16) -> String {
    if status_code == INVALID_STATUS_CODE {
        "---".to_string()
    } else {
        status_code.to_string()
    }
}

/// Build a single list item from a check result.
fn build_item(result: &UrlCheckResult) -> serde_json::Value {
    json!({
        "id": result.url,
        "title": format!("{} [{}] {}", status_icon(result), format_status_code(result.status_code), result.url),
        "subtitle": result.message,
        "style": {}
    })
}

/// Build the full widget content from a list of check results.
fn build_content(results: &[UrlCheckResult]) -> serde_json::Value {
    if results.is_empty() {
        return json!({
            "type": "text",
            "text": "No URLs configured"
        });
    }

    let items: Vec<serde_json::Value> = results.iter().map(build_item).collect();

    json!({
        "type": "list",
        "items": items,
        "selectable": true,
        "actions": [
            {"id": "open", "label": "Open in browser", "key": "o", "confirm": false}
        ]
    })
}

/// Map an on_action request to a URL to open. `item_id` is the checked
/// URL itself (set as the item's `id` in `build_item`).
fn build_open_action(action_id: &str, item_id: &str) -> Option<String> {
    match action_id {
        "open" | "select" => Some(item_id.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_url_https() {
        assert!(is_valid_url("https://example.com"));
    }

    #[test]
    fn test_is_valid_url_http() {
        assert!(is_valid_url("http://example.com"));
    }

    #[test]
    fn test_is_valid_url_no_scheme() {
        assert!(!is_valid_url("example.com"));
    }

    #[test]
    fn test_is_valid_url_ftp_scheme() {
        assert!(!is_valid_url("ftp://example.com"));
    }

    #[test]
    fn test_is_valid_url_empty_host() {
        assert!(!is_valid_url("https://"));
    }

    #[test]
    fn test_is_valid_url_empty_string() {
        assert!(!is_valid_url(""));
    }

    #[test]
    fn test_status_icon_ok() {
        let result = UrlCheckResult {
            url: "https://example.com".to_string(),
            is_valid: true,
            status_code: 200,
            message: "OK".to_string(),
        };
        assert_eq!(status_icon(&result), "🟢");
    }

    #[test]
    fn test_status_icon_client_error_still_green() {
        // wtfutil colors anything below 500 green, including 4xx
        let result = UrlCheckResult {
            url: "https://example.com".to_string(),
            is_valid: true,
            status_code: 404,
            message: "HTTP 404".to_string(),
        };
        assert_eq!(status_icon(&result), "🟢");
    }

    #[test]
    fn test_status_icon_server_error() {
        let result = UrlCheckResult {
            url: "https://example.com".to_string(),
            is_valid: true,
            status_code: 500,
            message: "HTTP 500".to_string(),
        };
        assert_eq!(status_icon(&result), "🔴");
    }

    #[test]
    fn test_status_icon_invalid() {
        let result = UrlCheckResult {
            url: "not-a-url".to_string(),
            is_valid: false,
            status_code: INVALID_STATUS_CODE,
            message: "Invalid URL".to_string(),
        };
        assert_eq!(status_icon(&result), "🔴");
    }

    #[test]
    fn test_format_status_code_normal() {
        assert_eq!(format_status_code(200), "200");
        assert_eq!(format_status_code(404), "404");
    }

    #[test]
    fn test_format_status_code_invalid() {
        assert_eq!(format_status_code(INVALID_STATUS_CODE), "---");
    }

    #[test]
    fn test_build_item_ok() {
        let result = UrlCheckResult {
            url: "https://example.com".to_string(),
            is_valid: true,
            status_code: 200,
            message: "OK".to_string(),
        };
        let item = build_item(&result);
        assert_eq!(item["id"], "https://example.com");
        assert_eq!(item["title"], "🟢 [200] https://example.com");
        assert_eq!(item["subtitle"], "OK");
    }

    #[test]
    fn test_build_item_invalid() {
        let result = UrlCheckResult {
            url: "not-a-url".to_string(),
            is_valid: false,
            status_code: INVALID_STATUS_CODE,
            message: "Invalid URL".to_string(),
        };
        let item = build_item(&result);
        assert_eq!(item["title"], "🔴 [---] not-a-url");
        assert_eq!(item["subtitle"], "Invalid URL");
    }

    #[test]
    fn test_build_content_empty() {
        let content = build_content(&[]);
        assert_eq!(content["type"], "text");
    }

    #[test]
    fn test_build_content_with_results() {
        let results = vec![
            UrlCheckResult {
                url: "https://example.com".to_string(),
                is_valid: true,
                status_code: 200,
                message: "OK".to_string(),
            },
            UrlCheckResult {
                url: "https://down.example.com".to_string(),
                is_valid: true,
                status_code: 503,
                message: "HTTP 503".to_string(),
            },
        ];
        let content = build_content(&results);
        assert_eq!(content["type"], "list");
        assert_eq!(content["selectable"], true);
        let items = content["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        let actions = content["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["id"], "open");
    }

    #[test]
    fn test_build_open_action_open() {
        assert_eq!(
            build_open_action("open", "https://example.com"),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn test_build_open_action_select() {
        assert_eq!(
            build_open_action("select", "https://example.com"),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn test_build_open_action_unknown() {
        assert_eq!(build_open_action("delete", "https://example.com"), None);
    }
}
