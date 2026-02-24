//! HTTP-based A2A client with automatic SSE streaming support.
//!
//! Provides [`A2AClient`] — the single, unified client for interacting with
//! A2A agents over HTTP/JSON-RPC. Streaming is auto-negotiated based on
//! [`ClientConfig`] and the agent's advertised capabilities.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use tracing::{debug, instrument};

use super::{Client, ClientConfig, ClientEvent, EventStream};
use crate::error::{A2AError, Result};
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, GetTaskPushConfigParams, JsonRpcRequest,
    JsonRpcResponse, ListTaskPushConfigParams, Message, MessageSendConfig, MessageSendParams,
    SendMessageResult, Task, TaskIdParams, TaskPushConfig, TaskQueryParams,
};

/// Unified HTTP-based A2A client with optional SSE streaming.
///
/// Automatically chooses between `message/send` (synchronous) and
/// `message/stream` (SSE) based on the client configuration and the
/// agent's advertised capabilities.
#[derive(Debug, Clone)]
pub struct A2AClient {
    http_client: reqwest::Client,
    base_url: String,
    card_url: String,
    config: ClientConfig,
    agent_card: Option<Arc<AgentCard>>,
}

impl A2AClient {
    /// Creates a new client for the given agent URL with default config.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        Self::with_config(base_url, ClientConfig::default())
    }

    /// Creates a new client with custom configuration.
    pub fn with_config(base_url: impl Into<String>, config: ClientConfig) -> Result<Self> {
        Self::with_headers(base_url, config, HeaderMap::new())
    }

    /// Creates a new client with custom configuration and HTTP headers.
    pub fn with_headers(
        base_url: impl Into<String>,
        config: ClientConfig,
        headers: HeaderMap,
    ) -> Result<Self> {
        let base_url = base_url.into();
        let card_url = crate::agent_card_url(&base_url);

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .default_headers(headers)
            .build()
            .map_err(|e| A2AError::Other(e.to_string()))?;

        Ok(Self {
            http_client,
            base_url,
            card_url,
            config,
            agent_card: None,
        })
    }

    /// Returns the base URL of the agent.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the client configuration.
    #[must_use]
    pub const fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Returns the cached agent card, if available.
    #[must_use]
    pub fn cached_agent_card(&self) -> Option<&AgentCard> {
        self.agent_card.as_deref()
    }

    /// Returns `true` if streaming is enabled in the config and
    /// the agent (if known) also supports it.
    #[must_use]
    pub fn supports_streaming(&self) -> bool {
        if !self.config.streaming {
            return false;
        }
        self.agent_card
            .as_ref()
            .is_none_or(|card| card.supports_streaming())
    }

    /// Sends a typed JSON-RPC request and deserialises the result.
    async fn send_request<P, R>(&self, request: JsonRpcRequest<P>) -> Result<R>
    where
        P: serde::Serialize + Send + Sync,
        R: serde::de::DeserializeOwned,
    {
        let response = self
            .http_client
            .post(&self.base_url)
            .header(CONTENT_TYPE, "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        let json_response: JsonRpcResponse<R> = response.json().await?;
        match json_response {
            JsonRpcResponse::Success(s) => Ok(s.result),
            JsonRpcResponse::Error(e) => Err(A2AError::JsonRpc(e.error)),
        }
    }

    /// Non-streaming `message/send` — returns a single-item stream.
    async fn send_non_streaming(&self, message: Message) -> Result<EventStream> {
        let params = MessageSendParams::new(message);
        let request: JsonRpcRequest<MessageSendParams> =
            JsonRpcRequest::new("message/send", params);

        let result: SendMessageResult = self.send_request(request).await?;

        let event = match result {
            SendMessageResult::Task(task) => ClientEvent::TaskUpdate {
                task: Box::new(task),
                update: None,
            },
            SendMessageResult::Message(msg) => ClientEvent::Message(msg),
        };
        Ok(Box::pin(stream::once(async move { Ok(event) })))
    }

    /// Streaming `message/stream` — returns an SSE event stream.
    async fn send_streaming(&self, message: Message) -> Result<EventStream> {
        let mut params = MessageSendParams::new(message);

        // Apply client-level send configuration
        if !self.config.accepted_output_modes.is_empty()
            || !self.config.push_notification_configs.is_empty()
        {
            let mut cfg = MessageSendConfig::default();
            if !self.config.accepted_output_modes.is_empty() {
                cfg.accepted_output_modes = Some(self.config.accepted_output_modes.clone());
            }
            if let Some(push_cfg) = self.config.push_notification_configs.first() {
                cfg.push_notification_config = Some(push_cfg.clone());
            }
            params.configuration = Some(cfg);
        }

        let request: JsonRpcRequest<MessageSendParams> =
            JsonRpcRequest::new("message/stream", params);
        let body = serde_json::to_string(&request)?;

        debug!("Sending streaming request to {}", self.base_url);

        let response = self
            .http_client
            .post(&self.base_url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "text/event-stream")
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        let byte_stream = response.bytes_stream();
        let line_stream =
            super::sse::SseLineStream::new(futures::StreamExt::map(byte_stream, |result| {
                result
                    .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
                    .map_err(|e| e.to_string())
            }));
        Ok(Box::pin(line_stream))
    }

    /// Fetches the agent card from the well-known URL.
    async fn fetch_agent_card(&self) -> Result<AgentCard> {
        let response = self.http_client.get(&self.card_url).send().await?;
        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }
        response.json().await.map_err(Into::into)
    }
}

