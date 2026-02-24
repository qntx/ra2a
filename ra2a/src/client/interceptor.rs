//! Call interceptors for cross-cutting concerns (auth, logging, tracing).
//!
//! Aligned with Go's `CallInterceptor` in `a2aclient/middleware.go`.
//! Interceptors can inspect and modify both request/response payloads
//! and metadata (HTTP headers). `CallMeta` is propagated to the transport
//! layer via [`CALL_META`] task-local, mirroring Go's `context.Value`.

use std::any::Any;
use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::{A2AError, Result};

tokio::task_local! {
    /// Per-request metadata propagated from interceptors to the transport layer.
    ///
    /// Mirrors Go's `CallMetaFrom(ctx)` pattern. Transport implementations
    /// use [`call_meta`] to read interceptor-set headers (e.g. auth tokens).
    pub static CALL_META: CallMeta;
}

/// Returns a clone of the current request's [`CallMeta`], if set.
///
/// Transport implementations call this to retrieve headers set by interceptors.
pub fn call_meta() -> Option<CallMeta> {
    CALL_META.try_with(|m| m.clone()).ok()
}

/// Case-insensitive metadata map carried through interceptor chains.
///
/// Mirrors Go's `CallMeta` (backed by `http.Header`). Keys are lowercased
/// on insertion; values are multi-valued like HTTP headers.
#[derive(Debug, Clone, Default)]
pub struct CallMeta {
    inner: HashMap<String, Vec<String>>,
}

impl CallMeta {
    /// Appends a value, lowercasing the key. Duplicates are not added.
    pub fn append(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let vals = self
            .inner
            .entry(key.into().to_ascii_lowercase())
            .or_default();
        let v = value.into();
        if !vals.contains(&v) {
            vals.push(v);
        }
    }

    /// Returns the first value for the given key (case-insensitive).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.inner
            .get(&key.to_ascii_lowercase())
            .and_then(|v| v.first().map(String::as_str))
    }

    /// Returns all values for the given key (case-insensitive).
    pub fn get_all(&self, key: &str) -> &[String] {
        self.inner
            .get(&key.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Returns an iterator over all key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.inner.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Returns true if the map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Transport-agnostic outgoing request that interceptors can observe and modify.
///
/// Aligned with Go's `a2aclient.Request`. Carries the method name, metadata
/// (HTTP headers), the agent card, and the actual request payload.
pub struct Request {
    /// The method being called (e.g. `"SendMessage"`).
    pub method: String,
    /// The base URL of the agent interface.
    pub base_url: String,
    /// Metadata to attach as HTTP headers on the outgoing request.
    pub meta: CallMeta,
    /// The agent card, if already resolved.
    pub card: Option<crate::types::AgentCard>,
    /// The request payload. One of the `a2a` parameter types, boxed.
    pub payload: Box<dyn Any + Send>,
}

/// Transport-agnostic response that interceptors can observe and modify.
///
/// Aligned with Go's `a2aclient.Response`. Carries the method name, metadata,
/// the agent card, and the actual response payload or error.
pub struct Response {
    /// The method that was called.
    pub method: String,
    /// The base URL of the agent interface.
    pub base_url: String,
    /// Metadata from response headers.
    pub meta: CallMeta,
    /// The agent card, if resolved.
    pub card: Option<crate::types::AgentCard>,
    /// The response payload, if successful.
    pub payload: Option<Box<dyn Any + Send>>,
    /// The error, if the call failed.
    pub err: Option<A2AError>,
}

/// Middleware for intercepting client calls.
///
/// Aligned with Go's `CallInterceptor` interface. Both `before` and `after`
/// are invoked in the order interceptors were attached.
#[async_trait]
pub trait CallInterceptor: Send + Sync {
    /// Called before the transport call. May modify outgoing metadata and payload.
    async fn before(&self, req: &mut Request) -> Result<()> {
        let _ = req;
        Ok(())
    }

    /// Called after the transport call. May inspect/modify response or error.
    async fn after(&self, resp: &mut Response) -> Result<()> {
        let _ = resp;
        Ok(())
    }
}

/// No-op interceptor for embedding in custom implementations.
pub struct PassthroughInterceptor;

#[async_trait]
impl CallInterceptor for PassthroughInterceptor {}

/// A [`CallInterceptor`] that attaches static metadata to all outgoing requests.
///
/// Aligned with Go's `NewStaticCallMetaInjector`. Useful for injecting
/// fixed headers (e.g. API keys, tracing IDs) into every request.
///
/// # Example
///
/// ```
/// use ra2a::client::{CallMeta, StaticCallMetaInjector, Client};
///
/// let mut meta = CallMeta::default();
/// meta.append("x-api-key", "my-secret");
/// // client.with_interceptor(StaticCallMetaInjector::new(meta));
/// ```
pub struct StaticCallMetaInjector {
    inject: CallMeta,
}

impl StaticCallMetaInjector {
    /// Creates a new injector that appends the given metadata to every request.
    pub fn new(meta: CallMeta) -> Self {
        Self { inject: meta }
    }
}

#[async_trait]
impl CallInterceptor for StaticCallMetaInjector {
    async fn before(&self, req: &mut Request) -> Result<()> {
        for (key, values) in self.inject.iter() {
            for value in values {
                req.meta.append(key, value);
            }
        }
        Ok(())
    }
}
