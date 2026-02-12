//! Server-side middleware: call context, interceptors, and auth primitives.
//!
//! Aligned with Go's `middleware.go`, `reqmeta.go`, and `auth.go` in `a2asrv`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

// ---------------------------------------------------------------------------
// RequestMeta — transport-agnostic request metadata (Go: reqmeta.go)
// ---------------------------------------------------------------------------

/// Holds metadata associated with a request (e.g. HTTP headers, gRPC metadata).
///
/// Custom transport implementations can attach this to a [`CallContext`] via
/// [`CallContext::new`]. Keys are stored in lower-case for case-insensitive
/// lookups, matching Go's `RequestMeta`.
#[derive(Debug, Clone, Default)]
pub struct RequestMeta {
    kv: HashMap<String, Vec<String>>,
}

impl RequestMeta {
    /// Creates a new `RequestMeta` from a map of header-like key-value pairs.
    ///
    /// Keys are normalized to lower-case.
    #[must_use] 
    pub fn new(src: HashMap<String, Vec<String>>) -> Self {
        let kv = src
            .into_iter()
            .map(|(k, v)| (k.to_lowercase(), v))
            .collect();
        Self { kv }
    }

    /// Creates an empty `RequestMeta`.
    #[must_use] 
    pub fn empty() -> Self {
        Self::default()
    }

    /// Performs a case-insensitive lookup.
    #[must_use] 
    pub fn get(&self, key: &str) -> Option<&[String]> {
        self.kv.get(&key.to_lowercase()).map(std::vec::Vec::as_slice)
    }

    /// Returns an iterator over all key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.kv.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Merges additional metadata, creating a new `RequestMeta`.
    #[must_use] 
    pub fn with(&self, additional: HashMap<String, Vec<String>>) -> Self {
        if additional.is_empty() {
            return self.clone();
        }
        let mut merged = self.kv.clone();
        for (k, v) in additional {
            merged.insert(k.to_lowercase(), v);
        }
        Self { kv: merged }
    }
}

// ---------------------------------------------------------------------------
// User — server-side auth primitive (Go: auth.go)
// ---------------------------------------------------------------------------

/// Represents an authenticated (or unauthenticated) user.
///
/// Aligned with Go's `User` interface in `auth.go`. Implement this trait in
/// your auth middleware to attach user identity to [`CallContext`].
pub trait User: Send + Sync + std::fmt::Debug {
    /// Returns the username.
    fn name(&self) -> &str;
    /// Returns `true` if the request was authenticated.
    fn authenticated(&self) -> bool;
}

/// A simple authenticated user.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// The username.
    pub username: String,
}

impl AuthenticatedUser {
    /// Creates a new authenticated user.
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
        }
    }
}

impl User for AuthenticatedUser {
    fn name(&self) -> &str {
        &self.username
    }

    fn authenticated(&self) -> bool {
        true
    }
}

/// Represents an unauthenticated request.
#[derive(Debug, Clone, Copy)]
pub struct UnauthenticatedUser;

impl User for UnauthenticatedUser {
    fn name(&self) -> &'static str {
        ""
    }

    fn authenticated(&self) -> bool {
        false
    }
}

/// The metadata key for A2A extensions passed in request/response headers.
///
/// Aligned with Go's `ExtensionsMetaKey` in `extensions.go`.
pub const EXTENSIONS_META_KEY: &str = "x-a2a-extensions";

// ---------------------------------------------------------------------------
// CallContext — per-call scope (Go: middleware.go CallContext)
// ---------------------------------------------------------------------------

/// Holds information about the current server call scope.
///
/// Aligned with Go's `CallContext`. Created by the transport layer for every
/// incoming request and made available to [`CallInterceptor`]s and handlers.
#[derive(Debug)]
pub struct CallContext {
    method: String,
    request_meta: RequestMeta,
    activated_extensions: Vec<String>,
    /// The authenticated user for this request.
    pub user: Arc<dyn User>,
}

impl CallContext {
    /// Creates a new `CallContext`.
    pub fn new(method: impl Into<String>, meta: RequestMeta) -> Self {
        Self {
            method: method.into(),
            request_meta: meta,
            activated_extensions: Vec::new(),
            user: Arc::new(UnauthenticatedUser),
        }
    }

    /// Returns the handler method name being executed.
    #[must_use] 
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the request metadata.
    #[must_use] 
    pub const fn request_meta(&self) -> &RequestMeta {
        &self.request_meta
    }

    /// Returns the list of activated extension URIs.
    #[must_use] 
    pub fn activated_extensions(&self) -> &[String] {
        &self.activated_extensions
    }

    /// Activates an extension URI in this call scope.
    pub fn activate_extension(&mut self, uri: impl Into<String>) {
        self.activated_extensions.push(uri.into());
    }

