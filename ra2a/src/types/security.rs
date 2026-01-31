//! Security scheme types for the A2A protocol.
//!
//! Defines security schemes that can be used to secure agent endpoints,
//! following the OpenAPI 3.0 Security Scheme Object specification.

use serde::{Deserialize, Serialize};

use super::OAuthFlows;

/// Defines a security scheme that can be used to secure agent endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SecurityScheme {
    /// API Key security scheme.
    ApiKey(APIKeySecurityScheme),
    /// HTTP authentication security scheme.
    Http(HTTPAuthSecurityScheme),
    /// OAuth 2.0 security scheme.
    #[serde(rename = "oauth2")]
    OAuth2(OAuth2SecurityScheme),
    /// OpenID Connect security scheme.
    OpenIdConnect(OpenIdConnectSecurityScheme),
    /// Mutual TLS security scheme.
    MutualTLS(MutualTLSSecurityScheme),
}

/// The location of the API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    /// API key in a cookie.
    Cookie,
    /// API key in a header.
    Header,
    /// API key in a query parameter.
    Query,
}

/// Defines a security scheme using an API key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct APIKeySecurityScheme {
    /// The name of the header, query, or cookie parameter.
    pub name: String,
    /// The location of the API key.
    #[serde(rename = "in")]
    pub location: ApiKeyLocation,
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl APIKeySecurityScheme {
    /// Creates a new API key security scheme.
    pub fn new(name: impl Into<String>, location: ApiKeyLocation) -> Self {
        Self {
            name: name.into(),
            location,
            description: None,
        }
    }

    /// Creates a header-based API key scheme.
    pub fn header(name: impl Into<String>) -> Self {
        Self::new(name, ApiKeyLocation::Header)
    }

    /// Creates a query-based API key scheme.
    pub fn query(name: impl Into<String>) -> Self {
        Self::new(name, ApiKeyLocation::Query)
    }
}

/// Defines a security scheme using HTTP authentication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HTTPAuthSecurityScheme {
    /// The name of the HTTP Authentication scheme (e.g., "Bearer").
    pub scheme: String,
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A hint to identify how the bearer token is formatted (e.g., "JWT").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_format: Option<String>,
}

impl HTTPAuthSecurityScheme {
    /// Creates a new HTTP authentication security scheme.
    pub fn new(scheme: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into(),
            description: None,
            bearer_format: None,
        }
    }

    /// Creates a Bearer token authentication scheme.
    pub fn bearer() -> Self {
        Self::new("Bearer")
    }

    /// Creates a Bearer JWT authentication scheme.
    pub fn bearer_jwt() -> Self {
        Self {
            scheme: "Bearer".to_string(),
            description: None,
            bearer_format: Some("JWT".to_string()),
        }
    }

    /// Creates a Basic authentication scheme.
    pub fn basic() -> Self {
        Self::new("Basic")
    }
}

/// Defines a security scheme using OAuth 2.0.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OAuth2SecurityScheme {
    /// Configuration for the supported OAuth 2.0 flows.
    pub flows: OAuthFlows,
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// URL to the OAuth2 authorization server metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth2_metadata_url: Option<String>,
}

/// Defines a security scheme using OpenID Connect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenIdConnectSecurityScheme {
    /// The OpenID Connect Discovery URL.
    pub open_id_connect_url: String,
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl OpenIdConnectSecurityScheme {
    /// Creates a new OpenID Connect security scheme.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            open_id_connect_url: url.into(),
            description: None,
        }
    }
}

/// Defines a security scheme using mTLS authentication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MutualTLSSecurityScheme {
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Default for MutualTLSSecurityScheme {
    fn default() -> Self {
        Self { description: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_security_scheme() {
        let scheme = APIKeySecurityScheme::header("X-API-Key");
        assert_eq!(scheme.name, "X-API-Key");
        assert_eq!(scheme.location, ApiKeyLocation::Header);
    }

    #[test]
    fn test_http_auth_scheme() {
        let scheme = HTTPAuthSecurityScheme::bearer_jwt();
        assert_eq!(scheme.scheme, "Bearer");
        assert_eq!(scheme.bearer_format, Some("JWT".to_string()));
    }

    #[test]
    fn test_security_scheme_serialization() {
        let scheme = SecurityScheme::ApiKey(APIKeySecurityScheme::header("X-API-Key"));
        let json = serde_json::to_string(&scheme).unwrap();
        assert!(json.contains("\"type\":\"apiKey\""));
    }
}
