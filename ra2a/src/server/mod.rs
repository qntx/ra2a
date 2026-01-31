//! A2A Server implementation.
//!
//! This module provides the server-side implementation for building A2A agents.
//!
//! # Features
//!
//! - **HTTP Server**: Axum-based HTTP server with JSON-RPC endpoint
//! - **SSE Streaming**: Server-Sent Events for real-time task updates
//! - **Event Queue**: Broadcast-based event distribution to subscribers
//! - **Task Management**: Task lifecycle and state management
//!
//! # Example
//!
//! ```rust,no_run
//! use ra2a::server::{A2AServer, AgentExecutor, ExecutionContext, ServerConfig};
//! use ra2a::types::{AgentCard, Message, Task};
//! use ra2a::error::Result;
//! use async_trait::async_trait;
//!
//! struct MyAgent {
//!     card: AgentCard,
//! }
//!
//! #[async_trait]
//! impl AgentExecutor for MyAgent {
//!     async fn execute(&self, ctx: &ExecutionContext, message: &Message) -> Result<Task> {
//!         // Process the message and return updated task
//!         Ok(Task::new(&ctx.task_id, &ctx.context_id))
//!     }
//!
//!     async fn cancel(&self, ctx: &ExecutionContext, task_id: &str) -> Result<Task> {
//!         Ok(Task::new(task_id, &ctx.context_id))
//!     }
//!
//!     fn agent_card(&self) -> &AgentCard {
//!         &self.card
//!     }
//! }
//! ```

mod app;
mod call_context;
mod context;
mod context_builder;
mod default_handler;
mod events;
mod handler;
mod id_generator;
mod push_notification;
mod request_handler;
mod rest_handler;
mod result_aggregator;
mod sse;
mod streaming_handler;
mod task_manager;
mod task_store;
mod task_updater;

pub use app::*;
pub use call_context::*;
pub use context::*;
pub use context_builder::*;
pub use default_handler::*;
pub use events::*;
pub use handler::*;
pub use id_generator::*;
pub use push_notification::*;
pub use request_handler::*;
pub use rest_handler::*;
pub use result_aggregator::*;
pub use sse::*;
pub use streaming_handler::*;
pub use task_manager::*;
pub use task_store::*;
pub use task_updater::*;

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::error::Result;
use crate::types::{AgentCard, Message, Task};

/// A boxed stream of events for streaming execution.
pub type ExecutionEventStream = Pin<Box<dyn Stream<Item = Result<Event>> + Send>>;

/// Trait for implementing agent execution logic.
///
/// Implement this trait to define how your agent processes messages and generates responses.
/// For streaming support, also implement the `execute_streaming` method.
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Processes an incoming message and returns the updated task.
    ///
    /// This method is called when a new message is received from a client.
    /// The implementation should process the message and update the task status accordingly.
    async fn execute(&self, ctx: &ExecutionContext, message: &Message) -> Result<Task>;

    /// Processes an incoming message and returns a stream of events.
    ///
    /// This method is called for streaming requests (`message/stream`).
    /// The default implementation wraps the non-streaming `execute` method.
    ///
    /// Override this method to provide true streaming support with incremental updates.
    async fn execute_streaming(
        &self,
        ctx: &ExecutionContext,
        message: &Message,
    ) -> Result<ExecutionEventStream> {
        let task = self.execute(ctx, message).await?;
        let stream = futures::stream::once(async move { Ok(Event::Task(task)) });
        Ok(Box::pin(stream))
    }

    /// Cancels an ongoing task.
    ///
    /// This method is called when a client requests to cancel a task.
    /// The implementation should stop any ongoing processing and update the task status.
    async fn cancel(&self, ctx: &ExecutionContext, task_id: &str) -> Result<Task>;

    /// Returns the agent card describing this agent's capabilities.
    fn agent_card(&self) -> &AgentCard;

    /// Returns true if this executor supports streaming.
    ///
    /// Override this to return `true` if you implement `execute_streaming`.
    fn supports_streaming(&self) -> bool {
        self.agent_card().supports_streaming()
    }
}

/// Context passed to the executor during message processing.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// The task ID being processed.
    pub task_id: String,
    /// The context ID for maintaining session state.
    pub context_id: String,
    /// Additional metadata from the request.
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl ExecutionContext {
    /// Creates a new execution context.
    pub fn new(task_id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            context_id: context_id.into(),
            metadata: None,
        }
    }

    /// Creates a new execution context with auto-generated IDs.
    pub fn create() -> Self {
        Self::new(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        )
    }
}
