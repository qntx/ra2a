//! Server-side shared state.

use std::sync::Arc;

/// Server state shared across all request handlers.
///
/// Holds an [`AgentCard`] for the well-known endpoint and a boxed
/// [`RequestHandler`](super::RequestHandler) that all JSON-RPC methods are
/// dispatched to. This eliminates duplicate storage previously kept in both
/// `ServerState` and `DefaultRequestHandler`.
#[derive(Clone)]
pub struct ServerState {
    /// The request handler all methods are dispatched to.
    pub handler: Arc<dyn super::RequestHandler>,
    /// The agent card served on the well-known endpoint.
    pub agent_card: Arc<crate::types::AgentCard>,
}

impl ServerState {
    /// Creates a server state from an existing handler and agent card.
    pub fn new(
        handler: Arc<dyn super::RequestHandler>,
        agent_card: crate::types::AgentCard,
    ) -> Self {
        Self {
            handler,
            agent_card: Arc::new(agent_card),
        }
    }

    /// Convenience constructor: wraps an [`AgentExecutor`](super::AgentExecutor) in a
    /// [`DefaultRequestHandler`](super::DefaultRequestHandler) automatically.
    pub fn from_executor<E: super::AgentExecutor + 'static>(executor: E) -> Self {
        let card = executor.agent_card().clone();
        let handler = Arc::new(super::DefaultRequestHandler::new(executor));
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
