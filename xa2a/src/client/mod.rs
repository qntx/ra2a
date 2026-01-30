//! A2A Client implementation.
//!
//! This module provides the client-side implementation for interacting with A2A agents.
//!
//! # Features
//!
//! - **HTTP Client**: Basic HTTP/JSON-RPC client for synchronous requests
//! - **SSE Streaming**: Server-Sent Events support for real-time updates
//! - **Middleware**: Request/response interceptors for authentication and logging
//!
//! # Example
//!
//! ```rust,no_run
//! use xa2a::client::{A2AClient, Client, StreamingClient};
//! use xa2a::types::{Message, Part};
//!
//! #[tokio::main]
//! async fn main() -> xa2a::Result<()> {
//!     // Create a streaming client
//!     let client = StreamingClient::new("https://agent.example.com")?;
//!     
//!     // Send a message and receive streaming updates
//!     let message = Message::user_text("Hello, agent!");
//!     let mut stream = client.send_message_streaming(message).await?;
//!     
//!     while let Some(event) = futures::StreamExt::next(&mut stream).await {
//!         println!("Received: {:?}", event?);
//!     }
//!     
//!     Ok(())
//! }
//! ```

mod card_resolver;
mod config;
mod http;
mod middleware;
mod sse;
mod streaming;
mod transport;

pub use card_resolver::*;
pub use config::*;
pub use http::*;
pub use middleware::*;
pub use sse::*;
pub use streaming::*;
pub use transport::*;

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::error::Result;
use crate::types::{
    AgentCard, GetTaskPushNotificationConfigParams, Message, Task, TaskArtifactUpdateEvent,
    TaskIdParams, TaskPushNotificationConfig, TaskQueryParams, TaskStatusUpdateEvent,
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
    async fn set_task_callback(
        &self,
        config: TaskPushNotificationConfig,
    ) -> Result<TaskPushNotificationConfig>;

    /// Retrieves the push notification configuration for a task.
    async fn get_task_callback(
        &self,
        params: GetTaskPushNotificationConfigParams,
    ) -> Result<TaskPushNotificationConfig>;

    /// Resubscribes to a task's event stream.
    async fn resubscribe(&self, params: TaskIdParams) -> Result<EventStream>;

    /// Retrieves the agent's card.
    async fn get_agent_card(&self) -> Result<AgentCard>;
}
