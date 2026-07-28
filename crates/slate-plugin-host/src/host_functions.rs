use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::permissions::PermissionGuard;

/// HTTP request structure for the http_request host function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

/// HTTP response returned from the http_request host function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// Execute an HTTP request, checking permissions first.
pub async fn http_request(guard: &PermissionGuard, req: HttpRequest) -> Result<HttpResponse> {
    // Extract host from URL for permission check
    let host = extract_host(&req.url)?;
    guard.check_network(&host)?;

    let client = reqwest::Client::new();
    let mut builder = match req.method.to_uppercase().as_str() {
        "GET" => client.get(&req.url),
        "POST" => client.post(&req.url),
        "PUT" => client.put(&req.url),
        "DELETE" => client.delete(&req.url),
        "PATCH" => client.patch(&req.url),
        _ => anyhow::bail!("Unsupported HTTP method: {}", req.method),
    };

    for (key, value) in &req.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    if let Some(body) = req.body {
        builder = builder.body(body);
    }

    let response = builder.send().await?;
    let status = response.status().as_u16();
    let headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body = response.text().await?;

    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn extract_host(url: &str) -> Result<String> {
    let url = reqwest::Url::parse(url)?;
    url.host_str()
        .map(|h| h.to_string())
        .ok_or_else(|| anyhow::anyhow!("No host in URL"))
}

/// Key-value store for plugins (sandboxed per-plugin).
#[derive(Debug, Default)]
pub struct PluginStore {
    data: HashMap<String, Vec<u8>>,
}

impl PluginStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.data.get(key).map(|v| v.as_slice())
    }

    pub fn set(&mut self, key: &str, value: Vec<u8>) {
        self.data.insert(key.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::PermissionGuard;
    use serde_json::json;
    use slate_plugin_sdk::Permissions;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn plugin_store_starts_empty() {
        let store = PluginStore::new();
        assert_eq!(store.get("missing"), None);
    }

    #[test]
    fn plugin_store_gets_and_sets_values() {
        let mut store = PluginStore::new();
        store.set("token", vec![1, 2, 3]);

        assert_eq!(store.get("token"), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn plugin_store_overwrites_existing_values() {
        let mut store = PluginStore::new();
        store.set("token", vec![1, 2, 3]);
        store.set("token", vec![4, 5]);

        assert_eq!(store.get("token"), Some(&[4, 5][..]));
    }

    #[test]
    fn plugin_store_keeps_keys_isolated_and_owns_values() {
        let mut store = PluginStore::new();
        let mut value = vec![9, 8, 7];
        store.set("alpha", value.clone());
        store.set("beta", vec![1, 2, 3]);
        value[0] = 0;

        assert_eq!(store.get("alpha"), Some(&[9, 8, 7][..]));
        assert_eq!(store.get("beta"), Some(&[1, 2, 3][..]));
        assert_eq!(store.get("gamma"), None);
    }

    #[test]
    fn extract_host_returns_host_for_valid_urls() {
        assert_eq!(extract_host("https://example.com").unwrap(), "example.com");
        assert_eq!(
            extract_host("https://example.com:8080/api/v1").unwrap(),
            "example.com"
        );
        assert_eq!(
            extract_host("http://subdomain.example.com/path?q=1").unwrap(),
            "subdomain.example.com"
        );
        assert_eq!(
            extract_host("https://user:pass@localhost:3000/dashboard").unwrap(),
            "localhost"
        );
        assert_eq!(extract_host("http://127.0.0.1:8080").unwrap(), "127.0.0.1");
    }

    #[test]
    fn extract_host_errors_when_url_has_no_host() {
        let err = extract_host("mailto:test@example.com").unwrap_err();
        assert!(err.to_string().contains("No host"));
    }

    #[test]
    fn default_method_returns_get() {
        assert_eq!(default_method(), "GET");
    }

    #[test]
    fn http_request_default_method_applies_when_method_is_missing() {
        let request: HttpRequest = serde_json::from_value(json!({
            "url": "https://example.com"
        }))
        .unwrap();

        assert_eq!(request.method, "GET");
        assert!(request.headers.is_empty());
        assert!(request.body.is_none());
    }

    #[test]
    fn http_request_deserializes_with_default_method() {
        let request: HttpRequest = serde_json::from_value(json!({
            "url": "https://example.com",
            "headers": { "Accept": "application/json" }
        }))
        .unwrap();

        assert_eq!(request.method, "GET");
        assert_eq!(
            request.headers.get("Accept").map(String::as_str),
            Some("application/json")
        );
        assert_eq!(request.body, None);
    }

    #[test]
    fn http_request_deserializes_explicit_fields() {
        let request: HttpRequest = serde_json::from_value(json!({
            "url": "https://example.com/api",
            "method": "POST",
            "headers": { "Content-Type": "application/json" },
            "body": "{\"ok\":true}"
        }))
        .unwrap();

        assert_eq!(request.url, "https://example.com/api");
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.headers.get("Content-Type").map(String::as_str),
            Some("application/json")
        );
        assert_eq!(request.body.as_deref(), Some("{\"ok\":true}"));
    }

    #[tokio::test]
    async fn http_request_denies_unauthorized_host() {
        let guard = PermissionGuard::new(Permissions {
            network: vec!["allowed.com".to_string()],
            ..Default::default()
        });
        let request = HttpRequest {
            url: "https://denied.com/api".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        };

        let error = http_request(&guard, request).await.unwrap_err();
        assert!(error.to_string().contains("network access denied"));
    }

    #[tokio::test]
    async fn http_request_rejects_unsupported_methods_before_sending() {
        let guard = PermissionGuard::new(Permissions {
            network: vec!["example.com".to_string()],
            ..Default::default()
        });
        let request = HttpRequest {
            url: "https://example.com/api".to_string(),
            method: "TRACE".to_string(),
            headers: HashMap::new(),
            body: None,
        };

        let error = http_request(&guard, request).await.unwrap_err();
        assert!(error.to_string().contains("Unsupported HTTP method"));
    }

    #[tokio::test]
    async fn http_request_sends_headers_and_body_to_allowed_host() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let response =
            "HTTP/1.1 200 OK\r\ncontent-length: 5\r\nx-test: ok\r\nconnection: close\r\n\r\nhello";

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 2048];
            let read = stream.read(&mut request).await.unwrap();
            let request_text = String::from_utf8_lossy(&request[..read]);
            assert!(request_text.contains("POST /api HTTP/1.1"));
            assert!(request_text.contains("x-demo: yes"));
            assert!(request_text.ends_with("payload"));
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let guard = PermissionGuard::new(Permissions {
            network: vec!["127.0.0.1".to_string()],
            ..Default::default()
        });
        let result = http_request(
            &guard,
            HttpRequest {
                url: format!("http://{addr}/api"),
                method: "POST".to_string(),
                headers: HashMap::from([("x-demo".to_string(), "yes".to_string())]),
                body: Some("payload".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(result.status, 200);
        assert_eq!(result.body, "hello");
        assert_eq!(result.headers.get("x-test").map(String::as_str), Some("ok"));
    }
}
