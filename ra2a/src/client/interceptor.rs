//! Call interceptors for cross-cutting concerns (auth, logging, tracing).
//!
//! Aligned with Go's `CallInterceptor` in `a2aclient/middleware.go`.
//! Interceptors are applied by [`super::Client`] before and after each
//! transport call.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::Result;

/// Case-insensitive metadata map carried through interceptor chains.
///
/// Mirrors Go's `CallMeta` (backed by `http.Header`). Keys are lowercased
/// on insertion; values are multi-valued like HTTP headers.
#[derive(Debug, Clone, Default)]
pub struct CallMeta {
    inner: HashMap<String, Vec<String>>,
}

impl CallMeta {
    /// Inserts a value, lowercasing the key.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner
            .entry(key.into().to_ascii_lowercase())
            .or_default()
            .push(value.into());
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

/// Context passed to interceptors before a transport call.
///
/// Aligned with Go's `CallContext` — carries the method name and the
/// agent card (if resolved).
pub struct CallContext {
    /// The JSON-RPC method being called (e.g. `"message/send"`).
    pub method: String,
    /// The agent card, if already resolved.
    pub agent_card: Option<crate::types::AgentCard>,
}

/// Outgoing request metadata that interceptors can modify.
pub struct Request {
    /// Metadata to attach as HTTP headers on the outgoing request.
    pub meta: CallMeta,
}

/// Incoming response metadata available to interceptors.
pub struct Response {
    /// Metadata extracted from response headers.
    pub meta: CallMeta,
}

/// Middleware for intercepting client calls.
///
/// Aligned with Go's `CallInterceptor` interface. Interceptors are invoked
/// in order for `before`, and in reverse order for `after`.
#[async_trait]
pub trait CallInterceptor: Send + Sync {
    /// Called before the transport call. May modify outgoing metadata.
    async fn before(&self, ctx: &CallContext, req: &mut Request) -> Result<()> {
        let _ = (ctx, req);
        Ok(())
    }

    /// Called after the transport call. May inspect response metadata.
    async fn after(&self, ctx: &CallContext, resp: &mut Response) -> Result<()> {
        let _ = (ctx, resp);
        Ok(())
    }
}
