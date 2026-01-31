//! Client-side middleware for intercepting and modifying requests.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::types::AgentCard;

/// A context passed with each client call for call-specific configuration.
#[derive(Debug, Clone, Default)]
pub struct ClientCallContext {
    /// Mutable state for passing data between interceptors.
    pub state: HashMap<String, serde_json::Value>,
}

impl ClientCallContext {
    /// Creates a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets a value from the context state.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.state.get(key)
    }

    /// Sets a value in the context state.
    pub fn set(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.state.insert(key.into(), value);
    }

    /// Removes a value from the context state.
    pub fn remove(&mut self, key: &str) -> Option<serde_json::Value> {
        self.state.remove(key)
    }
}

/// An abstract trait for client-side call interceptors.
///
/// Interceptors can inspect and modify requests before they are sent,
/// which is ideal for concerns like authentication, logging, or tracing.
#[async_trait]
pub trait ClientCallInterceptor: Send + Sync {
    /// Intercepts a client call before the request is sent.
    ///
    /// # Arguments
    /// * `method_name` - The name of the RPC method (e.g., "message/send")
    /// * `request_payload` - The JSON-RPC request payload
    /// * `headers` - The HTTP headers to be sent
    /// * `agent_card` - The AgentCard associated with the client
    /// * `context` - The ClientCallContext for this specific call
    ///
    /// # Returns
    /// A tuple containing the (potentially modified) request_payload and headers
    async fn intercept(
        &self,
        method_name: &str,
        request_payload: serde_json::Value,
        headers: HashMap<String, String>,
        agent_card: Option<&AgentCard>,
        context: Option<&ClientCallContext>,
    ) -> crate::error::Result<(serde_json::Value, HashMap<String, String>)>;
}

/// A logging interceptor that logs all outgoing requests.
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
    async fn intercept(
        &self,
        method_name: &str,
        request_payload: serde_json::Value,
        headers: HashMap<String, String>,
        _agent_card: Option<&AgentCard>,
        _context: Option<&ClientCallContext>,
    ) -> crate::error::Result<(serde_json::Value, HashMap<String, String>)> {
        match self.log_level {
            LogLevel::Debug => {
                tracing::debug!(method = method_name, payload = ?request_payload, "Outgoing request");
            }
            LogLevel::Info => {
                tracing::info!(method = method_name, "Outgoing request");
            }
            LogLevel::Warn => {
                tracing::warn!(method = method_name, "Outgoing request");
            }
        }
        Ok((request_payload, headers))
    }
}

/// An authentication interceptor that adds authorization headers.
#[derive(Debug, Clone)]
pub struct AuthInterceptor {
    /// The authorization header value.
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
            auth_header: format!("Basic {}", encoded),
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
    async fn intercept(
        &self,
        _method_name: &str,
        request_payload: serde_json::Value,
        mut headers: HashMap<String, String>,
        _agent_card: Option<&AgentCard>,
        _context: Option<&ClientCallContext>,
    ) -> crate::error::Result<(serde_json::Value, HashMap<String, String>)> {
        headers.insert("Authorization".to_string(), self.auth_header.clone());
        Ok((request_payload, headers))
    }
}

/// A chain of interceptors that are executed in order.
#[derive(Default)]
pub struct InterceptorChain {
    interceptors: Vec<Box<dyn ClientCallInterceptor>>,
}

impl InterceptorChain {
    /// Creates a new empty interceptor chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an interceptor to the chain.
    pub fn add<I: ClientCallInterceptor + 'static>(mut self, interceptor: I) -> Self {
        self.interceptors.push(Box::new(interceptor));
        self
    }

    /// Executes all interceptors in the chain.
    pub async fn execute(
        &self,
        method_name: &str,
        mut request_payload: serde_json::Value,
        mut headers: HashMap<String, String>,
        agent_card: Option<&AgentCard>,
        context: Option<&ClientCallContext>,
    ) -> crate::error::Result<(serde_json::Value, HashMap<String, String>)> {
        for interceptor in &self.interceptors {
            (request_payload, headers) = interceptor
                .intercept(method_name, request_payload, headers, agent_card, context)
                .await?;
        }
        Ok((request_payload, headers))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_call_context() {
        let mut ctx = ClientCallContext::new();
        ctx.set("key", serde_json::json!("value"));
        assert_eq!(ctx.get("key"), Some(&serde_json::json!("value")));
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
}
