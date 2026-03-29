//! Client-side call interceptors and service parameters.
//!
//! - [`ServiceParams`] — A2A service parameters (protocol version, extensions)
//!   propagated via HTTP headers or gRPC metadata.
//! - [`CallInterceptor`] — middleware for observing/modifying requests and responses.

use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::error::{A2AError, Result};

tokio::task_local! {
    /// Per-request service parameters propagated from interceptors to the transport layer.
    pub static SERVICE_PARAMS: ServiceParams;
}

/// Returns a clone of the current request's [`ServiceParams`], if set.
pub fn current_service_params() -> Option<ServiceParams> {
    SERVICE_PARAMS.try_with(|m| m.clone()).ok()
}

/// A2A service parameters carried through interceptor chains and transport calls.
///
/// Corresponds to the A2A specification's "Service Parameters" concept.
/// Standard keys: `A2A-Version`, `A2A-Extensions`.
#[derive(Debug, Clone, Default)]
pub struct ServiceParams {
    inner: HashMap<String, Vec<String>>,
}

impl ServiceParams {
    /// Appends one or more values for a key (case-preserved).
    pub fn append(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let vals = self.inner.entry(key.into()).or_default();
        let v = value.into();
        if !vals.contains(&v) {
            vals.push(v);
        }
    }

    /// Returns the first value for the given key.
    #[must_use]
    pub fn get_first(&self, key: &str) -> Option<&str> {
        self.inner
            .get(key)
            .and_then(|v| v.first().map(String::as_str))
    }

    /// Returns all values for the given key.
    #[must_use]
    pub fn get_all(&self, key: &str) -> &[String] {
        self.inner.get(key).map_or(&[], Vec::as_slice)
    }

    /// Returns an iterator over all key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.inner.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Returns the full map.
    #[must_use]
    pub fn as_map(&self) -> &HashMap<String, Vec<String>> {
        &self.inner
    }

    /// Returns `true` if no parameters are set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Transport-agnostic outgoing request that interceptors can observe and modify.
pub struct Request {
    /// The method being called (e.g. `"SendMessage"`).
    pub method: String,
    /// The agent card, if already resolved.
    pub card: Option<crate::types::AgentCard>,
    /// Service parameters to propagate to the transport.
    pub service_params: ServiceParams,
    /// The request payload (one of the A2A request types), type-erased.
    pub payload: Box<dyn Any + Send>,
}

/// Transport-agnostic response that interceptors can observe and modify.
pub struct Response {
    /// The method that was called.
    pub method: String,
    /// The agent card, if resolved.
    pub card: Option<crate::types::AgentCard>,
    /// The response payload, if successful.
    pub payload: Option<Box<dyn Any + Send>>,
    /// The error, if the call failed.
    pub err: Option<A2AError>,
}

/// Middleware for intercepting client calls.
///
/// Both `before` and `after` are invoked in the order interceptors were attached.
pub trait CallInterceptor: Send + Sync {
    /// Called before the transport call. May modify service parameters and payload.
    fn before<'a>(
        &'a self,
        req: &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let _ = req;
        Box::pin(async { Ok(()) })
    }

    /// Called after the transport call. May inspect/modify response or error.
    fn after<'a>(
        &'a self,
        resp: &'a mut Response,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let _ = resp;
        Box::pin(async { Ok(()) })
    }
}

/// No-op interceptor.
pub struct PassthroughInterceptor;

impl CallInterceptor for PassthroughInterceptor {}

/// A [`CallInterceptor`] that attaches static service parameters to all requests.
///
/// Useful for injecting fixed headers (e.g. API keys, tracing IDs).
///
/// # Example
///
/// ```
/// use ra2a::client::{ServiceParams, StaticParamsInjector};
///
/// let mut params = ServiceParams::default();
/// params.append("x-api-key", "my-secret");
/// // client.with_interceptor(StaticParamsInjector::new(params));
/// ```
pub struct StaticParamsInjector {
    inject: ServiceParams,
}

impl StaticParamsInjector {
    /// Creates a new injector.
    pub fn new(params: ServiceParams) -> Self {
        Self { inject: params }
    }
}

impl CallInterceptor for StaticParamsInjector {
    fn before<'a>(
        &'a self,
        req: &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            for (key, values) in self.inject.iter() {
                for value in values {
                    req.service_params.append(key, value);
                }
            }
            Ok(())
        })
    }
}
