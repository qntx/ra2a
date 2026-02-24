//! Authentication and credential management for A2A clients.
//!
//! Provides credential providers for securing client requests to A2A agents.
//! Aligned with Go's `a2aclient/auth.go` — simple credential resolution model.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::Result;

/// Credential provider trait for obtaining authentication headers.
///
/// Implement this trait to provide custom authentication for client requests.
/// Built-in implementations: [`ApiKeyCredential`], [`BearerTokenCredential`],
/// [`BasicAuthCredential`], [`NoCredential`].
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    /// Returns authentication headers to attach to the request.
    async fn get_headers(&self) -> Result<HashMap<String, String>>;
}

/// Static API key credential provider.
#[derive(Debug, Clone)]
pub struct ApiKeyCredential {
    header_name: String,
    api_key: String,
}

impl ApiKeyCredential {
    /// Creates a new API key credential with a custom header name.
    pub fn new(header_name: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            header_name: header_name.into(),
            api_key: api_key.into(),
        }
    }

    /// Creates an `X-API-Key` credential.
    pub fn x_api_key(api_key: impl Into<String>) -> Self {
        Self::new("X-API-Key", api_key)
    }
}

#[async_trait]
impl CredentialProvider for ApiKeyCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        Ok(HashMap::from([(
            self.header_name.clone(),
            self.api_key.clone(),
        )]))
    }
}

/// Bearer token credential provider.
#[derive(Debug, Clone)]
pub struct BearerTokenCredential {
    token: String,
}

impl BearerTokenCredential {
    /// Creates a new bearer token credential.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

#[async_trait]
impl CredentialProvider for BearerTokenCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        Ok(HashMap::from([(
            "Authorization".to_string(),
            format!("Bearer {}", self.token),
        )]))
    }
}

/// Basic authentication credential provider.
#[derive(Debug, Clone)]
pub struct BasicAuthCredential {
    username: String,
    password: String,
}

impl BasicAuthCredential {
    /// Creates a new basic auth credential.
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }
}

#[async_trait]
impl CredentialProvider for BasicAuthCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", self.username, self.password));
        Ok(HashMap::from([(
            "Authorization".to_string(),
            format!("Basic {encoded}"),
        )]))
    }
}

/// No-op credential provider for unauthenticated requests.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCredential;

#[async_trait]
impl CredentialProvider for NoCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_key_credential() {
        let cred = ApiKeyCredential::x_api_key("test-key");
        let headers = cred.get_headers().await.unwrap();
        assert_eq!(headers.get("X-API-Key"), Some(&"test-key".to_string()));
    }

    #[tokio::test]
    async fn test_bearer_token_credential() {
        let cred = BearerTokenCredential::new("my-token");
        let headers = cred.get_headers().await.unwrap();
        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer my-token".to_string())
        );
    }

    #[tokio::test]
    async fn test_basic_auth_credential() {
        let cred = BasicAuthCredential::new("user", "pass");
        let headers = cred.get_headers().await.unwrap();
        let auth = headers.get("Authorization").unwrap();
        assert!(auth.starts_with("Basic "));
    }

    #[tokio::test]
    async fn test_no_credential() {
        let cred = NoCredential;
        let headers = cred.get_headers().await.unwrap();
        assert!(headers.is_empty());
    }
}
