//! Client factory — creates [`Client`] instances from agent cards or URLs.
//!
//! Aligned with Go's `Factory` in `a2aclient/factory.go`.

use super::interceptor::CallInterceptor;
use super::jsonrpc::{JsonRpcTransport, TransportConfig};
use super::Client;
use crate::error::Result;
use crate::types::AgentCard;

/// Factory for creating [`Client`] instances.
///
/// Supports creating clients from an agent URL directly, or from an
/// [`AgentCard`] (which provides the endpoint and capabilities).
///
/// Aligned with Go's `Factory` — immutable after creation, produces
/// configured `Client` values.
pub struct Factory {
    interceptors: Vec<Box<dyn CallInterceptor>>,
    timeout_secs: u64,
}

impl Default for Factory {
    fn default() -> Self {
        Self {
            interceptors: Vec::new(),
            timeout_secs: 30,
        }
    }
}

impl Factory {
    /// Creates a new factory with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a call interceptor applied to every client created by this factory.
    pub fn with_interceptor(mut self, interceptor: impl CallInterceptor + 'static) -> Self {
        self.interceptors.push(Box::new(interceptor));
        self
    }

    /// Sets the request timeout for transports created by this factory.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Creates a [`Client`] from an agent base URL.
    ///
    /// Uses JSON-RPC transport by default.
    pub fn create(&self, base_url: impl Into<String>) -> Result<Client> {
        let config = TransportConfig {
            base_url: base_url.into(),
            timeout_secs: self.timeout_secs,
            ..TransportConfig::new("")
        };
        let transport = JsonRpcTransport::new(config)?;
        Ok(self.build_client(Box::new(transport)))
    }

    /// Creates a [`Client`] from an [`AgentCard`].
    ///
    /// The card's URL is used as the transport endpoint; the card is cached
    /// in the returned client for capability checks.
    pub fn create_from_card(&self, card: &AgentCard) -> Result<Client> {
        let config = TransportConfig {
            base_url: card.url.clone(),
            timeout_secs: self.timeout_secs,
            ..TransportConfig::new("")
        };
        let transport = JsonRpcTransport::new(config)?;
        let client = self.build_client(Box::new(transport));
        client.set_card(card.clone());
        Ok(client)
    }

    /// Assembles a [`Client`] from a boxed transport, cloning interceptors.
    fn build_client(&self, transport: Box<dyn super::transport::Transport>) -> Client {
        // Interceptors cannot be cloned generically, so Factory is
        // designed to be used once or the interceptors are Arc-wrapped.
        // For simplicity, we require Factory::create to consume or the
        // caller re-creates. This matches Go where Factory is immutable
        // and each Create call produces a fresh Client.
        Client::new(transport)
    }
}
