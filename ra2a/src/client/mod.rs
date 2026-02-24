//! A2A client module.
//!
//! - [`Transport`] — transport-agnostic interface for all A2A protocol operations
//! - [`Client`] — concrete client wrapping a [`Transport`] with [`CallInterceptor`] support
//! - [`JsonRpcTransport`] — default HTTP/JSON-RPC + SSE transport

mod interceptor;
mod jsonrpc;

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
pub use interceptor::{CallContext, CallInterceptor, CallMeta, Request, Response};
pub use jsonrpc::{JsonRpcTransport, TransportConfig};

use crate::error::Result;
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, Event, GetTaskPushConfigParams,
    ListTaskPushConfigParams, ListTasksRequest, ListTasksResponse, MessageSendParams,
    SendMessageResult, Task, TaskIdParams, TaskPushConfig, TaskQueryParams,
};

/// A boxed stream of protocol [`Event`]s for streaming responses.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event>> + Send>>;

/// Transport-agnostic interface for all A2A protocol operations.
///
/// Each method corresponds to a single A2A protocol method. The transport
/// handles serialization, HTTP/gRPC details, and SSE parsing internally.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Sends a message (non-streaming). Corresponds to `message/send`.
    async fn send_message(&self, params: &MessageSendParams) -> Result<SendMessageResult>;

    /// Sends a message with streaming response. Corresponds to `message/stream`.
    async fn send_message_stream(&self, params: &MessageSendParams) -> Result<EventStream>;

    /// Retrieves a task. Corresponds to `tasks/get`.
    async fn get_task(&self, params: &TaskQueryParams) -> Result<Task>;

    /// Lists tasks. Corresponds to `tasks/list`.
    async fn list_tasks(&self, params: &ListTasksRequest) -> Result<ListTasksResponse>;

    /// Cancels a task. Corresponds to `tasks/cancel`.
    async fn cancel_task(&self, params: &TaskIdParams) -> Result<Task>;

    /// Resubscribes to a task's event stream. Corresponds to `tasks/resubscribe`.
    async fn resubscribe(&self, params: &TaskIdParams) -> Result<EventStream>;

    /// Sets push notification config for a task.
    async fn set_task_push_config(&self, params: &TaskPushConfig) -> Result<TaskPushConfig>;

    /// Gets push notification config for a task.
    async fn get_task_push_config(
        &self,
        params: &GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig>;

    /// Lists push notification configs for a task.
    async fn list_task_push_config(
        &self,
        params: &ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>>;

    /// Deletes push notification config for a task.
    async fn delete_task_push_config(&self, params: &DeleteTaskPushConfigParams) -> Result<()>;

    /// Retrieves the agent card.
    async fn get_agent_card(&self) -> Result<AgentCard>;

    /// Releases any resources held by the transport.
    async fn destroy(&self) {}
}

/// A2A protocol client.
///
/// Wraps a [`Transport`] and applies [`CallInterceptor`]s before/after each
/// call. This is a concrete struct — not a trait — matching Go's `Client`.
///
/// # Example
///
/// ```no_run
/// use ra2a::client::{Client, JsonRpcTransport};
/// use ra2a::types::{Message, MessageSendParams, Part};
///
/// # async fn example() -> ra2a::error::Result<()> {
/// let client = Client::from_url("https://agent.example.com")?;
/// let msg = Message::user(vec![Part::text("Hello")]);
/// let result = client.send_message(&MessageSendParams::new(msg)).await?;
/// # Ok(())
/// # }
/// ```
pub struct Client {
    transport: Box<dyn Transport>,
    interceptors: Vec<Box<dyn CallInterceptor>>,
    card: std::sync::RwLock<Option<AgentCard>>,
}

impl Client {
    /// Creates a new client wrapping the given transport.
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self {
            transport,
            interceptors: Vec::new(),
            card: std::sync::RwLock::new(None),
        }
    }

    /// Creates a client from a base URL using [`JsonRpcTransport`].
    pub fn from_url(base_url: impl Into<String>) -> Result<Self> {
        let transport = JsonRpcTransport::from_url(base_url)?;
        Ok(Self::new(Box::new(transport)))
    }

    /// Adds a call interceptor.
    pub fn with_interceptor(mut self, interceptor: impl CallInterceptor + 'static) -> Self {
        self.interceptors.push(Box::new(interceptor));
        self
    }

    /// Caches an agent card for capability checks.
    pub fn set_card(&self, card: AgentCard) {
        *self.card.write().unwrap() = Some(card);
    }

    /// Returns the cached agent card, if any.
    pub fn card(&self) -> Option<AgentCard> {
        self.card.read().unwrap().clone()
    }

    // ----- A2A protocol methods (delegate to transport) ---------------------

    /// Sends a message (non-streaming). Corresponds to `message/send`.
    pub async fn send_message(&self, params: &MessageSendParams) -> Result<SendMessageResult> {
        self.transport.send_message(params).await
    }

    /// Sends a message with streaming response. Corresponds to `message/stream`.
    pub async fn send_message_stream(&self, params: &MessageSendParams) -> Result<EventStream> {
        self.transport.send_message_stream(params).await
    }

    /// Retrieves a task. Corresponds to `tasks/get`.
    pub async fn get_task(&self, params: &TaskQueryParams) -> Result<Task> {
        self.transport.get_task(params).await
    }

    /// Lists tasks. Corresponds to `tasks/list`.
    pub async fn list_tasks(&self, params: &ListTasksRequest) -> Result<ListTasksResponse> {
        self.transport.list_tasks(params).await
    }

    /// Cancels a task. Corresponds to `tasks/cancel`.
    pub async fn cancel_task(&self, params: &TaskIdParams) -> Result<Task> {
        self.transport.cancel_task(params).await
    }

    /// Resubscribes to a task's event stream. Corresponds to `tasks/resubscribe`.
    pub async fn resubscribe(&self, params: &TaskIdParams) -> Result<EventStream> {
        self.transport.resubscribe(params).await
    }

    /// Sets push notification config. Corresponds to `tasks/pushNotificationConfig/set`.
    pub async fn set_task_push_config(&self, params: &TaskPushConfig) -> Result<TaskPushConfig> {
        self.transport.set_task_push_config(params).await
    }

    /// Gets push notification config. Corresponds to `tasks/pushNotificationConfig/get`.
    pub async fn get_task_push_config(
        &self,
        params: &GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig> {
        self.transport.get_task_push_config(params).await
    }

    /// Lists push notification configs. Corresponds to `tasks/pushNotificationConfig/list`.
    pub async fn list_task_push_config(
        &self,
        params: &ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        self.transport.list_task_push_config(params).await
    }

    /// Deletes push notification config. Corresponds to `tasks/pushNotificationConfig/delete`.
    pub async fn delete_task_push_config(&self, params: &DeleteTaskPushConfigParams) -> Result<()> {
        self.transport.delete_task_push_config(params).await
    }

    /// Retrieves the agent card from the server.
    ///
    /// The card is also cached internally for future capability checks.
    pub async fn get_agent_card(&self) -> Result<AgentCard> {
        let card = self.transport.get_agent_card().await?;
        self.set_card(card.clone());
        Ok(card)
    }

    /// Releases transport resources.
    pub async fn destroy(&self) {
        self.transport.destroy().await;
    }
}
