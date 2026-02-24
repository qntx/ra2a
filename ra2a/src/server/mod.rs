//! A2A Server module.
//!
//! - [`a2a_router`] — composable Axum router with all A2A endpoints
//! - [`AgentExecutor`] — trait for implementing agent business logic
//! - [`RequestHandler`] — trait defining all A2A JSON-RPC method handlers
//! - [`DefaultRequestHandler`] — standard implementation coordinating executor, stores, queues
//! - [`InterceptedHandler`] — decorator applying [`CallInterceptor`]s
//! - [`TaskStore`] / [`EventQueue`] / [`PushConfigStore`] — storage and eventing

mod event;
mod executor;
mod handler;
mod http;
mod middleware;
mod push;
mod task_store;

use std::sync::Arc;

use async_trait::async_trait;
pub use event::{Event, EventQueue, QueueManager};
pub use executor::{
    AgentExecutor, ReferencedTasksLoader, RequestContext, RequestContextInterceptor,
};
pub use handler::{
    DefaultRequestHandler, EventStream, InterceptedHandler, RequestHandler, handle_request,
};
pub use http::{a2a_router, handle_agent_card, handle_jsonrpc, handle_sse};
pub use middleware::{
    AuthenticatedUser, CallContext, CallInterceptor, PassthroughInterceptor, Request, RequestMeta,
    Response, UnauthenticatedUser, User,
};
pub use push::{
    HttpPushSender, HttpPushSenderConfig, InMemoryPushConfigStore, PushConfigStore, PushSender,
};
pub use task_store::{InMemoryTaskStore, TaskStore};

use crate::error::Result;
use crate::types::AgentCard;

/// Trait for producing agent cards dynamically.
///
/// Allows agent cards to vary based on request context (e.g. authentication).
/// For static cards, [`AgentCard`] itself implements this trait.
#[async_trait]
pub trait AgentCardProducer: Send + Sync {
    /// Returns the public agent card.
    async fn card(&self) -> Result<AgentCard>;
}

#[async_trait]
impl AgentCardProducer for AgentCard {
    async fn card(&self) -> Result<Self> {
        Ok(self.clone())
    }
}

/// Builder for constructing a fully-configured [`InterceptedHandler`].
///
/// # Example
/// ```ignore
/// let handler = HandlerBuilder::new(my_executor, my_agent_card)
///     .with_task_store(my_store)
///     .with_push_notifications(push_store, push_sender)
///     .build();
/// ```
pub struct HandlerBuilder {
    executor: Box<dyn AgentExecutor>,
    agent_card: AgentCard,
    task_store: Option<Arc<dyn TaskStore>>,
    queue_manager: Option<Arc<QueueManager>>,
    push_config_store: Option<Arc<dyn PushConfigStore>>,
    push_sender: Option<Arc<dyn PushSender>>,
    req_context_interceptors: Vec<Arc<dyn RequestContextInterceptor>>,
    call_interceptors: Vec<Arc<dyn CallInterceptor>>,
}

impl HandlerBuilder {
    /// Creates a new builder with the given agent executor and agent card.
    pub fn new(executor: impl AgentExecutor + 'static, agent_card: AgentCard) -> Self {
        Self {
            executor: Box::new(executor),
            agent_card,
            task_store: None,
            queue_manager: None,
            push_config_store: None,
            push_sender: None,
            req_context_interceptors: Vec::new(),
            call_interceptors: Vec::new(),
        }
    }

    /// Overrides the task store (default: in-memory).
    pub fn with_task_store(mut self, store: Arc<dyn TaskStore>) -> Self {
        self.task_store = Some(store);
        self
    }

    /// Overrides the event queue manager (default: in-memory).
    pub fn with_queue_manager(mut self, manager: Arc<QueueManager>) -> Self {
        self.queue_manager = Some(manager);
        self
    }

    /// Adds push notification support.
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
    pub fn with_request_context_interceptor(
        mut self,
        interceptor: Arc<dyn RequestContextInterceptor>,
    ) -> Self {
        self.req_context_interceptors.push(interceptor);
        self
    }

    /// Adds a call interceptor (applied by [`InterceptedHandler`]).
    pub fn with_call_interceptor(mut self, interceptor: Arc<dyn CallInterceptor>) -> Self {
        self.call_interceptors.push(interceptor);
        self
    }

    /// Builds the final [`InterceptedHandler`] wrapping a [`DefaultRequestHandler`].
    pub fn build(self) -> InterceptedHandler {
        let mut handler = DefaultRequestHandler::new_from_boxed(self.executor, self.agent_card);

        if let Some(manager) = self.queue_manager {
            handler = handler.with_queue_manager(manager);
        }
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

    /// Convenience: wraps an [`AgentExecutor`] in a [`DefaultRequestHandler`].
    pub fn from_executor(executor: impl AgentExecutor + 'static, agent_card: AgentCard) -> Self {
        let card_producer: Arc<dyn AgentCardProducer> = Arc::new(agent_card.clone());
        let handler = Arc::new(DefaultRequestHandler::new(executor, agent_card));
        Self {
            handler,
            card_producer,
        }
    }
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerState").finish_non_exhaustive()
    }
}
