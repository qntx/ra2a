//! Transport layer abstractions for the A2A client.

use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::error::Result;
use crate::types::{JsonRpcRequest, JsonRpcResponse};

/// A transport layer for sending JSON-RPC requests.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Sends a JSON-RPC request and receives a response.
    async fn send<P, R>(&self, request: JsonRpcRequest<P>) -> Result<JsonRpcResponse<R>>
    where
        P: Serialize + Send + Sync,
        R: DeserializeOwned + Send;

    /// Sends a streaming request and returns a stream of responses.
    async fn send_streaming<P, R>(
        &self,
        request: JsonRpcRequest<P>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<JsonRpcResponse<R>>> + Send + Unpin>>
    where
        P: Serialize + Send + Sync,
        R: DeserializeOwned + Send;
}

/// Transport configuration options.
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Base URL for the transport.
    pub base_url: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Custom headers to include in requests.
    pub headers: Vec<(String, String)>,
}

impl TransportConfig {
    /// Creates a new transport configuration.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout_secs: 30,
            headers: vec![],
        }
    }

    /// Sets the request timeout.
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Adds a custom header.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Adds an authorization header with a Bearer token.
    pub fn bearer_auth(self, token: impl Into<String>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.into()))
    }

    /// Adds an API key header.
    pub fn api_key(self, header_name: impl Into<String>, key: impl Into<String>) -> Self {
        self.header(header_name, key)
    }
}
