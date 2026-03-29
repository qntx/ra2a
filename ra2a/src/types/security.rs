//! Security scheme types for the A2A protocol.
//!
//! Maps to proto `SecurityScheme`, `SecurityRequirement`, and all OAuth flow types.

use std::collections::HashMap;

use serde::de::{self, Deserializer};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

/// A security scheme that can be used to secure an agent's endpoints.
///
/// Maps to proto `SecurityScheme` (oneof scheme). Serialized using JSON
/// member name as discriminator: `{"apiKey": {...}}`, `{"http": {...}}`, etc.
#[derive(Debug, Clone, PartialEq)]
pub enum SecurityScheme {
    /// API key-based authentication.
    ApiKey(ApiKeySecurityScheme),
    /// HTTP authentication (Basic, Bearer, etc.).
    Http(HttpAuthSecurityScheme),
    /// OAuth 2.0 authentication.
    OAuth2(Box<OAuth2SecurityScheme>),
    /// OpenID Connect authentication.
    OpenIdConnect(OpenIdConnectSecurityScheme),
    /// Mutual TLS authentication.
    MutualTLS(MutualTlsSecurityScheme),
}

impl Serialize for SecurityScheme {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::ApiKey(v) => map.serialize_entry("apiKey", v)?,
            Self::Http(v) => map.serialize_entry("http", v)?,
            Self::OAuth2(v) => map.serialize_entry("oauth2", v.as_ref())?,
            Self::OpenIdConnect(v) => map.serialize_entry("openIdConnect", v)?,
            Self::MutualTLS(v) => map.serialize_entry("mutualTLS", v)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for SecurityScheme {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: HashMap<String, serde_json::Value> = HashMap::deserialize(deserializer)?;
        if let Some(v) = raw.get("apiKey") {
            Ok(Self::ApiKey(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            ))
        } else if let Some(v) = raw.get("http") {
            Ok(Self::Http(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            ))
        } else if let Some(v) = raw.get("oauth2") {
            Ok(Self::OAuth2(Box::new(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            )))
        } else if let Some(v) = raw.get("openIdConnect") {
            Ok(Self::OpenIdConnect(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            ))
        } else if let Some(v) = raw.get("mutualTLS") {
            Ok(Self::MutualTLS(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            ))
        } else {
            Err(de::Error::custom(
                "SecurityScheme must contain one of: apiKey, http, oauth2, openIdConnect, mutualTLS",
            ))
        }
    }
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
///
/// Serialized as a map with a single entry: `{"authorizationCode": {...}}`, etc.
#[derive(Debug, Clone, PartialEq)]
pub enum OAuthFlow {
    /// Authorization Code flow.
    AuthorizationCode(AuthorizationCodeOAuthFlow),
    /// Client Credentials flow.
    ClientCredentials(ClientCredentialsOAuthFlow),
    /// Device Code flow (RFC 8628).
    DeviceCode(DeviceCodeOAuthFlow),
    /// Implicit flow (deprecated — use Authorization Code + PKCE).
    #[deprecated = "Use Authorization Code + PKCE instead"]
    Implicit(ImplicitOAuthFlow),
    /// Resource Owner Password flow (deprecated — use Authorization Code + PKCE or Device Code).
    #[deprecated = "Use Authorization Code + PKCE or Device Code"]
    Password(PasswordOAuthFlow),
}

impl Serialize for OAuthFlow {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::AuthorizationCode(v) => map.serialize_entry("authorizationCode", v)?,
            Self::ClientCredentials(v) => map.serialize_entry("clientCredentials", v)?,
            Self::DeviceCode(v) => map.serialize_entry("deviceCode", v)?,
            #[allow(deprecated)]
            Self::Implicit(v) => map.serialize_entry("implicit", v)?,
            #[allow(deprecated)]
            Self::Password(v) => map.serialize_entry("password", v)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for OAuthFlow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: HashMap<String, serde_json::Value> = HashMap::deserialize(deserializer)?;
        if let Some(v) = raw.get("authorizationCode") {
            Ok(Self::AuthorizationCode(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            ))
        } else if let Some(v) = raw.get("clientCredentials") {
            Ok(Self::ClientCredentials(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            ))
        } else if let Some(v) = raw.get("deviceCode") {
            Ok(Self::DeviceCode(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            ))
        } else if let Some(v) = raw.get("implicit") {
            #[allow(deprecated)]
            Ok(Self::Implicit(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            ))
        } else if let Some(v) = raw.get("password") {
            #[allow(deprecated)]
            Ok(Self::Password(
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?,
            ))
        } else {
            Err(de::Error::custom(
                "OAuthFlow must contain one of: authorizationCode, clientCredentials, deviceCode, implicit, password",
            ))
        }
    }
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

/// Deprecated: OAuth 2.0 Implicit flow.
///
/// Maps to proto `ImplicitOAuthFlow`. Use Authorization Code + PKCE instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImplicitOAuthFlow {
    /// The authorization URL.
    pub authorization_url: String,
    /// The refresh URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    /// Available scopes.
    #[serde(default)]
    pub scopes: HashMap<String, String>,
}

/// Deprecated: OAuth 2.0 Resource Owner Password flow.
///
/// Maps to proto `PasswordOAuthFlow`. Use Authorization Code + PKCE or Device Code instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PasswordOAuthFlow {
    /// The token URL.
    pub token_url: String,
    /// The refresh URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    /// Available scopes.
    #[serde(default)]
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
