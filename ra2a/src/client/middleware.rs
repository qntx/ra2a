//! Client-side middleware for intercepting and modifying requests.
//!
//! Aligned with Go's `a2aclient/middleware.go`. Provides a Before/After
//! interceptor model where `before` runs in registration order and
//! `after` runs in reverse order.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::types::AgentCard;

/// Transport-agnostic request metadata.
///
/// Aligned with Go's `CallMeta`. In JSON-RPC it becomes HTTP headers;
/// in gRPC it becomes metadata entries.
#[derive(Debug, Clone, Default)]
pub struct CallMeta {
    inner: HashMap<String, Vec<String>>,
}

impl CallMeta {
    /// Creates a new empty `CallMeta`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Case-insensitive lookup. Returns `None` if not present.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&[String]> {
        self.inner.get(&key.to_lowercase()).map(|v| v.as_slice())
    }

    /// Appends values, skipping duplicates. Key matching is case-insensitive.
    pub fn append(&mut self, key: &str, values: &[String]) {
        let entry = self.inner.entry(key.to_lowercase()).or_default();
        for v in values {
            if !entry.contains(v) {
                entry.push(v.clone());
            }
        }
    }

    /// Sets a single value, replacing any existing values.
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.inner.insert(key.to_lowercase(), vec![value.into()]);
    }

    /// Returns an iterator over all (key, values) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.inner.iter()
    }
}

/// A transport-agnostic request about to be sent to an A2A server.
///
/// Aligned with Go's `a2aclient.Request`.
pub struct ClientRequest {
    /// The A2A method name (e.g. `"message/send"`).
    pub method: String,
    /// Request metadata (becomes HTTP headers or gRPC metadata).
    pub meta: CallMeta,
    /// The agent card of the server, if known.
    pub card: Option<AgentCard>,
    /// The request payload (one of the A2A core types).
    pub payload: Option<serde_json::Value>,
}

/// A transport-agnostic response received from an A2A server.
///
/// Aligned with Go's `a2aclient.Response`.
pub struct ClientResponse {
    /// The A2A method name.
    pub method: String,
    /// Response metadata.
    pub meta: CallMeta,
    /// The agent card of the server, if known.
    pub card: Option<AgentCard>,
    /// The response payload.
    pub payload: Option<serde_json::Value>,
    /// The error, if the call failed.
    pub error: Option<crate::error::A2AError>,
}

/// Client-side call interceptor with Before/After hooks.
///
/// Aligned with Go's `a2aclient.CallInterceptor`:
/// - `before` runs in registration order, can modify or reject the request.
/// - `after` runs in **reverse** order, can observe or modify the response.
#[async_trait]
pub trait ClientCallInterceptor: Send + Sync {
    /// Called before the request is sent. Can modify the request or reject it.
    async fn before(&self, req: &mut ClientRequest) -> crate::error::Result<()> {
        let _ = req;
        Ok(())
    }

    /// Called after the response is received. Can observe or modify the response.
    async fn after(&self, resp: &mut ClientResponse) -> crate::error::Result<()> {
        let _ = resp;
        Ok(())
    }
}

/// A no-op interceptor. Embed or use directly when only one hook is needed.
///
/// Aligned with Go's `PassthroughInterceptor`.
pub struct PassthroughClientInterceptor;

impl ClientCallInterceptor for PassthroughClientInterceptor {}

/// A logging interceptor that logs outgoing requests and incoming responses.
#[derive(Debug, Default)]
pub struct LoggingInterceptor {
    /// Log level for the interceptor.
    pub log_level: LogLevel,
}

/// Log levels for the logging interceptor.
#[derive(Debug, Clone, Copy, Default)]
pub enum LogLevel {
    /// Debug level logging.
    Debug,
    /// Info level logging.
    #[default]
    Info,
    /// Warn level logging.
    Warn,
}

#[async_trait]
impl ClientCallInterceptor for LoggingInterceptor {
    async fn before(&self, req: &mut ClientRequest) -> crate::error::Result<()> {
        match self.log_level {
            LogLevel::Debug => {
                tracing::debug!(method = %req.method, payload = ?req.payload, "Outgoing request");
            }
            LogLevel::Info => {
                tracing::info!(method = %req.method, "Outgoing request");
            }
            LogLevel::Warn => {
                tracing::warn!(method = %req.method, "Outgoing request");
            }
        }
        Ok(())
    }

