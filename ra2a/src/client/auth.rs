//! Authentication and credential management for A2A clients.
//!
//! Provides credential providers, credential services, and authentication helpers
//! for securing client requests to A2A agents. This module mirrors the Python SDK's
//! `client/auth/` module structure.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::error::{A2AError, Result};

/// URL-encode a string for form submission.
fn urlencoding_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            ' ' => result.push('+'),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    result
}

/// Credential provider trait for obtaining authentication credentials.
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    /// Gets the authentication headers for a request.
    async fn get_headers(&self) -> Result<HashMap<String, String>>;

    /// Refreshes the credentials if needed.
    async fn refresh(&self) -> Result<()> {
        Ok(())
    }
}

/// Static API key credential provider.
#[derive(Debug, Clone)]
pub struct ApiKeyCredential {
    header_name: String,
    api_key: String,
}

impl ApiKeyCredential {
    /// Creates a new API key credential.
    pub fn new(header_name: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            header_name: header_name.into(),
            api_key: api_key.into(),
        }
    }

    /// Creates an X-API-Key credential.
    pub fn x_api_key(api_key: impl Into<String>) -> Self {
        Self::new("X-API-Key", api_key)
    }
}

#[async_trait]
impl CredentialProvider for ApiKeyCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        let mut headers = HashMap::new();
        headers.insert(self.header_name.clone(), self.api_key.clone());
        Ok(headers)
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
        let mut headers = HashMap::new();
        headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", self.token),
        );
        Ok(headers)
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

    /// Encodes the credentials to base64.
    fn encode(&self) -> String {
        use base64::Engine;
        let credentials = format!("{}:{}", self.username, self.password);
        base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes())
    }
}

#[async_trait]
impl CredentialProvider for BasicAuthCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        let mut headers = HashMap::new();
        headers.insert(
            "Authorization".to_string(),
            format!("Basic {}", self.encode()),
        );
        Ok(headers)
    }
}

/// No-op credential provider for unauthenticated requests.
#[derive(Debug, Clone, Default)]
pub struct NoCredential;

#[async_trait]
impl CredentialProvider for NoCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }
}

/// Composite credential provider that combines multiple providers.
pub struct CompositeCredential {
    providers: Vec<Box<dyn CredentialProvider>>,
}

impl CompositeCredential {
    /// Creates a new composite credential.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Adds a credential provider.
    pub fn add<C: CredentialProvider + 'static>(mut self, provider: C) -> Self {
        self.providers.push(Box::new(provider));
        self
    }
}

impl Default for CompositeCredential {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialProvider for CompositeCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        let mut all_headers = HashMap::new();
        for provider in &self.providers {
            let headers = provider.get_headers().await?;
            all_headers.extend(headers);
        }
        Ok(all_headers)
    }

    async fn refresh(&self) -> Result<()> {
        for provider in &self.providers {
            provider.refresh().await?;
        }
        Ok(())
    }
}

/// A service for managing and providing credentials dynamically.
///
/// This trait mirrors Python's `CredentialService` from `client/auth/credentials.py`.
/// It supports obtaining credentials for specific contexts and refreshing them.
#[async_trait]
pub trait CredentialService: Send + Sync {
    /// Gets credentials for the specified context.
    async fn get_credentials(&self, context: &CredentialContext) -> Result<Credentials>;

    /// Refreshes credentials for the specified context.
    async fn refresh_credentials(&self, context: &CredentialContext) -> Result<Credentials>;

    /// Checks if credentials need to be refreshed.
    async fn needs_refresh(&self, context: &CredentialContext) -> bool;
}

/// Context information for credential retrieval.
#[derive(Debug, Clone, Default)]
pub struct CredentialContext {
    /// The agent URL for which credentials are needed.
    pub agent_url: Option<String>,
    /// Additional scopes or permissions required.
    pub scopes: Vec<String>,
    /// Custom context data.
    pub metadata: HashMap<String, String>,
}

impl CredentialContext {
    /// Creates a new credential context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a context for a specific agent URL.
    pub fn for_agent(url: impl Into<String>) -> Self {
        Self {
            agent_url: Some(url.into()),
            ..Default::default()
        }
    }

    /// Adds a scope to the context.
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scopes.push(scope.into());
        self
    }

    /// Adds metadata to the context.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Credentials returned by a CredentialService.
#[derive(Debug, Clone)]
pub struct Credentials {
    /// The access token.
    pub access_token: String,
    /// Token type (e.g., "Bearer").
    pub token_type: String,
    /// When the token expires.
    pub expires_at: Option<Instant>,
    /// Refresh token if available.
    pub refresh_token: Option<String>,
}

impl Credentials {
    /// Creates new credentials with a bearer token.
    pub fn bearer(token: impl Into<String>) -> Self {
        Self {
            access_token: token.into(),
            token_type: "Bearer".to_string(),
            expires_at: None,
            refresh_token: None,
        }
    }

    /// Creates new credentials with expiration.
    pub fn with_expiry(mut self, expires_in: Duration) -> Self {
        self.expires_at = Some(Instant::now() + expires_in);
        self
    }

