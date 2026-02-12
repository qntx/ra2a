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
mod middleware;
mod sse;
mod streaming;
pub mod transports;

use std::pin::Pin;

use async_trait::async_trait;
pub use auth::*;
pub use card_resolver::*;
pub use config::*;
pub use factory::*;
use futures::Stream;
pub use http::*;
pub use middleware::*;
pub use sse::*;
pub use streaming::*;
pub use transports::{
    ClientTransport, EventStream as TransportEventStream, JsonRpcTransport, RestTransport,
    SendMessageResponse, StreamEvent, TransportOptions, TransportType,
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
        task: Task,
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
/// regardless of the underlying transport protocol.
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

/// Trait for consuming and processing client events.
///
/// Implement this trait to create custom event handlers that process streaming
/// responses from A2A agents. Similar to Python's `Consumer` class.
#[async_trait]
pub trait Consumer: Send + Sync {
    /// Called for each event received from the stream.
    async fn consume(&self, event: &ClientEvent) -> Result<()>;

    /// Called when the stream completes successfully.
    async fn on_complete(&self) -> Result<()> {
        Ok(())
    }

    /// Called when an error occurs during streaming.
    async fn on_error(&self, error: &crate::error::A2AError) -> Result<()> {
        tracing::error!(error = %error, "Consumer error");
        Ok(())
    }
}

/// A simple consumer that collects events into a vector.
#[derive(Debug, Default)]
pub struct CollectingConsumer {
    events: std::sync::Arc<tokio::sync::RwLock<Vec<ClientEvent>>>,
}

impl CollectingConsumer {
    /// Creates a new collecting consumer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns all collected events.
    pub async fn events(&self) -> Vec<ClientEvent> {
        self.events.read().await.clone()
    }

    /// Returns the final task if one was received.
    pub async fn final_task(&self) -> Option<Task> {
        let events = self.events.read().await;
        events.iter().rev().find_map(|e| match e {
            ClientEvent::TaskUpdate { task, .. } => Some(task.clone()),
            _ => None,
        })
    }
}

#[async_trait]
impl Consumer for CollectingConsumer {
    async fn consume(&self, event: &ClientEvent) -> Result<()> {
        self.events.write().await.push(event.clone());
        Ok(())
    }
}

/// A consumer that calls a callback function for each event.
pub struct CallbackConsumer<F>
where
    F: Fn(&ClientEvent) -> Result<()> + Send + Sync,
{
    callback: F,
}

impl<F> CallbackConsumer<F>
where
    F: Fn(&ClientEvent) -> Result<()> + Send + Sync,
{
    /// Creates a new callback consumer.
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

#[async_trait]
impl<F> Consumer for CallbackConsumer<F>
where
    F: Fn(&ClientEvent) -> Result<()> + Send + Sync + 'static,
{
    async fn consume(&self, event: &ClientEvent) -> Result<()> {
        (self.callback)(event)
    }
}

/// Runs a consumer on an event stream until completion or error.
pub async fn run_consumer<C, S>(consumer: &C, mut stream: S) -> Result<()>
where
    C: Consumer,
    S: Stream<Item = Result<ClientEvent>> + Unpin,
{
    use futures::StreamExt;

    while let Some(result) = stream.next().await {
        match result {
            Ok(event) => {
                if let Err(e) = consumer.consume(&event).await {
                    consumer.on_error(&e).await?;
                }
            }
            Err(e) => {
                consumer.on_error(&e).await?;
            }
        }
    }
    consumer.on_complete().await
}

/// Helper to send a message and process all events with a consumer.
pub async fn send_and_consume<C: Client, Co: Consumer>(
    client: &C,
    message: Message,
    consumer: &Co,
) -> Result<()> {
    let stream = client.send_message(message).await?;
    run_consumer(consumer, stream).await
}
