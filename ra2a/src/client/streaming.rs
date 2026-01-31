//! Streaming A2A client implementation.
//!
//! This module provides a full-featured A2A client with SSE streaming support,
//! combining both synchronous and asynchronous communication patterns.

use async_trait::async_trait;
use futures::stream;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, instrument};

use super::{Client, ClientConfig, ClientEvent, EventStream};
use crate::error::{A2AError, Result};
use crate::types::{
    AgentCard, DeleteTaskPushNotificationConfigParams, GetTaskPushNotificationConfigParams,
    JsonRpcRequest, JsonRpcResponse, ListTaskPushNotificationConfigParams, Message,
    MessageSendConfiguration, MessageSendParams, SendMessageResult, Task, TaskIdParams,
    TaskPushNotificationConfig, TaskQueryParams,
};

/// A full-featured A2A client with SSE streaming support.
///
/// This client automatically chooses between streaming and non-streaming
/// requests based on the agent's capabilities and client configuration.
#[derive(Debug, Clone)]
pub struct StreamingClient {
    http_client: reqwest::Client,
    base_url: String,
    card_url: String,
    config: ClientConfig,
    agent_card: Option<Arc<AgentCard>>,
}

impl StreamingClient {
    /// Creates a new streaming client for the given agent URL.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        Self::with_config(base_url, ClientConfig::default())
    }

    /// Creates a new streaming client with custom configuration.
    pub fn with_config(base_url: impl Into<String>, config: ClientConfig) -> Result<Self> {
        let base_url = base_url.into();
        let card_url = format!("{}/.well-known/agent.json", base_url.trim_end_matches('/'));

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| A2AError::Connection(e.to_string()))?;

        Ok(Self {
            http_client,
            base_url,
            card_url,
            config,
            agent_card: None,
        })
    }

    /// Creates a new streaming client with custom headers.
    pub fn with_headers(
        base_url: impl Into<String>,
        config: ClientConfig,
        headers: HeaderMap,
    ) -> Result<Self> {
        let base_url = base_url.into();
        let card_url = format!("{}/.well-known/agent.json", base_url.trim_end_matches('/'));

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .default_headers(headers)
            .build()
            .map_err(|e| A2AError::Connection(e.to_string()))?;

        Ok(Self {
            http_client,
            base_url,
            card_url,
            config,
            agent_card: None,
        })
    }

    /// Returns the base URL of the agent.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the client configuration.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Returns the cached agent card, if available.
    pub fn cached_agent_card(&self) -> Option<&AgentCard> {
        self.agent_card.as_deref()
    }

    /// Checks if streaming is supported by both client and agent.
    pub fn supports_streaming(&self) -> bool {
        if !self.config.streaming {
            return false;
        }
        self.agent_card
            .as_ref()
            .is_none_or(|card| card.supports_streaming())
    }

    /// Sends a message with streaming support.
    ///
    /// If streaming is supported, returns a stream of events.
    /// Otherwise, falls back to a single response.
    #[instrument(skip(self, message), fields(task_id = ?message.task_id))]
    pub async fn send_message_streaming(&self, message: Message) -> Result<EventStream> {
        if self.supports_streaming() {
            self.send_streaming_request(message).await
        } else {
            self.send_non_streaming_request(message).await
        }
    }

    /// Sends a streaming request using SSE.
    async fn send_streaming_request(&self, message: Message) -> Result<EventStream> {
        let mut params = MessageSendParams::new(message);

        // Apply client configuration
        if !self.config.accepted_output_modes.is_empty()
            || !self.config.push_notification_configs.is_empty()
        {
            let mut config = MessageSendConfiguration::default();
            if !self.config.accepted_output_modes.is_empty() {
                config.accepted_output_modes = Some(self.config.accepted_output_modes.clone());
            }
            if let Some(push_config) = self.config.push_notification_configs.first() {
                config.push_notification_config = Some(push_config.clone());
            }
            params.configuration = Some(config);
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

        // Create a stream from the response bytes
        let byte_stream = response.bytes_stream();
        let line_stream =
            super::sse::SseLineStream::new(futures::StreamExt::map(byte_stream, |result| {
                result
                    .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
                    .map_err(|e| e.to_string())
            }));

        Ok(Box::pin(line_stream))
    }

    /// Sends a non-streaming request.
    async fn send_non_streaming_request(&self, message: Message) -> Result<EventStream> {
        let params = MessageSendParams::new(message);
        let request: JsonRpcRequest<MessageSendParams> =
            JsonRpcRequest::new("message/send", params);

        let result: SendMessageResult = self.send_request(request).await?;

        let event = match result {
            SendMessageResult::Task(task) => ClientEvent::TaskUpdate { task, update: None },
            SendMessageResult::Message(msg) => ClientEvent::Message(msg),
        };

        Ok(Box::pin(stream::once(async move { Ok(event) })))
    }

    /// Sends a JSON-RPC request and returns the result.
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
            JsonRpcResponse::Success(success) => Ok(success.result),
            JsonRpcResponse::Error(error) => Err(A2AError::JsonRpc(error.error)),
        }
    }

    /// Fetches the agent card from the well-known URL.
    async fn fetch_agent_card(&self) -> Result<AgentCard> {
        let response = self.http_client.get(&self.card_url).send().await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        let card: AgentCard = response.json().await?;
        Ok(card)
    }
}

