//! Composable Axum handlers for A2A agents.
//!
//! The SDK does **not** own or manage the HTTP server — you mount A2A handlers
//! on your own Axum [`Router`], just like Go SDK's `NewJSONRPCHandler` /
//! `NewStaticAgentCardHandler` pattern.
//!
//! - [`a2a_router`] — complete A2A router (JSON-RPC + SSE + agent card)
//! - [`handle_jsonrpc`] / [`handle_sse`] / [`handle_agent_card`] — individual handlers
//!
//! ```ignore
//! let state = ServerState::from_executor(agent, card);
//! let app = Router::new().merge(a2a_router(state));
//! let listener = TcpListener::bind("0.0.0.0:8080").await?;
//! axum::serve(listener, app).await?;
//! ```

use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::Response,
    routing::{get, post},
};

use super::{ServerState, handle_request};
use crate::types::AgentCard;

/// Returns a complete Axum [`Router`] with all A2A protocol endpoints.
///
/// Mounts three routes:
/// - `/.well-known/agent-card.json` — agent card discovery ([`handle_agent_card`])
/// - `/` (POST) — JSON-RPC endpoint ([`handle_jsonrpc`])
/// - `/stream` (POST) — SSE streaming endpoint ([`handle_sse`])
///
/// # Example
///
/// ```ignore
/// use ra2a::server::{ServerState, a2a_router};
///
/// let state = ServerState::from_executor(my_executor, my_card);
/// let app = axum::Router::new()
///     .merge(a2a_router(state))
///     .route("/health", axum::routing::get(|| async { "OK" }));
///
/// let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
/// axum::serve(listener, app).await?;
/// ```
pub fn a2a_router(state: ServerState) -> Router {
    Router::new()
        .route(crate::WELL_KNOWN_AGENT_CARD_PATH, get(handle_agent_card))
        .route("/", post(handle_jsonrpc))
        .route("/stream", post(handle_sse))
        .with_state(state)
}

/// Axum handler for the agent card well-known endpoint.
///
/// Calls the [`AgentCardProducer`](super::AgentCardProducer) to generate the card.
/// Equivalent to Go SDK's `NewStaticAgentCardHandler`.
pub async fn handle_agent_card(
    State(state): State<ServerState>,
) -> Result<Json<AgentCard>, StatusCode> {
    match state.card_producer.card().await {
        Ok(card) => Ok(Json(card)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Axum handler for the JSON-RPC endpoint.
///
/// Dispatches all non-streaming A2A methods through the
/// [`RequestHandler`](super::RequestHandler).
/// Equivalent to Go SDK's `NewJSONRPCHandler`.
///
/// Propagates HTTP request headers to [`InterceptedHandler`](super::InterceptedHandler)
/// via [`REQUEST_META`](super::REQUEST_META) task-local, matching Go's
/// `WithCallContext(ctx, NewRequestMeta(req.Header))`.
pub async fn handle_jsonrpc(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    match super::REQUEST_META
        .scope(meta, handle_request(&state, &body))
        .await
    {
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

/// Axum handler for the SSE streaming endpoint.
///
/// Dispatches `message/stream` and `tasks/resubscribe` through the
/// [`RequestHandler`](super::RequestHandler), wrapping each event in a JSON-RPC
/// response envelope — aligned with Go's `handleStreamingRequest`.
pub async fn handle_sse(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    use axum::response::IntoResponse;
    use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
    use futures::StreamExt;

    use super::handler::parse_params;
    use crate::error::JsonRpcError;
    use crate::jsonrpc::{self, JsonRpcRequest};
    use crate::types::{MessageSendParams, TaskIdParams};

    let meta = super::RequestMeta::from_header_map(&headers);

    let request: JsonRpcRequest<serde_json::Value> = match serde_json::from_str(&body) {
        Ok(req) => req,
        Err(_) => {
            return sse_error_response(None, JsonRpcError::parse_error());
        }
    };

    let request_id = request.id.clone();
    let handler = &state.handler;

    // Wrap handler calls in REQUEST_META scope so InterceptedHandler sees headers
    let event_stream = super::REQUEST_META
        .scope(meta, async {
            match request.method.as_str() {
                jsonrpc::METHOD_MESSAGE_STREAM => {
                    match parse_params::<MessageSendParams>(&request) {
                        Ok(p) => handler.on_message_stream(p).await,
                        Err(e) => Err(e),
                    }
                }
                jsonrpc::METHOD_TASKS_RESUBSCRIBE => match parse_params::<TaskIdParams>(&request) {
                    Ok(p) => handler.on_resubscribe(p).await,
                    Err(e) => Err(e),
                },
                _ => Err(JsonRpcError::method_not_found(&request.method).into()),
            }
        })
        .await;

    let event_stream = match event_stream {
        Ok(s) => s,
        Err(e) => {
            return sse_error_response(Some(&request_id), e.to_jsonrpc_error());
        }
    };

    let id_for_stream = request_id.clone();
    let sse_stream = event_stream.map(move |item| {
        let data = match item {
            Ok(event) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id_for_stream,
                "result": event,
            }),
            Err(e) => {
                let rpc_err = e.to_jsonrpc_error();
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id_for_stream,
                    "error": { "code": rpc_err.code, "message": rpc_err.message },
                })
            }
        };
        Ok::<_, std::convert::Infallible>(SseEvent::default().data(data.to_string()))
    });

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Builds a JSON-RPC error response for SSE setup failures.
fn sse_error_response(
    id: Option<&crate::jsonrpc::RequestId>,
    error: crate::error::JsonRpcError,
) -> Response {
    let resp = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": error.code, "message": error.message },
    });
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(resp.to_string()))
        .unwrap()
}