    async fn after(&self, resp: &mut ClientResponse) -> crate::error::Result<()> {
        if let Some(ref err) = resp.error {
            tracing::warn!(method = %resp.method, error = %err, "Response error");
        } else {
            tracing::debug!(method = %resp.method, "Response received");
        }
        Ok(())
    }
}

/// An authentication interceptor that injects an authorization header.
#[derive(Debug, Clone)]
pub struct AuthInterceptor {
    auth_header: String,
}

impl AuthInterceptor {
    /// Creates a new auth interceptor with a Bearer token.
    pub fn bearer(token: impl Into<String>) -> Self {
        Self {
            auth_header: format!("Bearer {}", token.into()),
        }
    }

    /// Creates a new auth interceptor with a Basic auth credential.
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        use base64::Engine;
        let credentials = format!("{}:{}", username.into(), password.into());
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        Self {
            auth_header: format!("Basic {encoded}"),
        }
    }

    /// Creates a new auth interceptor with a custom header value.
    pub fn custom(auth_header: impl Into<String>) -> Self {
        Self {
            auth_header: auth_header.into(),
        }
    }
}

#[async_trait]
impl ClientCallInterceptor for AuthInterceptor {
    async fn before(&self, req: &mut ClientRequest) -> crate::error::Result<()> {
        req.meta.set("authorization", &self.auth_header);
        Ok(())
    }
}

/// Runs `before` on all interceptors (in order), then `after` (in reverse).
///
/// This is the core interception logic aligned with Go's
/// `client.interceptBefore` / `client.interceptAfter`.
pub async fn run_interceptors_before(
    interceptors: &[Box<dyn ClientCallInterceptor>],
    req: &mut ClientRequest,
) -> crate::error::Result<()> {
    for interceptor in interceptors {
        interceptor.before(req).await?;
    }
    Ok(())
}

/// Runs `after` on all interceptors in **reverse** order.
pub async fn run_interceptors_after(
    interceptors: &[Box<dyn ClientCallInterceptor>],
    resp: &mut ClientResponse,
) -> crate::error::Result<()> {
    for interceptor in interceptors.iter().rev() {
        interceptor.after(resp).await?;
    }
    Ok(())
}

/// Creates a new static call-meta injector interceptor.
///
/// Aligned with Go's `NewStaticCallMetaInjector`.
pub struct StaticCallMetaInjector {
    meta: CallMeta,
}

impl StaticCallMetaInjector {
    /// Creates a new injector that appends the given meta to every request.
    pub fn new(meta: CallMeta) -> Self {
        Self { meta }
    }
}

#[async_trait]
impl ClientCallInterceptor for StaticCallMetaInjector {
    async fn before(&self, req: &mut ClientRequest) -> crate::error::Result<()> {
        for (key, values) in self.meta.iter() {
            req.meta.append(key, values);
        }
        Ok(())
    }
}

/// Client interceptor that requests activation of extensions supported by the server.
///
/// Aligned with Go's `a2aext.NewActivator`. When the server's [`AgentCard`]
/// advertises support for one of the configured extension URIs, the activator
/// adds those URIs to the `X-A2A-Extensions` request header so the server
/// can activate them for the call.
pub struct ExtensionActivator {
    extension_uris: Vec<String>,
}

impl ExtensionActivator {
    /// Creates a new activator for the given extension URIs.
    pub fn new(uris: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            extension_uris: uris.into_iter().map(Into::into).collect(),
        }
    }

    /// Checks if the server supports the given extension URI.
    fn is_supported(card: &AgentCard, uri: &str) -> bool {
        card.capabilities
            .extensions
            .iter()
            .any(|ext| ext.uri == uri)
    }
}

