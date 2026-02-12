//! HTTP-based A2A client implementation.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};

use super::transports::TransportOptions;
use super::{Client, ClientConfig, ClientEvent, EventStream};
use crate::error::{A2AError, Result};
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, GetTaskPushConfigParams, JsonRpcRequest,
    JsonRpcResponse, ListTaskPushConfigParams, Message, MessageSendParams, SendMessageResult, Task,
    TaskIdParams, TaskPushConfig, TaskQueryParams,
};

/// HTTP-based A2A client.
///
/// This client uses HTTP/HTTPS with JSON-RPC for communication with A2A agents.
#[derive(Debug, Clone)]
pub struct A2AClient {
    /// The HTTP client.
    http_client: reqwest::Client,
    /// The base URL of the agent.
    base_url: String,
    /// The agent card URL (typically `base_url` + "/.well-known/agent-card.json").
    card_url: String,
    /// Client configuration.
    config: ClientConfig,
    /// Cached agent card (reserved for future use).
    #[allow(dead_code)]
    agent_card: Option<Arc<AgentCard>>,
}

impl A2AClient {
    /// Creates a new A2A client for the given agent URL.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        Self::with_config(base_url, ClientConfig::default())
    }

    /// Creates a new A2A client with custom configuration.
    pub fn with_config(base_url: impl Into<String>, config: ClientConfig) -> Result<Self> {
        let base_url = base_url.into();
        let card_url = format!("{}/.well-known/agent-card.json", base_url.trim_end_matches('/'));

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
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

    /// Creates a new A2A client with transport options.
    pub fn with_transport(transport: TransportOptions) -> Result<Self> {
        let mut headers = HeaderMap::new();
        for (name, value) in &transport.headers {
            if let (Ok(name), Ok(value)) = (
                reqwest::header::HeaderName::try_from(name.as_str()),
                HeaderValue::from_str(value),
            ) {
                headers.insert(name, value);
            }
        }

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(transport.timeout_secs))
            .default_headers(headers)
            .build()
            .map_err(|e| A2AError::Other(e.to_string()))?;

        let card_url = format!(
            "{}/.well-known/agent-card.json",
            transport.base_url.trim_end_matches('/')
        );

        Ok(Self {
            http_client,
            base_url: transport.base_url,
            card_url,
            config: ClientConfig::default(),
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

    /// Sends a JSON-RPC request to the agent.
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
impl Client for A2AClient {
    async fn send_message(&self, message: Message) -> Result<EventStream> {
        let params = MessageSendParams::new(message);
        let request: JsonRpcRequest<MessageSendParams> =
            JsonRpcRequest::new("message/send", params);

        let result: SendMessageResult = self.send_request(request).await?;

        // Convert the result to a stream of events
        let event = match result {
            SendMessageResult::Task(task) => ClientEvent::TaskUpdate { task: Box::new(task), update: None },
            SendMessageResult::Message(msg) => ClientEvent::Message(msg),
        };

        let stream = stream::once(async move { Ok(event) });
        Ok(Box::pin(stream))
    }

    async fn get_task(&self, params: TaskQueryParams) -> Result<Task> {
        let request: JsonRpcRequest<TaskQueryParams> = JsonRpcRequest::new("tasks/get", params);
        self.send_request(request).await
    }

    async fn cancel_task(&self, params: TaskIdParams) -> Result<Task> {
        let request: JsonRpcRequest<TaskIdParams> = JsonRpcRequest::new("tasks/cancel", params);
        self.send_request(request).await
    }

    async fn set_task_callback(&self, config: TaskPushConfig) -> Result<TaskPushConfig> {
        let request: JsonRpcRequest<TaskPushConfig> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/set", config);
        self.send_request(request).await
    }

    async fn get_task_callback(&self, params: GetTaskPushConfigParams) -> Result<TaskPushConfig> {
        let request: JsonRpcRequest<GetTaskPushConfigParams> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/get", params);
        self.send_request(request).await
    }

    async fn resubscribe(&self, params: TaskIdParams) -> Result<EventStream> {
        // For non-streaming client, we just get the current task state
        let task = self.get_task(TaskQueryParams::new(&params.id)).await?;
        let event = ClientEvent::TaskUpdate { task: Box::new(task), update: None };
        let stream = stream::once(async move { Ok(event) });
        Ok(Box::pin(stream))
    }

    async fn list_task_push_notification_config(
        &self,
        params: ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        let request: JsonRpcRequest<ListTaskPushConfigParams> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/list", params);
        self.send_request(request).await
    }

    async fn delete_task_push_notification_config(
        &self,
        params: DeleteTaskPushConfigParams,
    ) -> Result<()> {
        let request: JsonRpcRequest<DeleteTaskPushConfigParams> =
            JsonRpcRequest::new("tasks/pushNotificationConfig/delete", params);
        self.send_request(request).await
    }

    async fn get_agent_card(&self) -> Result<AgentCard> {
        self.fetch_agent_card().await
    }
}

/// Builder for creating an A2A client with custom options.
#[derive(Debug)]
pub struct A2AClientBuilder {
    base_url: String,
    config: ClientConfig,
    headers: Vec<(String, String)>,
    timeout_secs: Option<u64>,
}

impl A2AClientBuilder {
    /// Creates a new builder for the given agent URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            config: ClientConfig::default(),
            headers: vec![],
            timeout_secs: None,
        }
    }

    /// Sets the client configuration.
    #[must_use] 
    pub fn config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    /// Adds a custom header.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Sets the Bearer authentication token.
    pub fn bearer_auth(self, token: impl Into<String>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.into()))
    }

    /// Sets the API key.
    pub fn api_key(self, header_name: impl Into<String>, key: impl Into<String>) -> Self {
        self.header(header_name, key)
    }

    /// Sets the request timeout.
    #[must_use] 
    pub const fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Enables or disables streaming.
    #[must_use] 
    pub const fn streaming(mut self, enabled: bool) -> Self {
        self.config.streaming = enabled;
        self
    }

    /// Builds the A2A client.
    pub fn build(self) -> Result<A2AClient> {
        let mut headers = HeaderMap::new();
        for (name, value) in &self.headers {
            if let (Ok(name), Ok(value)) = (
                reqwest::header::HeaderName::try_from(name.as_str()),
                HeaderValue::from_str(value),
            ) {
                headers.insert(name, value);
            }
        }

        let timeout = self.timeout_secs.unwrap_or(self.config.timeout_secs);

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout))
            .default_headers(headers)
            .build()
            .map_err(|e| A2AError::Other(e.to_string()))?;

        let card_url = format!(
            "{}/.well-known/agent-card.json",
            self.base_url.trim_end_matches('/')
        );

        Ok(A2AClient {
            http_client,
            base_url: self.base_url,
            card_url,
            config: self.config,
            agent_card: None,
        })
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
            .build()
            .unwrap();

        assert_eq!(client.base_url(), "https://agent.example.com");
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
}
