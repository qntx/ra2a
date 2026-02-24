//! A2A Client implementation.
//!
//! This module provides the client-side implementation for interacting with A2A agents.
//!
//! # Features
//!
//! - **HTTP Client**: Basic HTTP/JSON-RPC client for synchronous requests
//! - **SSE Streaming**: Server-Sent Events support for real-time updates
//! - **Middleware**: Request/response interceptors for authentication and logging

mod auth;
mod card_resolver;
mod config;
mod factory;
mod http;
mod intercepted_client;
mod middleware;
mod sse;
pub mod transports;

use std::pin::Pin;

use async_trait::async_trait;
pub use auth::{
    ApiKeyCredential, BasicAuthCredential, BearerTokenCredential, CredentialProvider, NoCredential,
};
pub use card_resolver::A2ACardResolver;
pub use config::ClientConfig;
pub use factory::ClientFactory;
use futures::Stream;
pub use http::{A2AClient, A2AClientBuilder};
pub use intercepted_client::InterceptedClient;
pub use middleware::{
    AuthInterceptor, CallMeta, ClientCallInterceptor, ClientRequest, ClientResponse,
    ExtensionActivator, LogLevel, LoggingInterceptor, PassthroughClientInterceptor,
    StaticCallMetaInjector, run_interceptors_after, run_interceptors_before,
};
pub use sse::{A2ASseEvent, SseEventType, parse_sse_line};
pub use transports::{
    ClientTransport, EventStream as TransportEventStream, JsonRpcTransport, RestTransport,
    StreamEvent, TransportOptions, TransportType,
};

use crate::error::Result;
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, GetTaskPushConfigParams, ListTaskPushConfigParams,
    Message, Task, TaskArtifactUpdateEvent, TaskIdParams, TaskPushConfig, TaskQueryParams,
    TaskStatusUpdateEvent,
};

/// Update event from streaming responses.
#[derive(Debug, Clone)]
pub enum UpdateEvent {
    /// A status update event.
    Status(TaskStatusUpdateEvent),
    /// An artifact update event.
    Artifact(TaskArtifactUpdateEvent),
}

/// Event emitted by the client during message processing.
#[derive(Debug, Clone)]
pub enum ClientEvent {
    /// A task with optional update event.
    TaskUpdate {
        /// The current task state.
        task: Box<Task>,
        /// Optional update event.
        update: Option<UpdateEvent>,
    },
    /// A direct message response.
    Message(Message),
}

/// A boxed stream of client events.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<ClientEvent>> + Send>>;

/// Abstract interface for an A2A client.
///
/// This trait defines the standard set of methods for interacting with an A2A agent,
/// regardless of the underlying transport protocol. Aligned with Go's `Client` struct.
///
/// Streaming responses are returned as [`EventStream`] — use standard [`Stream`]
/// combinators (`map`, `for_each`, `collect`, etc.) to process events.
#[async_trait]
pub trait Client: Send + Sync {
    /// Sends a message to the agent.
    ///
    /// Returns a stream of events that may include task updates or direct messages.
    async fn send_message(&self, message: Message) -> Result<EventStream>;

    /// Retrieves the current state and history of a specific task.
    async fn get_task(&self, params: TaskQueryParams) -> Result<Task>;

    /// Requests the agent to cancel a specific task.
    async fn cancel_task(&self, params: TaskIdParams) -> Result<Task>;

    /// Sets or updates the push notification configuration for a task.
    async fn set_task_callback(&self, config: TaskPushConfig) -> Result<TaskPushConfig>;

    /// Retrieves the push notification configuration for a task.
    async fn get_task_callback(&self, params: GetTaskPushConfigParams) -> Result<TaskPushConfig>;

    /// Resubscribes to a task's event stream.
    async fn resubscribe(&self, params: TaskIdParams) -> Result<EventStream>;

    /// Lists all push notification configurations for a task.
    async fn list_task_push_notification_config(
        &self,
        params: ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>>;

    /// Deletes a push notification configuration for a task.
    async fn delete_task_push_notification_config(
        &self,
        params: DeleteTaskPushConfigParams,
    ) -> Result<()>;

    /// Retrieves the agent's card.
    async fn get_agent_card(&self) -> Result<AgentCard>;
}
