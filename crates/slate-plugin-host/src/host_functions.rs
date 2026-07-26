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
pub async fn http_request(
    guard: &PermissionGuard,
    req: HttpRequest,
) -> Result<HttpResponse> {
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
