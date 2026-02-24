//! Agent execution traits and request context.
//!
//! - [`AgentExecutor`] — trait for implementing agent business logic
//! - [`RequestContext`] — context passed to the executor during processing
//! - [`RequestContextInterceptor`] — extension point for modifying context pre-execution

use std::sync::Arc;

use async_trait::async_trait;

use super::event::EventQueue;
use super::task_store::TaskStore;
use crate::error::Result;
use crate::types::{Message, Task};

/// Trait for implementing agent execution logic.
///
/// The agent translates its outputs to A2A events and writes them to the
/// provided [`EventQueue`]. The server stops processing after:
/// - A [`Message`] event with any payload
/// - A [`TaskStatusUpdateEvent`](crate::types::TaskStatusUpdateEvent) with `final = true`
/// - A [`Task`] with a terminal [`TaskState`](crate::types::TaskState)
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
}

/// Request context passed to the executor during message processing.
///
/// Carries task identity, the triggering message, and any previously stored task.
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
    pub metadata: crate::types::Metadata,
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
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Creates a new request context with auto-generated IDs.
    #[must_use]
    pub fn create() -> Self {
        Self::new(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        )
    }
}

/// Extension point for modifying [`RequestContext`] before it reaches [`AgentExecutor`].
///
/// Multiple interceptors are applied in order of registration.
#[async_trait]
pub trait RequestContextInterceptor: Send + Sync {
    /// Intercept and optionally modify the request context before execution.
    async fn intercept(&self, ctx: &mut RequestContext) -> Result<()>;
}

/// Loads tasks referenced by [`Message::reference_task_ids`](crate::types::Message)
/// into [`RequestContext::related_tasks`].
pub struct ReferencedTasksLoader {
    store: Arc<dyn TaskStore>,
}

impl ReferencedTasksLoader {
    /// Creates a new loader backed by the given task store.
    pub fn new(store: Arc<dyn TaskStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl RequestContextInterceptor for ReferencedTasksLoader {
    async fn intercept(&self, ctx: &mut RequestContext) -> Result<()> {
        let reference_ids = match ctx.message.as_ref() {
            Some(m) if !m.reference_task_ids.is_empty() => m.reference_task_ids.clone(),
            _ => return Ok(()),
        };

        let mut tasks = Vec::new();
        for task_id in &reference_ids {
            match self.store.get(task_id).await {
                Ok(Some((t, _version))) => tasks.push(t),
                Ok(None) => {
                    tracing::info!(referenced_task_id = %task_id, "Referenced task not found");
                }
                Err(e) => {
                    tracing::info!(error = %e, referenced_task_id = %task_id, "Failed to load referenced task");
                }
            }
        }

        if !tasks.is_empty() {
            ctx.related_tasks = tasks;
        }
        Ok(())
    }
}
