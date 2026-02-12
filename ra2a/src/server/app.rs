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
/// Wraps an Axum router configured for handling A2A protocol requests.
/// All JSON-RPC methods are dispatched through a single
/// [`RequestHandler`](super::RequestHandler) held in [`ServerState`].
pub struct A2AServer {
    /// The Axum router.
    router: Router,
    /// Server configuration.
    config: ServerConfig,
}

impl A2AServer {
    /// Creates a new A2A server from an [`AgentExecutor`].
    ///
    /// Internally wraps the executor in a [`DefaultRequestHandler`](super::DefaultRequestHandler).
    pub fn new(executor: impl AgentExecutor + 'static) -> Self {
        Self::with_config(executor, ServerConfig::default())
    }

    /// Creates a new A2A server with custom configuration.
    pub fn with_config(executor: impl AgentExecutor + 'static, config: ServerConfig) -> Self {
        let state = ServerState::from_executor(executor);
        let router = Self::build_router(state, &config);
        Self { router, config }
    }

    /// Creates a server from a pre-built [`ServerState`].
    pub fn from_state(state: ServerState, config: ServerConfig) -> Self {
        let router = Self::build_router(state, &config);
        Self { router, config }
    }

    /// Builds the Axum router with all A2A endpoints.
    fn build_router(state: ServerState, config: &ServerConfig) -> Router {
        let mut router = Router::new()
            .route("/.well-known/agent.json", get(handle_agent_card))
            .route("/", post(handle_jsonrpc))
            .route("/stream", post(handle_sse_stream))
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
async fn handle_agent_card(State(state): State<ServerState>) -> Json<AgentCard> {
    Json((*state.agent_card).clone())
}

/// Handler for the main JSON-RPC endpoint.
async fn handle_jsonrpc(State(state): State<ServerState>, body: String) -> Response {
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
                .status(StatusCode::OK)
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
/// Dispatches `message/stream` through [`RequestHandler::on_message_stream`](super::RequestHandler::on_message_stream)
/// and converts the resulting event stream to Server-Sent Events.
async fn handle_sse_stream(State(state): State<ServerState>, body: String) -> Response {
    use crate::types::{JsonRpcRequest, MessageSendParams};
    use axum::response::IntoResponse;
    use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
    use futures::StreamExt;

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

    let request_id = request.id;

    // Dispatch to the handler
    let event_stream = match state.handler.on_message_stream(params).await {
        Ok(s) => s,
        Err(e) => {
            let error_data = serde_json::json!({
                "jsonrpc": "2.0",
                "error": { "code": -32603, "message": e.to_string() },
                "id": request_id
            });
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(error_data.to_string()))
                .unwrap();
        }
    };

    // Convert Event stream to SSE
    let sse_stream = event_stream.map(move |item| {
        match item {
            Ok(event) => {
                let (event_type, data) = event.to_sse_data();
                Ok::<_, std::convert::Infallible>(
                    SseEvent::default().event(event_type).data(data),
                )
            }
            Err(e) => {
                let error_data = serde_json::json!({
                    "error": { "code": -32603, "message": e.to_string() }
                });
                Ok(SseEvent::default().event("error").data(error_data.to_string()))
            }
        }
    });

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Builder for creating an A2A server.
#[derive(Debug)]
pub struct A2AServerBuilder {
    /// Pre-built server state (takes priority over executor).
    state: Option<ServerState>,
    /// Server configuration.
    config: ServerConfig,
}

impl A2AServerBuilder {
    /// Creates a new server builder.
    pub fn new() -> Self {
        Self {
            state: None,
            config: ServerConfig::default(),
        }
    }

    /// Sets the agent executor. Wraps it in a [`DefaultRequestHandler`](super::DefaultRequestHandler).
    pub fn executor(mut self, executor: impl AgentExecutor + 'static) -> Self {
        self.state = Some(ServerState::from_executor(executor));
        self
    }

    /// Sets a pre-built server state (for custom RequestHandler).
    pub fn state(mut self, state: ServerState) -> Self {
        self.state = Some(state);
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
    /// Panics if neither executor nor state has been set.
    pub fn build(self) -> A2AServer {
        let state = self.state.expect("Executor or state must be set");
        A2AServer::from_state(state, self.config)
    }
}

impl Default for A2AServerBuilder {
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