    /// Checks if a specific extension is activated.
    #[must_use] 
    pub fn is_extension_active(&self, uri: &str) -> bool {
        self.activated_extensions.iter().any(|e| e == uri)
    }

    /// Returns URIs of extensions requested by the client.
    ///
    /// Reads from the `X-A2A-Extensions` header in [`RequestMeta`],
    /// aligned with Go's `Extensions.RequestedURIs()`.
    #[must_use]
    pub fn requested_extension_uris(&self) -> Vec<String> {
        self.request_meta
            .get(EXTENSIONS_META_KEY)
            .map(|v| v.to_vec())
            .unwrap_or_default()
    }

    /// Checks if a specific extension was requested by the client.
    #[must_use]
    pub fn is_extension_requested(&self, uri: &str) -> bool {
        self.requested_extension_uris().iter().any(|e| e == uri)
    }
}

// ---------------------------------------------------------------------------
// CallInterceptor (Go: middleware.go CallInterceptor)
// ---------------------------------------------------------------------------

/// Transport-agnostic request payload passed to interceptors.
#[derive(Debug)]
pub struct InterceptorRequest {
    /// The request payload (one of the A2A param types), serialized as JSON.
    pub payload: Option<serde_json::Value>,
}

/// Transport-agnostic response payload passed to interceptors.
#[derive(Debug)]
pub struct InterceptorResponse {
    /// The response payload, serialized as JSON. `None` when `error` is set.
    pub payload: Option<serde_json::Value>,
    /// Set when request processing failed.
    pub error: Option<crate::error::A2AError>,
}

/// Server-side call interceptor, applied before and after every handler method.
///
/// Aligned with Go's `CallInterceptor`. If multiple interceptors are added:
/// - `before` is executed in attachment order.
/// - `after` is executed in reverse order.
#[async_trait]
pub trait CallInterceptor: Send + Sync {
    /// Called before the handler method. Can observe, modify, or reject a request.
    async fn before(
        &self,
        ctx: &mut CallContext,
        req: &mut InterceptorRequest,
    ) -> Result<(), crate::error::A2AError>;

    /// Called after the handler method. Can observe, modify, or override a response.
    async fn after(
        &self,
        ctx: &CallContext,
        resp: &mut InterceptorResponse,
    ) -> Result<(), crate::error::A2AError>;
}

/// A no-op interceptor that passes everything through unchanged.
///
/// Embed this in your interceptor if you only need one of `before`/`after`.
pub struct PassthroughInterceptor;

#[async_trait]
impl CallInterceptor for PassthroughInterceptor {
    async fn before(
        &self,
        _ctx: &mut CallContext,
        _req: &mut InterceptorRequest,
    ) -> Result<(), crate::error::A2AError> {
        Ok(())
    }

    async fn after(
        &self,
        _ctx: &CallContext,
        _resp: &mut InterceptorResponse,
    ) -> Result<(), crate::error::A2AError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_meta_case_insensitive() {
        let mut src = HashMap::new();
        src.insert("Authorization".to_string(), vec!["Bearer xyz".to_string()]);
        let meta = RequestMeta::new(src);
        assert_eq!(
            meta.get("authorization"),
            Some(vec!["Bearer xyz".to_string()].as_slice())
        );
        assert_eq!(
            meta.get("AUTHORIZATION"),
            Some(vec!["Bearer xyz".to_string()].as_slice())
        );
    }

    #[test]
    fn test_authenticated_user() {
        let user = AuthenticatedUser::new("alice");
        assert_eq!(user.name(), "alice");
        assert!(user.authenticated());
    }

    #[test]
    fn test_unauthenticated_user() {
        let user = UnauthenticatedUser;
        assert_eq!(user.name(), "");
        assert!(!user.authenticated());
    }

    #[test]
    fn test_call_context_extensions() {
        let mut ctx = CallContext::new("message/send", RequestMeta::empty());
        assert!(!ctx.is_extension_active("urn:test:ext"));
        ctx.activate_extension("urn:test:ext");
        assert!(ctx.is_extension_active("urn:test:ext"));
    }

    #[test]
    fn test_call_context_requested_extensions() {
        let mut src = HashMap::new();
        src.insert(
            "x-a2a-extensions".to_string(),
            vec!["urn:ext:a".to_string(), "urn:ext:b".to_string()],
        );
        let meta = RequestMeta::new(src);
        let ctx = CallContext::new("message/send", meta);

        assert!(ctx.is_extension_requested("urn:ext:a"));
        assert!(ctx.is_extension_requested("urn:ext:b"));
        assert!(!ctx.is_extension_requested("urn:ext:c"));
        assert_eq!(ctx.requested_extension_uris().len(), 2);
    }
}
