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
//! use xa2a::server::{A2AServer, AgentExecutor, ExecutionContext, ServerConfig};
//! use xa2a::types::{AgentCard, Message, Task};
//! use xa2a::error::Result;
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
mod context;
mod events;
mod handler;
mod push_notification;
mod sse;
mod streaming_handler;
mod task_manager;
mod task_store;

pub use app::*;
pub use context::*;
pub use events::*;
pub use handler::*;
pub use push_notification::*;
pub use sse::*;
pub use streaming_handler::*;
pub use task_manager::*;
pub use task_store::*;

use async_trait::async_trait;

use crate::error::Result;
use crate::types::{AgentCard, Message, Task};

/// Trait for implementing agent execution logic.
///
/// Implement this trait to define how your agent processes messages and generates responses.
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Processes an incoming message and returns the updated task.
    ///
    /// This method is called when a new message is received from a client.
    /// The implementation should process the message and update the task status accordingly.
    async fn execute(&self, ctx: &ExecutionContext, message: &Message) -> Result<Task>;

    /// Cancels an ongoing task.
    ///
    /// This method is called when a client requests to cancel a task.
    /// The implementation should stop any ongoing processing and update the task status.
    async fn cancel(&self, ctx: &ExecutionContext, task_id: &str) -> Result<Task>;

    /// Returns the agent card describing this agent's capabilities.
    fn agent_card(&self) -> &AgentCard;
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