#[async_trait]
impl ClientCallInterceptor for ExtensionActivator {
    async fn before(&self, req: &mut ClientRequest) -> crate::error::Result<()> {
        let card = match req.card.as_ref() {
            Some(c) => c,
            // No card available — assume all extensions are supported (same as Go)
            None => {
                if !self.extension_uris.is_empty() {
                    req.meta.append(
                        crate::EXTENSIONS_META_KEY,
                        &self.extension_uris,
                    );
                }
                return Ok(());
            }
        };

        if card.capabilities.extensions.is_empty() {
            return Ok(());
        }

        let supported: Vec<String> = self
            .extension_uris
            .iter()
            .filter(|uri| Self::is_supported(card, uri))
            .cloned()
            .collect();

        if !supported.is_empty() {
            req.meta.append(crate::EXTENSIONS_META_KEY, &supported);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_meta() {
        let mut meta = CallMeta::new();
        meta.set("Content-Type", "application/json");
        assert_eq!(meta.get("content-type"), Some(["application/json".to_string()].as_slice()));

        meta.append("x-custom", &["a".into(), "b".into()]);
        meta.append("X-Custom", &["b".into(), "c".into()]);
        assert_eq!(meta.get("x-custom").unwrap().len(), 3);
    }

    #[test]
    fn test_auth_interceptor_bearer() {
        let interceptor = AuthInterceptor::bearer("test-token");
        assert!(interceptor.auth_header.starts_with("Bearer "));
    }

    #[test]
    fn test_auth_interceptor_basic() {
        let interceptor = AuthInterceptor::basic("user", "pass");
        assert!(interceptor.auth_header.starts_with("Basic "));
    }

    #[tokio::test]
    async fn test_interceptor_before_after_order() {
        use std::sync::{Arc, Mutex};

        let log = Arc::new(Mutex::new(Vec::<String>::new()));

        struct OrderInterceptor {
            name: String,
            log: Arc<Mutex<Vec<String>>>,
        }

        #[async_trait]
        impl ClientCallInterceptor for OrderInterceptor {
            async fn before(&self, _req: &mut ClientRequest) -> crate::error::Result<()> {
                self.log.lock().unwrap().push(format!("before:{}", self.name));
                Ok(())
            }
            async fn after(&self, _resp: &mut ClientResponse) -> crate::error::Result<()> {
                self.log.lock().unwrap().push(format!("after:{}", self.name));
                Ok(())
            }
        }

        let interceptors: Vec<Box<dyn ClientCallInterceptor>> = vec![
            Box::new(OrderInterceptor { name: "A".into(), log: Arc::clone(&log) }),
            Box::new(OrderInterceptor { name: "B".into(), log: Arc::clone(&log) }),
        ];

        let mut req = ClientRequest {
            method: "test".into(),
            meta: CallMeta::new(),
            card: None,
            payload: None,
        };
        run_interceptors_before(&interceptors, &mut req).await.unwrap();

        let mut resp = ClientResponse {
            method: "test".into(),
            meta: CallMeta::new(),
            card: None,
            payload: None,
            error: None,
        };
        run_interceptors_after(&interceptors, &mut resp).await.unwrap();

        let entries = log.lock().unwrap();
        assert_eq!(&*entries, &["before:A", "before:B", "after:B", "after:A"]);
    }

    #[tokio::test]
    async fn test_extension_activator_filters_by_card() {
        use crate::types::{AgentCapabilities, AgentExtension};

        let activator = ExtensionActivator::new(["urn:ext:a", "urn:ext:b", "urn:ext:c"]);

        let mut card = AgentCard::builder("test", "http://localhost").build();
        card.capabilities = AgentCapabilities {
            extensions: vec![
                AgentExtension { uri: "urn:ext:a".into(), description: None, required: false, params: None },
                AgentExtension { uri: "urn:ext:c".into(), description: None, required: false, params: None },
            ],
            ..Default::default()
        };

        let mut req = ClientRequest {
            method: "message/send".into(),
            meta: CallMeta::new(),
            card: Some(card),
            payload: None,
        };

        activator.before(&mut req).await.unwrap();

        let exts = req.meta.get(crate::EXTENSIONS_META_KEY).unwrap();
        assert_eq!(exts, &["urn:ext:a".to_string(), "urn:ext:c".to_string()]);
        // urn:ext:b is NOT supported by card, so it should not be present
        assert!(!exts.contains(&"urn:ext:b".to_string()));
    }

    #[tokio::test]
    async fn test_extension_activator_no_card_sends_all() {
        let activator = ExtensionActivator::new(["urn:ext:x"]);

        let mut req = ClientRequest {
            method: "message/send".into(),
            meta: CallMeta::new(),
            card: None,
            payload: None,
        };

        activator.before(&mut req).await.unwrap();

        let exts = req.meta.get(crate::EXTENSIONS_META_KEY).unwrap();
        assert_eq!(exts, &["urn:ext:x".to_string()]);
    }
}