#[async_trait]
impl Client for A2AClient {
    #[instrument(skip(self, message), fields(task_id = ?message.task_id))]
    async fn send_message(&self, message: Message) -> Result<EventStream> {
        if self.supports_streaming() {
            self.send_streaming(message).await
        } else {
            self.send_non_streaming(message).await
        }
    }

    #[instrument(skip(self))]
    async fn get_task(&self, params: TaskQueryParams) -> Result<Task> {
        let request: JsonRpcRequest<TaskQueryParams> = JsonRpcRequest::new("tasks/get", params);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn cancel_task(&self, params: TaskIdParams) -> Result<Task> {
        let request: JsonRpcRequest<TaskIdParams> = JsonRpcRequest::new("tasks/cancel", params);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn set_task_callback(&self, config: TaskPushConfig) -> Result<TaskPushConfig> {
        let request: JsonRpcRequest<TaskPushConfig> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/set", config);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn get_task_callback(&self, params: GetTaskPushConfigParams) -> Result<TaskPushConfig> {
        let request: JsonRpcRequest<GetTaskPushConfigParams> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/get", params);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn resubscribe(&self, params: TaskIdParams) -> Result<EventStream> {
        if !self.supports_streaming() {
            let task = self.get_task(TaskQueryParams::new(&params.id)).await?;
            let event = ClientEvent::TaskUpdate {
                task: Box::new(task),
                update: None,
            };
            return Ok(Box::pin(stream::once(async move { Ok(event) })));
        }

        let request: JsonRpcRequest<TaskIdParams> =
            JsonRpcRequest::new("tasks/resubscribe", params);
        let body = serde_json::to_string(&request)?;

        let response = self
            .http_client
            .post(&self.base_url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "text/event-stream")
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        let byte_stream = response.bytes_stream();
        let line_stream =
            super::sse::SseLineStream::new(futures::StreamExt::map(byte_stream, |result| {
                result
                    .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
                    .map_err(|e| e.to_string())
            }));
        Ok(Box::pin(line_stream))
    }

    #[instrument(skip(self))]
    async fn list_task_push_notification_config(
        &self,
        params: ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        let request: JsonRpcRequest<ListTaskPushConfigParams> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/list", params);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn delete_task_push_notification_config(
        &self,
        params: DeleteTaskPushConfigParams,
    ) -> Result<()> {
        let request: JsonRpcRequest<DeleteTaskPushConfigParams> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/delete", params);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn get_agent_card(&self) -> Result<AgentCard> {
        self.fetch_agent_card().await
    }
}

/// Builder for creating an [`A2AClient`] with custom options.
#[derive(Debug)]
pub struct A2AClientBuilder {
    base_url: String,
    config: ClientConfig,
    headers: HeaderMap,
    timeout_secs: Option<u64>,
}

impl A2AClientBuilder {
    /// Creates a new builder for the given agent URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            config: ClientConfig::default(),
            headers: HeaderMap::new(),
            timeout_secs: None,
        }
    }

    /// Replaces the entire client configuration.
    #[must_use]
    pub fn config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    /// Enables or disables SSE streaming (enabled by default).
    #[must_use]
    pub const fn streaming(mut self, enabled: bool) -> Self {
        self.config.streaming = enabled;
        self
    }

    /// Adds a custom HTTP header.
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::try_from(name.as_ref()),
            HeaderValue::from_str(value.as_ref()),
        ) {
            self.headers.insert(name, value);
        }
        self
    }

    /// Sets Bearer authentication.
    pub fn bearer_auth(self, token: impl AsRef<str>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.as_ref()))
    }

    /// Sets an API-key header.
    pub fn api_key(self, header_name: impl AsRef<str>, key: impl AsRef<str>) -> Self {
        self.header(header_name, key)
    }

    /// Sets the request timeout in seconds.
    #[must_use]
    pub const fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Sets the accepted output modes for message requests.
    #[must_use]
    pub fn accepted_output_modes(mut self, modes: Vec<String>) -> Self {
        self.config.accepted_output_modes = modes;
        self
    }

    /// Builds the [`A2AClient`].
    pub fn build(mut self) -> Result<A2AClient> {
        if let Some(timeout) = self.timeout_secs {
            self.config.timeout_secs = timeout;
        }
        A2AClient::with_headers(self.base_url, self.config, self.headers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_builder() {
        let client = A2AClientBuilder::new("https://agent.example.com")
            .timeout(60)
            .streaming(true)
            .bearer_auth("test-token")
            .build()
            .unwrap();

        assert_eq!(client.base_url(), "https://agent.example.com");
        assert!(client.config().streaming);
    }

    #[test]
    fn test_card_url_generation() {
        let client = A2AClient::new("https://agent.example.com").unwrap();
        assert_eq!(
            client.card_url,
            "https://agent.example.com/.well-known/agent-card.json"
        );

        let client = A2AClient::new("https://agent.example.com/").unwrap();
        assert_eq!(
            client.card_url,
            "https://agent.example.com/.well-known/agent-card.json"
        );
    }

    #[test]
    fn test_streaming_disabled() {
        let client = A2AClientBuilder::new("https://agent.example.com")
            .streaming(false)
            .build()
            .unwrap();

        assert!(!client.supports_streaming());
    }
}
