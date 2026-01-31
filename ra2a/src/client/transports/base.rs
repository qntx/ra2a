//! Base transport trait and common types.
//!
//! Defines the abstract interface that all transport implementations must follow.

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::error::Result;
use crate::types::{
    AgentCard, DeleteTaskPushNotificationConfigParams, GetTaskPushNotificationConfigParams,
    ListTaskPushNotificationConfigParams, Message, Task, TaskIdParams, TaskPushNotificationConfig,
    TaskQueryParams, TaskResubscriptionParams,
};

/// A boxed stream of streaming events.
pub type EventStream<T> = Pin<Box<dyn Stream<Item = Result<T>> + Send>>;

/// Supported transport protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportType {
    /// JSON-RPC over HTTP/HTTPS.
    JsonRpc,
    /// REST API (HTTP+JSON).
    Rest,
    /// gRPC transport.
    Grpc,
}

impl Default for TransportType {
    fn default() -> Self {
        Self::JsonRpc
    }
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JsonRpc => write!(f, "JSONRPC"),
            Self::Rest => write!(f, "HTTP+JSON"),
            Self::Grpc => write!(f, "GRPC"),
        }
    }
}

impl std::str::FromStr for TransportType {
    type Err = crate::error::A2AError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_uppercase().as_str() {
            "JSONRPC" | "JSON-RPC" => Ok(Self::JsonRpc),
            "HTTP+JSON" | "REST" => Ok(Self::Rest),
            "GRPC" => Ok(Self::Grpc),
            _ => Err(crate::error::A2AError::InvalidConfig(format!(
                "Unknown transport type: {}",
                s
            ))),
        }
    }
}

/// Result of a send message operation.
#[derive(Debug, Clone)]
pub enum SendMessageResponse {
    /// A task was created or updated.
    Task(Task),
    /// A direct message response.
    Message(Message),
}

/// Streaming event from a message/stream operation.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A task update.
    Task(Task),
    /// A message response.
    Message(Message),
    /// A status update event.
    StatusUpdate(crate::types::TaskStatusUpdateEvent),
    /// An artifact update event.
    ArtifactUpdate(crate::types::TaskArtifactUpdateEvent),
}

/// Abstract transport interface for A2A client communication.
///
/// All transport implementations (JSON-RPC, REST, gRPC) must implement this trait.
#[async_trait]
pub trait ClientTransport: Send + Sync {
    /// Returns the transport type.
    fn transport_type(&self) -> TransportType;

    /// Sends a message to the agent (non-streaming).
    async fn send_message(&self, message: Message) -> Result<SendMessageResponse>;

    /// Sends a message to the agent with streaming response.
    async fn send_message_streaming(&self, message: Message) -> Result<EventStream<StreamEvent>>;

    /// Gets a task by its parameters.
    async fn get_task(&self, params: TaskQueryParams) -> Result<Task>;

    /// Cancels a task.
    async fn cancel_task(&self, params: TaskIdParams) -> Result<Task>;

    /// Sets or updates push notification configuration for a task.
    async fn set_task_push_notification_config(
        &self,
        config: TaskPushNotificationConfig,
    ) -> Result<TaskPushNotificationConfig>;

    /// Gets push notification configuration for a task.
    async fn get_task_push_notification_config(
        &self,
        params: GetTaskPushNotificationConfigParams,
    ) -> Result<TaskPushNotificationConfig>;

    /// Lists all push notification configurations for a task.
    async fn list_task_push_notification_configs(
        &self,
        params: ListTaskPushNotificationConfigParams,
    ) -> Result<Vec<TaskPushNotificationConfig>>;

    /// Deletes a push notification configuration.
    async fn delete_task_push_notification_config(
        &self,
        params: DeleteTaskPushNotificationConfigParams,
    ) -> Result<()>;

    /// Resubscribes to a task's event stream.
    async fn resubscribe(
        &self,
        params: TaskResubscriptionParams,
    ) -> Result<EventStream<StreamEvent>>;

    /// Gets the agent card.
    async fn get_agent_card(&self) -> Result<AgentCard>;
}

/// Configuration for creating a transport.
#[derive(Debug, Clone)]
pub struct TransportOptions {
    /// Base URL of the agent.
    pub base_url: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Custom headers to include in requests.
    pub headers: Vec<(String, String)>,
    /// Whether to verify TLS certificates.
    pub verify_tls: bool,
}

impl TransportOptions {
    /// Creates new transport options with default values.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout_secs: 30,
            headers: Vec::new(),
            verify_tls: true,
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

    /// Adds a Bearer token authorization header.
    pub fn bearer_auth(self, token: impl Into<String>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.into()))
    }

    /// Adds an API key header.
    pub fn api_key(self, header_name: impl Into<String>, key: impl Into<String>) -> Self {
        self.header(header_name, key)
    }

    /// Disables TLS verification (not recommended for production).
    pub fn danger_accept_invalid_certs(mut self) -> Self {
        self.verify_tls = false;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_type_from_str() {
        assert_eq!(
            "JSONRPC".parse::<TransportType>().unwrap(),
            TransportType::JsonRpc
        );
        assert_eq!(
            "HTTP+JSON".parse::<TransportType>().unwrap(),
            TransportType::Rest
        );
        assert_eq!(
            "GRPC".parse::<TransportType>().unwrap(),
            TransportType::Grpc
        );
    }

    #[test]
    fn test_transport_options_builder() {
        let opts = TransportOptions::new("https://example.com")
            .timeout(60)
            .bearer_auth("token123");

        assert_eq!(opts.base_url, "https://example.com");
        assert_eq!(opts.timeout_secs, 60);
        assert_eq!(opts.headers.len(), 1);
    }
}