#[async_trait]
impl Client for StreamingClient {
    #[instrument(skip(self, message), fields(task_id = ?message.task_id))]
    async fn send_message(&self, message: Message) -> Result<EventStream> {
        self.send_message_streaming(message).await
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
    async fn set_task_callback(
        &self,
        config: TaskPushNotificationConfig,
    ) -> Result<TaskPushNotificationConfig> {
        let request: JsonRpcRequest<TaskPushNotificationConfig> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/set", config);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn get_task_callback(
        &self,
        params: GetTaskPushNotificationConfigParams,
    ) -> Result<TaskPushNotificationConfig> {
        let request: JsonRpcRequest<GetTaskPushNotificationConfigParams> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/get", params);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn resubscribe(&self, params: TaskIdParams) -> Result<EventStream> {
        if !self.supports_streaming() {
            // Fall back to getting current task state
            let task = self.get_task(TaskQueryParams::new(&params.id)).await?;
            let event = ClientEvent::TaskUpdate { task, update: None };
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
        params: ListTaskPushNotificationConfigParams,
    ) -> Result<Vec<TaskPushNotificationConfig>> {
        let request: JsonRpcRequest<ListTaskPushNotificationConfigParams> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/list", params);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn delete_task_push_notification_config(
        &self,
        params: DeleteTaskPushNotificationConfigParams,
    ) -> Result<()> {
        let request: JsonRpcRequest<DeleteTaskPushNotificationConfigParams> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/delete", params);
        self.send_request(request).await
    }

    #[instrument(skip(self))]
    async fn get_agent_card(&self) -> Result<AgentCard> {
        self.fetch_agent_card().await
    }
}

/// Builder for creating a `StreamingClient` with custom options.
#[derive(Debug)]
pub struct StreamingClientBuilder {
    base_url: String,
    config: ClientConfig,
    headers: HeaderMap,
    timeout_secs: Option<u64>,
}

impl StreamingClientBuilder {
    /// Creates a new builder for the given agent URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            config: ClientConfig::default(),
            headers: HeaderMap::new(),
            timeout_secs: None,
        }
    }

    /// Sets the client configuration.
    pub fn config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    /// Enables or disables streaming.
    pub fn streaming(mut self, enabled: bool) -> Self {
        self.config.streaming = enabled;
        self
    }

    /// Adds a custom header.
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::try_from(name.as_ref()),
            HeaderValue::from_str(value.as_ref()),
        ) {
            self.headers.insert(name, value);
        }
        self
    }

    /// Sets the Bearer authentication token.
    pub fn bearer_auth(self, token: impl AsRef<str>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.as_ref()))
    }

    /// Sets the API key.
    pub fn api_key(self, header_name: impl AsRef<str>, key: impl AsRef<str>) -> Self {
        self.header(header_name, key)
    }

    /// Sets the request timeout.
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Sets the accepted output modes.
    pub fn accepted_output_modes(mut self, modes: Vec<String>) -> Self {
        self.config.accepted_output_modes = modes;
        self
    }

    /// Builds the streaming client.
    pub fn build(mut self) -> Result<StreamingClient> {
        if let Some(timeout) = self.timeout_secs {
            self.config.timeout_secs = timeout;
        }
        StreamingClient::with_headers(self.base_url, self.config, self.headers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_client_builder() {
        let client = StreamingClientBuilder::new("https://agent.example.com")
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
        let client = StreamingClient::new("https://agent.example.com").unwrap();
        assert_eq!(
            client.card_url,
            "https://agent.example.com/.well-known/agent.json"
        );
    }
}
