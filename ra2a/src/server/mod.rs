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

mod app;
mod context;
mod default_handler;
mod events;
mod handler;
mod push_notification;
mod rest_handler;
mod sse;
mod task_store;

pub use app::*;
pub use context::*;
pub use default_handler::*;
pub use events::*;
pub use handler::*;
pub use push_notification::*;
pub use rest_handler::*;
pub use sse::*;
pub use task_store::*;

use async_trait::async_trait;

use crate::error::Result;
use crate::types::{AgentCard, Message, Task};

/// Trait for implementing agent execution logic.
///
/// Aligned with Go's `AgentExecutor` interface in `agentexec.go`.
/// Implement this trait to define how your agent processes messages.
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Processes an incoming message and returns the updated task.
    async fn execute(&self, ctx: &RequestContext, message: &Message) -> Result<Task>;

    /// Cancels an ongoing task.
    async fn cancel(&self, ctx: &RequestContext, task_id: &str) -> Result<Task>;

    /// Returns the agent card describing this agent's capabilities.
    fn agent_card(&self) -> &AgentCard;
}

/// Request context passed to the executor during message processing.
///
/// Aligned with Go's `RequestContext` in `reqctx.go`. Carries task identity,
/// the triggering message, and any previously stored task.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// The task ID being processed (or newly generated).
    pub task_id: String,
    /// The context ID for maintaining session state.
    pub context_id: String,
    /// The existing task if the message references one.
    pub stored_task: Option<Task>,
    /// Additional metadata from the request.
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl RequestContext {
    /// Creates a new request context.
    pub fn new(task_id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            context_id: context_id.into(),
            stored_task: None,
            metadata: None,
        }
    }

    /// Creates a new request context with auto-generated IDs.
    pub fn create() -> Self {
        Self::new(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        )
    }
}
