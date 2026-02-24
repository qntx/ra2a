//! Transport trait — transport-agnostic interface for A2A protocol operations.
//!
//! Aligned with Go's `a2aclient.Transport` interface. Concrete implementations
//! (e.g. [`super::JsonRpcTransport`]) handle the wire format; [`super::Client`]
//! wraps a transport and applies interceptors.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::error::Result;
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, Event, GetTaskPushConfigParams, ListTaskPushConfigParams,
    ListTasksRequest, ListTasksResponse, MessageSendParams, SendMessageResult, Task, TaskIdParams,
    TaskPushConfig, TaskQueryParams,
};

/// A boxed stream of protocol [`Event`]s for streaming responses.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event>> + Send>>;

/// Transport-agnostic interface for all A2A protocol operations.
///
/// Each method corresponds to a single A2A protocol method. The transport
/// handles serialization, HTTP/gRPC details, and SSE parsing internally.
///
/// Aligned with Go's `Transport` interface in `a2aclient/transport.go`.
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