    /// Creates new credentials with a refresh token.
    pub fn with_refresh_token(mut self, refresh_token: impl Into<String>) -> Self {
        self.refresh_token = Some(refresh_token.into());
        self
    }

    /// Checks if the credentials have expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| Instant::now() >= exp)
    }

    /// Returns the authorization header value.
    pub fn authorization_header(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }
}

/// A credential provider that automatically refreshes tokens using a service.
pub struct RefreshableCredential {
    service: Arc<dyn CredentialService>,
    context: CredentialContext,
    cached: RwLock<Option<Credentials>>,
    /// Buffer time before expiry to trigger refresh.
    refresh_buffer: Duration,
}

impl RefreshableCredential {
    /// Creates a new refreshable credential.
    pub fn new(service: Arc<dyn CredentialService>, context: CredentialContext) -> Self {
        Self {
            service,
            context,
            cached: RwLock::new(None),
            refresh_buffer: Duration::from_secs(60),
        }
    }

    /// Sets the refresh buffer time.
    pub fn with_refresh_buffer(mut self, buffer: Duration) -> Self {
        self.refresh_buffer = buffer;
        self
    }

    /// Checks if cached credentials need refresh.
    async fn needs_refresh(&self) -> bool {
        let cached = self.cached.read().await;
        match cached.as_ref() {
            None => true,
            Some(creds) => {
                if let Some(expires_at) = creds.expires_at {
                    Instant::now() + self.refresh_buffer >= expires_at
                } else {
                    false
                }
            }
        }
    }
}

impl std::fmt::Debug for RefreshableCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefreshableCredential")
            .field("context", &self.context)
            .field("refresh_buffer", &self.refresh_buffer)
            .finish()
    }
}

#[async_trait]
impl CredentialProvider for RefreshableCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        // Check if we need to refresh
        if self.needs_refresh().await {
            self.refresh().await?;
        }

        let cached = self.cached.read().await;
        let creds = cached
            .as_ref()
            .ok_or_else(|| A2AError::InvalidParams("No credentials available".to_string()))?;

        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), creds.authorization_header());
        Ok(headers)
    }

    async fn refresh(&self) -> Result<()> {
        let new_creds = self.service.refresh_credentials(&self.context).await?;
        let mut cached = self.cached.write().await;
        *cached = Some(new_creds);
        Ok(())
    }
}

/// An in-memory store for managing credentials per context.
///
/// Mirrors Python's credential store pattern for multi-tenant scenarios.
#[derive(Debug, Default)]
pub struct InMemoryCredentialStore {
    credentials: RwLock<HashMap<String, Credentials>>,
}

impl InMemoryCredentialStore {
    /// Creates a new in-memory credential store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores credentials for a context key.
    pub async fn store(&self, key: impl Into<String>, credentials: Credentials) {
        let mut store = self.credentials.write().await;
        store.insert(key.into(), credentials);
    }

    /// Retrieves credentials for a context key.
    pub async fn get(&self, key: &str) -> Option<Credentials> {
        let store = self.credentials.read().await;
        store.get(key).cloned()
    }

    /// Removes credentials for a context key.
    pub async fn remove(&self, key: &str) -> Option<Credentials> {
        let mut store = self.credentials.write().await;
        store.remove(key)
    }

    /// Clears all stored credentials.
    pub async fn clear(&self) {
        let mut store = self.credentials.write().await;
        store.clear();
    }

    /// Returns the number of stored credentials.
    pub async fn len(&self) -> usize {
        let store = self.credentials.read().await;
        store.len()
    }

    /// Checks if the store is empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

/// A credential service backed by an in-memory store.
pub struct InMemoryCredentialService {
    store: Arc<InMemoryCredentialStore>,
}

impl InMemoryCredentialService {
    /// Creates a new in-memory credential service.
    pub fn new() -> Self {
        Self {
            store: Arc::new(InMemoryCredentialStore::new()),
        }
    }

    /// Creates a service with an existing store.
    pub fn with_store(store: Arc<InMemoryCredentialStore>) -> Self {
        Self { store }
    }

    /// Gets the underlying store.
    pub fn store(&self) -> &Arc<InMemoryCredentialStore> {
        &self.store
    }
}

impl Default for InMemoryCredentialService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialService for InMemoryCredentialService {
    async fn get_credentials(&self, context: &CredentialContext) -> Result<Credentials> {
        let key = context.agent_url.as_deref().unwrap_or("default");
        self.store
            .get(key)
            .await
            .ok_or_else(|| A2AError::InvalidParams(format!("No credentials for context: {}", key)))
    }

    async fn refresh_credentials(&self, context: &CredentialContext) -> Result<Credentials> {
        // In-memory service doesn't actually refresh, just returns stored credentials
        self.get_credentials(context).await
    }

