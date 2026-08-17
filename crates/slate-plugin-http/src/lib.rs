//! Shared HTTP GET helpers for Slate WASM plugins.
//!
//! Every WASM plugin that talks to an HTTP API repeats the same
//! boilerplate: build an `extism_pdk::HttpRequest`, chain headers onto
//! it, issue the request, reject a non-success status, and decode the
//! body as UTF-8 (optionally then as JSON). This crate centralizes that
//! so plugins only need to supply a URL, a list of headers, and (for
//! [`get_json`]) a target type.
//!
//! [`build_request`] and [`status_error`] are pure and compile on any
//! target, so they are unit tested natively. [`get_text`] and
//! [`get_json`] actually perform the HTTP request via `extism_pdk::http`
//! and are only available when compiling for `wasm32`, matching how
//! every Slate WASM plugin gates its own host-calling code.

use extism_manifest::HttpRequest;

/// Build an `HttpRequest` for `url`, chaining `headers` onto it in
/// order. Later entries for the same header name override earlier ones,
/// matching `HttpRequest::with_header`'s own behavior.
pub fn build_request(url: &str, headers: &[(&str, &str)]) -> HttpRequest {
    let mut request = HttpRequest::new(url);
    for (key, value) in headers {
        request = request.with_header(*key, *value);
    }
    request
}

/// Describe a non-success (i.e. non-2xx) HTTP status as an error
/// message, or `None` if `status` indicates success.
pub fn status_error(status: u16) -> Option<String> {
    if (200..300).contains(&status) {
        None
    } else {
        Some(format!("HTTP request failed with status {}", status))
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::{build_request, status_error};
    use extism_pdk::{http, Error};
    use serde::de::DeserializeOwned;

    /// Perform an HTTP GET to `url` with `headers`, returning the
    /// response body decoded as UTF-8 text.
    ///
    /// Returns an error if the request itself fails, the response
    /// status is not in the 2xx range, or the body is not valid UTF-8.
    pub fn get_text(url: &str, headers: &[(&str, &str)]) -> Result<String, Error> {
        let request = build_request(url, headers);
        let response = http::request::<String>(&request, None)?;

        if let Some(message) = status_error(response.status_code()) {
            return Err(Error::msg(message));
        }

        String::from_utf8(response.body())
            .map_err(|e| Error::msg(format!("Response body was not valid UTF-8: {}", e)))
    }

    /// Perform an HTTP GET to `url` with `headers`, deserializing the
    /// JSON response body into `T`.
    ///
    /// Returns an error under the same conditions as [`get_text`], plus
    /// when the response body is not valid JSON for `T`.
    pub fn get_json<T: DeserializeOwned>(url: &str, headers: &[(&str, &str)]) -> Result<T, Error> {
        let body = get_text(url, headers)?;
        serde_json::from_str(&body).map_err(Error::from)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{get_json, get_text};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_sets_url_with_no_headers() {
        let req = build_request("https://example.com", &[]);
        assert_eq!(req.url, "https://example.com");
        assert!(req.headers.is_empty());
        assert_eq!(req.method, None);
    }

    #[test]
    fn build_request_chains_headers_in_order() {
        let req = build_request(
            "https://example.com",
            &[("Accept", "application/json"), ("User-Agent", "slate/0.1")],
        );
        assert_eq!(
            req.headers.get("Accept"),
            Some(&"application/json".to_string())
        );
        assert_eq!(
            req.headers.get("User-Agent"),
            Some(&"slate/0.1".to_string())
        );
        assert_eq!(req.headers.len(), 2);
    }

    #[test]
    fn build_request_later_duplicate_header_overrides_earlier() {
        let req = build_request(
            "https://example.com",
            &[("X-Test", "one"), ("X-Test", "two")],
        );
        assert_eq!(req.headers.get("X-Test"), Some(&"two".to_string()));
        assert_eq!(req.headers.len(), 1);
    }

    #[test]
    fn status_error_none_for_2xx() {
        assert_eq!(status_error(200), None);
        assert_eq!(status_error(204), None);
        assert_eq!(status_error(299), None);
    }

    #[test]
    fn status_error_some_below_2xx() {
        assert!(status_error(199).is_some());
        assert!(status_error(101).is_some());
    }

    #[test]
    fn status_error_some_for_redirects_and_client_and_server_errors() {
        assert!(status_error(301).is_some());
        assert!(status_error(404).is_some());
        assert!(status_error(500).is_some());
    }

    #[test]
    fn status_error_message_includes_status_code() {
        let message = status_error(503).unwrap();
        assert!(message.contains("503"));
    }
}
