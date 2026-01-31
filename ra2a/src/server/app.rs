//! Axum-based HTTP server application for A2A agents.

use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{Method, StatusCode, header},
    response::Response,
    routing::{get, post},
};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use super::{AgentExecutor, ServerState, handle_request};
use crate::types::AgentCard;

/// Configuration for the A2A server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The host address to bind to.
    pub host: String,
    /// The port to listen on.
    pub port: u16,
    /// Enable CORS for all origins.
    pub enable_cors: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            enable_cors: true,
        }
    }
}

impl ServerConfig {
    /// Creates a new server configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the host address.
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Sets the port.
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Enables or disables CORS.
    pub fn cors(mut self, enabled: bool) -> Self {
        self.enable_cors = enabled;
        self
    }

    /// Returns the bind address.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// A2A Server application.
///
/// This struct wraps an Axum router configured for handling A2A protocol requests.
pub struct A2AServer<E: AgentExecutor + 'static> {
    /// The Axum router.
    router: Router,
    /// Server configuration.
    config: ServerConfig,
    /// Server state (reserved for future middleware access).
    #[allow(dead_code)]
    state: ServerState<E>,
}

impl<E: AgentExecutor + 'static> A2AServer<E> {
    /// Creates a new A2A server with the given executor.
    pub fn new(executor: E) -> Self {
        Self::with_config(executor, ServerConfig::default())
    }

    /// Creates a new A2A server with custom configuration.
    pub fn with_config(executor: E, config: ServerConfig) -> Self {
        let state = ServerState::new(executor);
        let router = Self::build_router(state.clone(), &config);

        Self {
            router,
            config,
            state,
        }
    }

    /// Builds the Axum router with all A2A endpoints.
    fn build_router(state: ServerState<E>, config: &ServerConfig) -> Router {
        let mut router = Router::new()
            // Agent card endpoint
            .route("/.well-known/agent.json", get(handle_agent_card::<E>))
            // Main JSON-RPC endpoint
            .route("/", post(handle_jsonrpc::<E>))
            // SSE streaming endpoint
            .route("/stream", post(handle_sse_stream::<E>))
            // Health check endpoint
            .route("/health", get(handle_health))
            .with_state(state);

        if config.enable_cors {
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
            router = router.layer(cors);
        }

        router
    }

    /// Returns the Axum router.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Returns the server configuration.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Starts the server and listens for incoming connections.
    pub async fn serve(self) -> Result<(), std::io::Error> {
        let addr = self.config.bind_address();
        info!("Starting A2A server on {}", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, self.router).await
    }

    /// Starts the server with graceful shutdown support.
    pub async fn serve_with_shutdown<F>(self, shutdown_signal: F) -> Result<(), std::io::Error>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let addr = self.config.bind_address();
        info!("Starting A2A server on {} (with graceful shutdown)", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, self.router)
            .with_graceful_shutdown(shutdown_signal)
            .await
    }
}

/// Handler for the agent card endpoint.
async fn handle_agent_card<E: AgentExecutor>(
    State(state): State<ServerState<E>>,
) -> Json<AgentCard> {
    Json(state.executor.agent_card().clone())
}

/// Handler for the main JSON-RPC endpoint.
async fn handle_jsonrpc<E: AgentExecutor>(
    State(state): State<ServerState<E>>,
    body: String,
) -> Response {
    match handle_request(&state, &body).await {
        Ok(response) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(response))
            .unwrap(),
        Err(e) => {
            let error_body = serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32603,
                    "message": e.to_string()
                },
                "id": null
            });
            Response::builder()
                .status(StatusCode::OK) // JSON-RPC errors still return 200
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(error_body.to_string()))
                .unwrap()
        }
    }
}

/// Handler for the health check endpoint.
async fn handle_health() -> &'static str {
    "OK"
}

/// Handler for the SSE streaming endpoint.
///
/// This endpoint handles `message/stream` requests via Server-Sent Events.
async fn handle_sse_stream<E: AgentExecutor + 'static>(
    State(state): State<ServerState<E>>,
    body: String,
) -> Response {
    use super::ExecutionContext;
    use crate::types::{
        JsonRpcRequest, JsonRpcSuccessResponse, MessageSendParams, StreamingMessageResult,
    };
    use axum::response::IntoResponse;
    use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};

    // Parse the request
    let request: JsonRpcRequest<MessageSendParams> = match serde_json::from_str(&body) {
        Ok(req) => req,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"error":"{}"}}"#, e)))
                .unwrap();
        }
    };

    let params = match request.params {
        Some(p) => p,
        None => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(r#"{"error":"Missing params"}"#))
                .unwrap();
        }
    };

    let message = params.message;
    let request_id = request.id;

    // Determine task_id and context_id
    let (task_id, context_id) = match (&message.task_id, &message.context_id) {
        (Some(tid), Some(cid)) => (tid.clone(), cid.clone()),
        (Some(tid), None) => {
            if let Some(task) = state.get_task(tid).await {
                (tid.clone(), task.context_id)
            } else {
                (tid.clone(), uuid::Uuid::new_v4().to_string())
            }
        }
        _ => (
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        ),
    };

    // Execute the agent
    let ctx = ExecutionContext::new(&task_id, &context_id);
    let executor = state.executor.clone();

    // Create a stream that executes and emits events
    let stream = async_stream::stream! {
        match executor.execute(&ctx, &message).await {
            Ok(task) => {
                let result = StreamingMessageResult::Task(task);
                let response = JsonRpcSuccessResponse::new(Some(request_id.clone()), result);
                let data = serde_json::to_string(&response).unwrap_or_default();
                yield Ok::<_, std::convert::Infallible>(
                    SseEvent::default().event("task").data(data)
                );
            }
            Err(e) => {
                let error_data = serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32603,
                        "message": e.to_string()
                    },
                    "id": request_id
                });
                yield Ok(SseEvent::default().event("error").data(error_data.to_string()));
            }
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Builder for creating an A2A server.
#[derive(Debug)]
pub struct A2AServerBuilder<E> {
    executor: Option<E>,
    config: ServerConfig,
}

impl<E: AgentExecutor + 'static> A2AServerBuilder<E> {
    /// Creates a new server builder.
    pub fn new() -> Self {
        Self {
            executor: None,
            config: ServerConfig::default(),
        }
    }

    /// Sets the agent executor.
    pub fn executor(mut self, executor: E) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Sets the host address.
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.config.host = host.into();
        self
    }

    /// Sets the port.
    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    /// Enables or disables CORS.
    pub fn cors(mut self, enabled: bool) -> Self {
        self.config.enable_cors = enabled;
        self
    }

    /// Builds the server.
    ///
    /// # Panics
    ///
    /// Panics if no executor has been set.
    pub fn build(self) -> A2AServer<E> {
        let executor = self.executor.expect("Executor must be set");
        A2AServer::with_config(executor, self.config)
    }
}

impl<E: AgentExecutor + 'static> Default for A2AServerBuilder<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert!(config.enable_cors);
    }

    #[test]
    fn test_server_config_builder() {
        let config = ServerConfig::new().host("127.0.0.1").port(3000).cors(false);

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 3000);
        assert!(!config.enable_cors);
        assert_eq!(config.bind_address(), "127.0.0.1:3000");
    }
}
