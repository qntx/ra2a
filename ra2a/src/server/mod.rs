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
mod intercepted_handler;
mod middleware;
mod push;
mod task_store;

use std::sync::Arc;

pub use app::*;
use async_trait::async_trait;
pub use default_handler::*;
pub use events::*;
pub use handler::*;
pub use intercepted_handler::*;
pub use middleware::*;
pub use push::*;
pub use task_store::*;

use crate::error::Result;
use crate::types::{AgentCard, Message, Task};

/// Trait for producing agent cards dynamically.
///
/// Aligned with Go's `AgentCardProducer` interface in `agentcard.go`.
/// Allows agent cards to vary based on request context (e.g. authentication).
///
/// For static cards, [`AgentCard`] itself implements this trait.
#[async_trait]
pub trait AgentCardProducer: Send + Sync {
    /// Returns the public agent card.
    async fn card(&self) -> Result<AgentCard>;
}

/// A static card always returns a clone of itself.
#[async_trait]
impl AgentCardProducer for AgentCard {
    async fn card(&self) -> Result<AgentCard> {
        Ok(self.clone())
    }
}

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
/// Aligned with Go's `RequestContextInterceptor` in `reqctx.go`.
/// Multiple interceptors are applied in order of registration.
#[async_trait]
pub trait RequestContextInterceptor: Send + Sync {
    /// Intercept and optionally modify the request context before execution.
    async fn intercept(&self, ctx: &mut RequestContext) -> Result<()>;
}

/// Loads tasks referenced by [`Message::reference_task_ids`](crate::types::Message)
/// into [`RequestContext::related_tasks`].
///
/// Aligned with Go's `ReferencedTasksLoader`.
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
        let reference_ids = match ctx.message.as_ref().and_then(|m| m.reference_task_ids.as_ref()) {
            Some(ids) if !ids.is_empty() => ids.clone(),
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

// ---------------------------------------------------------------------------
// HandlerBuilder (Go: NewHandler + RequestHandlerOption)
// ---------------------------------------------------------------------------

/// Builder for constructing a fully-configured [`InterceptedHandler`].
///
/// Aligned with Go's `NewHandler(executor, ...RequestHandlerOption)`.
/// Assembles both the inner [`DefaultRequestHandler`] and the outer
/// [`InterceptedHandler`] in a single builder chain.
///
/// # Example
/// ```ignore
/// use std::sync::Arc;
/// use ra2a::server::HandlerBuilder;
///
/// let handler = HandlerBuilder::new(my_executor)
///     .with_task_store(my_store)
///     .with_push_notifications(push_store, push_sender)
///     .with_call_interceptor(Arc::new(my_interceptor))
///     .build();
/// ```
pub struct HandlerBuilder<E: AgentExecutor + 'static> {
    executor: E,
    task_store: Option<Arc<dyn TaskStore>>,
    queue_manager: Option<Arc<QueueManager>>,
    push_config_store: Option<Arc<dyn PushConfigStore>>,
    push_sender: Option<Arc<dyn PushSender>>,
    req_context_interceptors: Vec<Arc<dyn RequestContextInterceptor>>,
    call_interceptors: Vec<Arc<dyn CallInterceptor>>,
}

