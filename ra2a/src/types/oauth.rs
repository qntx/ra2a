//! OAuth 2.0 flow types for the A2A protocol.
//!
//! Defines configuration for OAuth 2.0 flows used in security schemes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Defines the configuration for the supported OAuth 2.0 flows.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OAuthFlows {
    /// Configuration for the OAuth Authorization Code flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_code: Option<AuthorizationCodeOAuthFlow>,
    /// Configuration for the OAuth Client Credentials flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_credentials: Option<ClientCredentialsOAuthFlow>,
    /// Configuration for the OAuth Implicit flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit: Option<ImplicitOAuthFlow>,
    /// Configuration for the OAuth Resource Owner Password flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<PasswordOAuthFlow>,
}

impl OAuthFlows {
    /// Creates flows with authorization code configuration.
    pub fn authorization_code(flow: AuthorizationCodeOAuthFlow) -> Self {
        Self {
            authorization_code: Some(flow),
            ..Default::default()
        }
    }

    /// Creates flows with client credentials configuration.
    pub fn client_credentials(flow: ClientCredentialsOAuthFlow) -> Self {
        Self {
            client_credentials: Some(flow),
            ..Default::default()
        }
    }
}

/// Defines configuration for the OAuth 2.0 Authorization Code flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthorizationCodeOAuthFlow {
    /// The authorization URL to be used for this flow.
    pub authorization_url: String,
    /// The token URL to be used for this flow.
    pub token_url: String,
    /// The available scopes for the OAuth2 security scheme.
    pub scopes: HashMap<String, String>,
    /// The URL to be used for obtaining refresh tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
}

impl AuthorizationCodeOAuthFlow {
    /// Creates a new authorization code flow configuration.
    pub fn new(
        authorization_url: impl Into<String>,
        token_url: impl Into<String>,
        scopes: HashMap<String, String>,
    ) -> Self {
        Self {
            authorization_url: authorization_url.into(),
            token_url: token_url.into(),
            scopes,
            refresh_url: None,
        }
    }
}

/// Defines configuration for the OAuth 2.0 Client Credentials flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientCredentialsOAuthFlow {
    /// The token URL to be used for this flow.
    pub token_url: String,
    /// The available scopes for the OAuth2 security scheme.
    pub scopes: HashMap<String, String>,
    /// The URL to be used for obtaining refresh tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
}

impl ClientCredentialsOAuthFlow {
    /// Creates a new client credentials flow configuration.
    pub fn new(token_url: impl Into<String>, scopes: HashMap<String, String>) -> Self {
        Self {
            token_url: token_url.into(),
            scopes,
            refresh_url: None,
        }
    }
}

/// Defines configuration for the OAuth 2.0 Implicit flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImplicitOAuthFlow {
    /// The authorization URL to be used for this flow.
    pub authorization_url: String,
    /// The available scopes for the OAuth2 security scheme.
    pub scopes: HashMap<String, String>,
    /// The URL to be used for obtaining refresh tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
}

impl ImplicitOAuthFlow {
    /// Creates a new implicit flow configuration.
    pub fn new(authorization_url: impl Into<String>, scopes: HashMap<String, String>) -> Self {
        Self {
            authorization_url: authorization_url.into(),
            scopes,
            refresh_url: None,
        }
    }
}

/// Defines configuration for the OAuth 2.0 Resource Owner Password flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PasswordOAuthFlow {
    /// The token URL to be used for this flow.
    pub token_url: String,
    /// The available scopes for the OAuth2 security scheme.
    pub scopes: HashMap<String, String>,
    /// The URL to be used for obtaining refresh tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
}

impl PasswordOAuthFlow {
    /// Creates a new password flow configuration.
    pub fn new(token_url: impl Into<String>, scopes: HashMap<String, String>) -> Self {
        Self {
            token_url: token_url.into(),
            scopes,
            refresh_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_flows_creation() {
        let mut scopes = HashMap::new();
        scopes.insert("read".to_string(), "Read access".to_string());

        let flow = ClientCredentialsOAuthFlow::new("https://auth.example.com/token", scopes);
        let flows = OAuthFlows::client_credentials(flow);

        assert!(flows.client_credentials.is_some());
        assert!(flows.authorization_code.is_none());
    }

    #[test]
    fn test_oauth_flow_serialization() {
        let mut scopes = HashMap::new();
        scopes.insert("read".to_string(), "Read access".to_string());

        let flow = AuthorizationCodeOAuthFlow::new(
            "https://auth.example.com/authorize",
            "https://auth.example.com/token",
            scopes,
        );
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains("authorization_url"));
        assert!(json.contains("token_url"));
    }
}
