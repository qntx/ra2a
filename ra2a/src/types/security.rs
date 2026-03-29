//! Security scheme types for the A2A protocol.
//!
//! Maps to proto `SecurityScheme`, `SecurityRequirement`, and all OAuth flow types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A security scheme that can be used to secure an agent's endpoints.
///
/// Maps to proto `SecurityScheme` (oneof scheme). Serialized with the `type`
/// field as discriminator per OpenAPI 3.2 convention.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SecurityScheme {
    /// API key-based authentication.
    ApiKey(ApiKeySecurityScheme),
    /// HTTP authentication (Basic, Bearer, etc.).
    Http(HttpAuthSecurityScheme),
    /// OAuth 2.0 authentication.
    #[serde(rename = "oauth2")]
    OAuth2(Box<OAuth2SecurityScheme>),
    /// OpenID Connect authentication.
    OpenIdConnect(OpenIdConnectSecurityScheme),
    /// Mutual TLS authentication.
    MutualTLS(MutualTlsSecurityScheme),
}

/// The location of an API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    /// In a query parameter.
    Query,
    /// In a header.
    Header,
    /// In a cookie.
    Cookie,
}

/// API key-based security scheme.
///
/// Maps to proto `APIKeySecurityScheme`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiKeySecurityScheme {
    /// An optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The location of the API key.
    #[serde(rename = "in")]
    pub location: ApiKeyLocation,
    /// The name of the header, query, or cookie parameter.
    pub name: String,
}

/// HTTP authentication security scheme.
///
/// Maps to proto `HTTPAuthSecurityScheme`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HttpAuthSecurityScheme {
    /// An optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The HTTP authentication scheme name (e.g. `"Bearer"`).
    pub scheme: String,
    /// A hint for how the bearer token is formatted (e.g. `"JWT"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_format: Option<String>,
}

impl HttpAuthSecurityScheme {
    /// Creates a Bearer token authentication scheme.
    #[must_use]
    pub fn bearer() -> Self {
        Self {
            description: None,
            scheme: "Bearer".into(),
            bearer_format: None,
        }
    }

    /// Creates a Bearer JWT authentication scheme.
    #[must_use]
    pub fn bearer_jwt() -> Self {
        Self {
            description: None,
            scheme: "Bearer".into(),
            bearer_format: Some("JWT".into()),
        }
    }
}

/// OAuth 2.0 security scheme.
///
/// Maps to proto `OAuth2SecurityScheme`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2SecurityScheme {
    /// An optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Configuration for the supported OAuth 2.0 flow.
    pub flows: OAuthFlow,
    /// URL to the OAuth2 authorization server metadata (RFC 8414).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth2_metadata_url: Option<String>,
}

/// OpenID Connect security scheme.
///
/// Maps to proto `OpenIdConnectSecurityScheme`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenIdConnectSecurityScheme {
    /// An optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The OpenID Connect Discovery URL.
    pub open_id_connect_url: String,
}

/// Mutual TLS security scheme.
///
/// Maps to proto `MutualTlsSecurityScheme`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MutualTlsSecurityScheme {
    /// An optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// OAuth 2.0 flow configuration — a `oneof` per proto `OAuthFlows`.
///
/// v1.0 treats this as a `oneof` (only one flow per scheme), with
/// `Implicit` and `Password` deprecated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum OAuthFlow {
    /// Authorization Code flow.
    AuthorizationCode(AuthorizationCodeOAuthFlow),
    /// Client Credentials flow.
    ClientCredentials(ClientCredentialsOAuthFlow),
    /// Device Code flow (RFC 8628).
    DeviceCode(DeviceCodeOAuthFlow),
}

/// OAuth 2.0 Authorization Code flow.
///
/// Maps to proto `AuthorizationCodeOAuthFlow`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationCodeOAuthFlow {
    /// The authorization URL.
    pub authorization_url: String,
    /// The token URL.
    pub token_url: String,
    /// The refresh URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    /// Available scopes (scope name → description).
    pub scopes: HashMap<String, String>,
    /// Whether PKCE (RFC 7636) is required.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub pkce_required: bool,
}

/// OAuth 2.0 Client Credentials flow.
///
/// Maps to proto `ClientCredentialsOAuthFlow`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClientCredentialsOAuthFlow {
    /// The token URL.
    pub token_url: String,
    /// The refresh URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    /// Available scopes.
    pub scopes: HashMap<String, String>,
}

/// OAuth 2.0 Device Code flow (RFC 8628).
///
/// Maps to proto `DeviceCodeOAuthFlow`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeOAuthFlow {
    /// The device authorization endpoint URL.
    pub device_authorization_url: String,
    /// The token URL.
    pub token_url: String,
    /// The refresh URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    /// Available scopes.
    pub scopes: HashMap<String, String>,
}

/// A security requirement — a set of security schemes that must be
/// used together (logical AND). Each map entry is scheme name → required scopes.
///
/// Maps to proto `SecurityRequirement`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityRequirement {
    /// Map of scheme name to required scopes.
    pub schemes: HashMap<String, Vec<String>>,
}
