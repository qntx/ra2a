//! HTTP helper functions for the A2A SDK.
//!
//! Provides utility functions for URL manipulation and HTTP header handling.

#![allow(dead_code)]

use std::collections::HashMap;

/// Extracts query parameters from a URL (simple implementation).
pub fn parse_query_params(url: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    if let Some(query_start) = url.find('?') {
        let query = &url[query_start + 1..];
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                params.insert(key.to_string(), value.to_string());
            }
        }
    }
    params
}

/// Builds a URL with query parameters.
pub fn build_url_with_params(base: &str, params: &HashMap<String, String>) -> String {
    if params.is_empty() {
        return base.to_string();
    }
    let query: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
    format!("{}?{}", base.trim_end_matches('?'), query.join("&"))
}

/// Extension header name for A2A protocol.
pub const HTTP_EXTENSION_HEADER: &str = "A2A-Extensions";

/// Updates extension header value by adding new extensions.
pub fn update_extension_header(existing: Option<&str>, new_extensions: &[String]) -> String {
    let mut extensions: Vec<&str> = existing
        .map(|e| e.split(',').map(str::trim).collect())
        .unwrap_or_default();

    for ext in new_extensions {
        if !extensions.contains(&ext.as_str()) {
            extensions.push(ext);
        }
    }

    extensions.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query_params() {
        let url = "https://example.com/path?foo=bar&baz=qux";
        let params = parse_query_params(url);
        assert_eq!(params.get("foo"), Some(&"bar".to_string()));
        assert_eq!(params.get("baz"), Some(&"qux".to_string()));
    }

    #[test]
    fn test_parse_query_params_empty() {
        let url = "https://example.com/path";
        let params = parse_query_params(url);
        assert!(params.is_empty());
    }

    #[test]
    fn test_build_url_with_params() {
        let mut params = HashMap::new();
        params.insert("foo".to_string(), "bar".to_string());
        let url = build_url_with_params("https://example.com", &params);
        assert!(url.contains("foo=bar"));
    }

    #[test]
    fn test_update_extension_header() {
        let existing = Some("ext1, ext2");
        let new = vec!["ext3".to_string()];
        let result = update_extension_header(existing, &new);
        assert!(result.contains("ext1"));
        assert!(result.contains("ext3"));
    }
}
