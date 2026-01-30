//! Request handler trait and default implementation for A2A server.
//!
//! This module provides the `RequestHandler` trait that defines the interface
//! for handling all A2A JSON-RPC methods, along with a default implementation.

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::error::Result;
use crate::types::{
    DeleteTaskPushNotificationConfigParams, GetTaskPushNotificationConfigParams,
    ListTaskPushNotificationConfigParams, Message, MessageSendParams, Task, TaskIdParams,
    TaskPushNotificationConfig, TaskQueryParams,
};

use super::call_context::ServerCallContext;
use super::events::Event;

/// A boxed stream of events for streaming responses.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event>> + Send>>;

/// Trait defining the interface for handling A2A JSON-RPC requests.
///
/// Implement this trait to customize how your server handles incoming requests.
/// The `DefaultRequestHandler` provides a standard implementation that coordinates
/// between the `AgentExecutor`, `TaskStore`, and `QueueManager`.
#[async_trait]
pub trait RequestHandler: Send + Sync {
    /// Handles the `message/send` request (non-streaming).
    ///
    /// Processes an incoming message and returns either a Task or Message response.
    async fn on_message_send(
        &self,
        params: MessageSendParams,
        context: Option<&ServerCallContext>,
    ) -> Result<SendMessageResponse>;

    /// Handles the `message/stream` request (streaming).
    ///
    /// Processes an incoming message and returns a stream of events.
    async fn on_message_stream(
        &self,
        params: MessageSendParams,
        context: Option<&ServerCallContext>,
    ) -> Result<EventStream>;

    /// Handles the `tasks/get` request.
    ///
    /// Retrieves a task by its ID.
    async fn on_get_task(
        &self,
        params: TaskQueryParams,
        context: Option<&ServerCallContext>,
    ) -> Result<Task>;

    /// Handles the `tasks/cancel` request.
    ///
    /// Cancels an active task.
    async fn on_cancel_task(
        &self,
        params: TaskIdParams,
        context: Option<&ServerCallContext>,
    ) -> Result<Task>;

    /// Handles the `tasks/resubscribe` request.
    ///
    /// Resubscribes to an existing task's event stream.
    async fn on_resubscribe(
        &self,
        params: TaskIdParams,
        context: Option<&ServerCallContext>,
    ) -> Result<EventStream>;

    /// Handles the `tasks/pushNotificationConfig/set` request.
    async fn on_set_push_notification_config(
        &self,
        params: TaskPushNotificationConfig,
        context: Option<&ServerCallContext>,
    ) -> Result<TaskPushNotificationConfig>;

    /// Handles the `tasks/pushNotificationConfig/get` request.
    async fn on_get_push_notification_config(
        &self,
        params: GetTaskPushNotificationConfigParams,
        context: Option<&ServerCallContext>,
    ) -> Result<TaskPushNotificationConfig>;

    /// Handles the `tasks/pushNotificationConfig/list` request.
    async fn on_list_push_notification_config(
        &self,
        params: ListTaskPushNotificationConfigParams,
        context: Option<&ServerCallContext>,
    ) -> Result<Vec<TaskPushNotificationConfig>>;

    /// Handles the `tasks/pushNotificationConfig/delete` request.
    async fn on_delete_push_notification_config(
        &self,
        params: DeleteTaskPushNotificationConfigParams,
        context: Option<&ServerCallContext>,
    ) -> Result<()>;
}

/// Response type for message/send operations.
#[derive(Debug, Clone)]
pub enum SendMessageResponse {
    /// A task was created or updated.
    Task(Task),
    /// A direct message response (no task created).
    Message(Message),
}

impl From<Task> for SendMessageResponse {
    fn from(task: Task) -> Self {
        Self::Task(task)
    }
}

impl From<Message> for SendMessageResponse {
    fn from(message: Message) -> Self {
        Self::Message(message)
    }
}
