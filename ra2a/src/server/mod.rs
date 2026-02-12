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
mod default_handler;
mod events;
mod handler;
mod task_store;

use std::sync::Arc;

pub use app::*;
use async_trait::async_trait;
pub use default_handler::*;
pub use events::*;
pub use handler::*;
pub use task_store::*;

use crate::error::Result;
use crate::types::{AgentCard, Message, Task};

/// Trait for implementing agent execution logic.
///
/// Aligned with Go's `AgentExecutor` interface in `agentexec.go`.
/// The agent translates its outputs to A2A events and writes them to the
/// provided [`EventQueue`]. The server stops processing after:
/// - A [`Message`] event with any payload
/// - A [`TaskStatusUpdateEvent`](crate::types::TaskStatusUpdateEvent) with `final = true`
/// - A [`Task`] with a terminal [`TaskState`](crate::types::TaskState)
///
/// # Example (streaming)
/// ```ignore
/// async fn execute(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()> {
///     // Emit "working" status
///     queue.send(Event::status_update(TaskStatusUpdateEvent::working(&ctx.task_id)))?;
///     // ... do work, emit artifact updates ...
///     // Emit final "completed" status
///     let mut evt = TaskStatusUpdateEvent::completed(&ctx.task_id, response_message);
///     evt.r#final = true;
///     queue.send(Event::status_update(evt))?;
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Executes the agent for an incoming message.
    ///
    /// The triggering message is available via [`RequestContext::message`].
    /// Write A2A events to `queue`; the server consumes them for streaming
    /// and persistence.
    async fn execute(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()>;

    /// Cancels an ongoing task.
    ///
    /// Write a cancellation event to `queue` (e.g. a [`Task`] with
    /// [`TaskState::Canceled`](crate::types::TaskState::Canceled)).
    async fn cancel(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()>;

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
    /// The message that triggered this execution. `None` for cancel requests.
    pub message: Option<Message>,
    /// The existing task if the message references one.
    pub stored_task: Option<Task>,
    /// Tasks referenced by `Message.reference_task_ids`, loaded by interceptors.
    pub related_tasks: Vec<Task>,
    /// Additional metadata from the request.
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl RequestContext {
    /// Creates a new request context.
    pub fn new(task_id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            context_id: context_id.into(),
            message: None,
            stored_task: None,
            related_tasks: Vec::new(),
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

/// Server state shared across all request handlers.
///
/// Holds an [`AgentCard`] for the well-known endpoint and a boxed
/// [`RequestHandler`] that all JSON-RPC methods are dispatched to.
#[derive(Clone)]
pub struct ServerState {
    /// The request handler all methods are dispatched to.
    pub handler: Arc<dyn RequestHandler>,
    /// The agent card served on the well-known endpoint.
    pub agent_card: Arc<AgentCard>,
}

impl ServerState {
    /// Creates a server state from an existing handler and agent card.
    pub fn new(handler: Arc<dyn RequestHandler>, agent_card: AgentCard) -> Self {
        Self {
            handler,
            agent_card: Arc::new(agent_card),
        }
    }

    /// Convenience constructor: wraps an [`AgentExecutor`] in a
    /// [`DefaultRequestHandler`] automatically.
    pub fn from_executor<E: AgentExecutor + 'static>(executor: E) -> Self {
        let card = executor.agent_card().clone();
        let handler = Arc::new(DefaultRequestHandler::new(executor));
        Self {
            handler,
            agent_card: Arc::new(card),
        }
    }
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerState")
            .field("agent_card", &self.agent_card)
            .finish_non_exhaustive()
    }
}