impl<E: AgentExecutor + 'static> HandlerBuilder<E> {
    /// Creates a new builder with the given agent executor.
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            task_store: None,
            queue_manager: None,
            push_config_store: None,
            push_sender: None,
            req_context_interceptors: Vec::new(),
            call_interceptors: Vec::new(),
        }
    }

    /// Overrides the task store (default: in-memory).
    ///
    /// Aligned with Go's `WithTaskStore`.
    pub fn with_task_store(mut self, store: Arc<dyn TaskStore>) -> Self {
        self.task_store = Some(store);
        self
    }

    /// Overrides the event queue manager (default: in-memory).
    ///
    /// Aligned with Go's `WithEventQueueManager`.
    pub fn with_queue_manager(mut self, manager: Arc<QueueManager>) -> Self {
        self.queue_manager = Some(manager);
        self
    }

    /// Adds push notification support.
    ///
    /// Aligned with Go's `WithPushNotifications`.
    pub fn with_push_notifications(
        mut self,
        store: Arc<dyn PushConfigStore>,
        sender: Arc<dyn PushSender>,
    ) -> Self {
        self.push_config_store = Some(store);
        self.push_sender = Some(sender);
        self
    }

    /// Adds a request context interceptor.
    ///
    /// Aligned with Go's `WithRequestContextInterceptor`.
    pub fn with_request_context_interceptor(
        mut self,
        interceptor: Arc<dyn RequestContextInterceptor>,
    ) -> Self {
        self.req_context_interceptors.push(interceptor);
        self
    }

    /// Adds a call interceptor (applied by [`InterceptedHandler`]).
    ///
    /// Aligned with Go's `WithCallInterceptor`.
    pub fn with_call_interceptor(mut self, interceptor: Arc<dyn CallInterceptor>) -> Self {
        self.call_interceptors.push(interceptor);
        self
    }

    /// Builds the final [`InterceptedHandler`] wrapping a [`DefaultRequestHandler`].
    pub fn build(self) -> InterceptedHandler {
        let mut handler = DefaultRequestHandler::new(self.executor);

        if let Some(store) = self.task_store {
            handler = handler.with_task_store(store);
        }
        if let Some(store) = self.push_config_store {
            handler = handler.with_push_config_store(store);
        }
        if let Some(sender) = self.push_sender {
            handler = handler.with_push_sender(sender);
        }
        for interceptor in self.req_context_interceptors {
            handler = handler.with_request_context_interceptor(interceptor);
        }

        let mut ih = InterceptedHandler::new(Arc::new(handler));
        for interceptor in self.call_interceptors {
            ih = ih.with_interceptor(interceptor);
        }
        ih
    }
}

// ---------------------------------------------------------------------------
// ConcurrencyConfig (Go: limiter/limiter.go)
// ---------------------------------------------------------------------------

/// Configuration for controlling concurrent agent executions.
///
/// Aligned with Go's `limiter.ConcurrencyConfig`. Limits are only enforced
/// when greater than zero.
#[derive(Clone, Default)]
pub struct ConcurrencyConfig {
    /// Maximum number of concurrent active executions (global limit).
    ///
    /// Only enforced when greater than zero.
    pub max_executions: usize,

    /// Optional per-scope limit function.
    ///
    /// When set, returns the maximum number of concurrent executions for a
    /// given scope string. Only enforced when the returned value is > 0.
    pub get_max_executions: Option<Arc<dyn Fn(&str) -> usize + Send + Sync>>,
}

impl std::fmt::Debug for ConcurrencyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConcurrencyConfig")
            .field("max_executions", &self.max_executions)
            .field("get_max_executions", &self.get_max_executions.as_ref().map(|_| "..."))
            .finish()
    }
}

/// Server state shared across all request handlers.
///
/// Holds an [`AgentCardProducer`] for the well-known endpoint and a boxed
/// [`RequestHandler`] that all JSON-RPC methods are dispatched to.
#[derive(Clone)]
pub struct ServerState {
    /// The request handler all methods are dispatched to.
    pub handler: Arc<dyn RequestHandler>,
    /// The agent card producer for the well-known endpoint.
    pub card_producer: Arc<dyn AgentCardProducer>,
}

impl ServerState {
    /// Creates a server state from an existing handler and a static agent card.
    pub fn new(handler: Arc<dyn RequestHandler>, agent_card: AgentCard) -> Self {
        Self {
            handler,
            card_producer: Arc::new(agent_card),
        }
    }

    /// Creates a server state with a dynamic card producer.
    pub fn with_card_producer(
        handler: Arc<dyn RequestHandler>,
        card_producer: Arc<dyn AgentCardProducer>,
    ) -> Self {
        Self {
            handler,
            card_producer,
        }
    }

    /// Convenience constructor: wraps an [`AgentExecutor`] in a
    /// [`DefaultRequestHandler`] automatically.
    pub fn from_executor<E: AgentExecutor + 'static>(executor: E) -> Self {
        let card = executor.agent_card().clone();
        let handler = Arc::new(DefaultRequestHandler::new(executor));
        Self {
            handler,
            card_producer: Arc::new(card),
        }
    }
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerState").finish_non_exhaustive()
    }
}