    async fn needs_refresh(&self, context: &CredentialContext) -> bool {
        let key = context.agent_url.as_deref().unwrap_or("default");
        match self.store.get(key).await {
            Some(creds) => creds.is_expired(),
            None => true,
        }
    }
}
/// OAuth2 client credentials flow provider.
#[derive(Debug, Clone)]
pub struct OAuth2ClientCredential {
    client_id: String,
    client_secret: String,
    token_url: String,
    scopes: Vec<String>,
    cached_token: Arc<RwLock<Option<Credentials>>>,
}

impl OAuth2ClientCredential {
    /// Creates a new OAuth2 client credentials provider.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        token_url: impl Into<String>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            token_url: token_url.into(),
            scopes: Vec::new(),
            cached_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Adds scopes to the OAuth2 request.
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Fetches a new token from the token endpoint.
    async fn fetch_token(&self) -> Result<Credentials> {
        let client = reqwest::Client::new();

        // Build URL-encoded form body
        let mut body_parts = vec![
            format!("grant_type=client_credentials"),
            format!("client_id={}", urlencoding_encode(&self.client_id)),
            format!("client_secret={}", urlencoding_encode(&self.client_secret)),
        ];

        if !self.scopes.is_empty() {
            let scope_string = self.scopes.join(" ");
            body_parts.push(format!("scope={}", urlencoding_encode(&scope_string)));
        }

        let body = body_parts.join("&");

        let response = client
            .post(&self.token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| A2AError::Other(format!("Token request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Err(A2AError::InvalidParams(format!(
                "Token request failed with status {}: {}",
                status, error_body
            )));
        }

        let token_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| A2AError::Other(format!("Failed to parse token response: {}", e)))?;

        let access_token = token_response["access_token"]
            .as_str()
            .ok_or_else(|| A2AError::InvalidParams("Missing access_token in response".into()))?
            .to_string();

        let token_type = token_response["token_type"]
            .as_str()
            .unwrap_or("Bearer")
            .to_string();

        let expires_in = token_response["expires_in"].as_u64();

        let mut creds = Credentials {
            access_token,
            token_type,
            expires_at: None,
            refresh_token: None,
        };

        if let Some(secs) = expires_in {
            creds = creds.with_expiry(Duration::from_secs(secs));
        }

        Ok(creds)
    }
}

#[async_trait]
impl CredentialProvider for OAuth2ClientCredential {
    async fn get_headers(&self) -> Result<HashMap<String, String>> {
        // Check cached token
        {
            let cached = self.cached_token.read().await;
            if let Some(ref creds) = *cached {
                if !creds.is_expired() {
                    let mut headers = HashMap::new();
                    headers.insert("Authorization".to_string(), creds.authorization_header());
                    return Ok(headers);
                }
            }
        }

        // Fetch new token
        self.refresh().await?;

        let cached = self.cached_token.read().await;
        let creds = cached
            .as_ref()
            .ok_or_else(|| A2AError::InvalidParams("Failed to obtain token".to_string()))?;

        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), creds.authorization_header());
        Ok(headers)
    }

    async fn refresh(&self) -> Result<()> {
        let new_token = self.fetch_token().await?;
        let mut cached = self.cached_token.write().await;
        *cached = Some(new_token);
        Ok(())
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
    async fn test_composite_credential() {
        let cred = CompositeCredential::new()
            .add(ApiKeyCredential::x_api_key("key1"))
            .add(BearerTokenCredential::new("token1"));

        let headers = cred.get_headers().await.unwrap();
        assert!(headers.contains_key("X-API-Key"));
        assert!(headers.contains_key("Authorization"));
    }

    #[tokio::test]
    async fn test_credential_context() {
        let ctx = CredentialContext::for_agent("https://agent.example.com")
            .with_scope("read")
            .with_scope("write")
            .with_metadata("tenant", "test");

        assert_eq!(ctx.agent_url, Some("https://agent.example.com".to_string()));
        assert_eq!(ctx.scopes.len(), 2);
        assert_eq!(ctx.metadata.get("tenant"), Some(&"test".to_string()));
    }

    #[tokio::test]
    async fn test_credentials() {
        let creds = Credentials::bearer("test-token").with_expiry(Duration::from_secs(3600));

        assert_eq!(creds.token_type, "Bearer");
        assert_eq!(creds.access_token, "test-token");
        assert!(!creds.is_expired());
        assert_eq!(creds.authorization_header(), "Bearer test-token");
    }

    #[tokio::test]
    async fn test_in_memory_credential_store() {
        let store = InMemoryCredentialStore::new();

        let creds = Credentials::bearer("stored-token");
        store.store("test-key", creds.clone()).await;

        let retrieved = store.get("test-key").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().access_token, "stored-token");

        assert_eq!(store.len().await, 1);
        store.remove("test-key").await;
        assert!(store.is_empty().await);
    }

    #[test]
    fn test_urlencoding() {
        assert_eq!(urlencoding_encode("hello world"), "hello+world");
        assert_eq!(urlencoding_encode("test@example.com"), "test%40example.com");
        assert_eq!(urlencoding_encode("simple"), "simple");
    }
}
