//! REST API handler for the A2A server.
//!
//! Provides RESTful endpoints as an alternative to JSON-RPC.
//! Mirrors Python's `server/apps/rest/rest_adapter.py`.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse, sse::Event as SseEvent},
};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;

use crate::error::A2AError;
use crate::types::{
    AgentCard, DeleteTaskPushNotificationConfigParams, GetTaskPushNotificationConfigParams,
    ListTaskPushNotificationConfigParams, Message, MessageSendParams, TaskPushNotificationConfig,
};

use super::call_context::ServerCallContext;
use super::events::Event;
use super::request_handler::{RequestHandler, SendMessageResponse};

/// REST API response wrapper.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RestResponse<T> {
    /// Successful response with data.
    Success(T),
}

/// REST API error response.
#[derive(Debug, Clone, Serialize)]
pub struct RestErrorResponse {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub message: String,
    /// Additional error details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl RestErrorResponse {
    /// Creates a new error response.
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Creates an error response with details.
    pub fn with_details(code: i32, message: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            code,
            message: message.into(),
            details: Some(details),
        }
    }

    /// Creates from an A2AError.
    pub fn from_error(err: &A2AError) -> Self {
        match err {
            A2AError::JsonRpc(e) => Self::new(e.code, &e.message),
            A2AError::ClientJsonRpc { code, message, .. } => Self::new(*code, message),
            _ => Self::new(-32603, err.to_string()),
        }
    }
}

impl IntoResponse for RestErrorResponse {
    fn into_response(self) -> Response {
        let status = match self.code {
            -32700 => StatusCode::BAD_REQUEST,
            -32600 => StatusCode::BAD_REQUEST,
            -32601 => StatusCode::NOT_FOUND,
            -32602 => StatusCode::BAD_REQUEST,
            -32001 => StatusCode::NOT_FOUND,
            -32002 => StatusCode::CONFLICT,
            -32003 => StatusCode::NOT_IMPLEMENTED,
            -32004 => StatusCode::NOT_IMPLEMENTED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self)).into_response()
    }
}

/// Query parameters for message send.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MessageSendQuery {
    /// Whether to stream the response.
    #[serde(default)]
    pub stream: bool,
}

/// Query parameters for task operations.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TaskQuery {
    /// Maximum history length to return.
    pub history_length: Option<u32>,
}

/// Query parameters for push notification config.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PushConfigQuery {
    /// Config ID to filter by.
    pub config_id: Option<String>,
}

/// REST handler that wraps a RequestHandler.
pub struct RestHandler<H> {
    handler: Arc<H>,
    agent_card: Arc<AgentCard>,
}

impl<H> Clone for RestHandler<H> {
    fn clone(&self) -> Self {
        Self {
            handler: Arc::clone(&self.handler),
            agent_card: Arc::clone(&self.agent_card),
        }
    }
}

impl<H> RestHandler<H> {
    /// Creates a new REST handler.
    pub fn new(handler: H, agent_card: AgentCard) -> Self {
        Self {
            handler: Arc::new(handler),
            agent_card: Arc::new(agent_card),
        }
    }

    /// Creates a new REST handler with Arc handler.
    pub fn with_arc(handler: Arc<H>, agent_card: AgentCard) -> Self {
        Self {
            handler,
            agent_card: Arc::new(agent_card),
        }
    }
}

/// POST /v1/message - Send a message (non-streaming or streaming based on query).
pub async fn send_message<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
    Query(query): Query<MessageSendQuery>,
    Json(message): Json<Message>,
) -> Response {
    let params = MessageSendParams::new(message);
    let context = ServerCallContext::new();

    if query.stream {
        // Return streaming response
        match rest.handler.on_message_stream(params, Some(&context)).await {
            Ok(stream) => {
                let sse_stream = event_stream_to_sse(stream);
                Sse::new(sse_stream).into_response()
            }
            Err(e) => RestErrorResponse::from_error(&e).into_response(),
        }
    } else {
        // Return non-streaming response
        match rest.handler.on_message_send(params, Some(&context)).await {
            Ok(response) => match response {
                SendMessageResponse::Task(task) => Json(task).into_response(),
                SendMessageResponse::Message(msg) => Json(msg).into_response(),
            },
            Err(e) => RestErrorResponse::from_error(&e).into_response(),
        }
    }
}

/// POST /v1/message:stream - Send a message with streaming response.
pub async fn send_message_stream<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
    Json(message): Json<Message>,
) -> Response {
    let params = MessageSendParams::new(message);
    let context = ServerCallContext::new();

    match rest.handler.on_message_stream(params, Some(&context)).await {
        Ok(stream) => {
            let sse_stream = event_stream_to_sse(stream);
            Sse::new(sse_stream).into_response()
        }
        Err(e) => RestErrorResponse::from_error(&e).into_response(),
    }
}

/// GET /v1/tasks/{id} - Get a task by ID.
pub async fn get_task<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
    Path(task_id): Path<String>,
    Query(query): Query<TaskQuery>,
) -> Response {
    let mut params = crate::types::TaskQueryParams::new(task_id);
    params.history_length = query.history_length.map(|v| v as i32);
    let context = ServerCallContext::new();

    match rest.handler.on_get_task(params, Some(&context)).await {
        Ok(task) => Json(task).into_response(),
        Err(e) => RestErrorResponse::from_error(&e).into_response(),
    }
}

/// POST /v1/tasks/{id}:cancel - Cancel a task.
pub async fn cancel_task<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
    Path(task_id): Path<String>,
) -> Response {
    let params = crate::types::TaskIdParams::new(task_id);
    let context = ServerCallContext::new();

    match rest.handler.on_cancel_task(params, Some(&context)).await {
        Ok(task) => Json(task).into_response(),
        Err(e) => RestErrorResponse::from_error(&e).into_response(),
    }
}

/// GET /v1/tasks/{id}:subscribe - Subscribe to task updates (SSE).
pub async fn subscribe_task<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
    Path(task_id): Path<String>,
) -> Response {
    let params = crate::types::TaskIdParams::new(task_id);
    let context = ServerCallContext::new();

    match rest.handler.on_resubscribe(params, Some(&context)).await {
        Ok(stream) => {
            let sse_stream = event_stream_to_sse(stream);
            Sse::new(sse_stream).into_response()
        }
        Err(e) => RestErrorResponse::from_error(&e).into_response(),
    }
}

/// GET /v1/agent-card - Get the agent card.
pub async fn get_agent_card<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
) -> Json<AgentCard> {
    Json((*rest.agent_card).clone())
}

/// POST /v1/tasks/{id}/push-config - Set push notification config.
pub async fn set_push_config<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
    Path(task_id): Path<String>,
    Json(config): Json<crate::types::PushNotificationConfig>,
) -> Response {
    let params = TaskPushNotificationConfig {
        task_id,
        push_notification_config: config,
    };
    let context = ServerCallContext::new();

    match rest
        .handler
        .on_set_push_notification_config(params, Some(&context))
        .await
    {
        Ok(config) => Json(config).into_response(),
        Err(e) => RestErrorResponse::from_error(&e).into_response(),
    }
}

/// GET /v1/tasks/{id}/push-config - Get push notification config.
pub async fn get_push_config<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
    Path(task_id): Path<String>,
    Query(query): Query<PushConfigQuery>,
) -> Response {
    let mut params = GetTaskPushNotificationConfigParams::new(task_id);
    if let Some(config_id) = query.config_id {
        params = params.with_config_id(config_id);
    }
    let context = ServerCallContext::new();

    match rest
        .handler
        .on_get_push_notification_config(params, Some(&context))
        .await
    {
        Ok(config) => Json(config).into_response(),
        Err(e) => RestErrorResponse::from_error(&e).into_response(),
    }
}

/// GET /v1/tasks/{id}/push-configs - List all push notification configs.
pub async fn list_push_configs<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
    Path(task_id): Path<String>,
) -> Response {
    let params = ListTaskPushNotificationConfigParams::new(task_id);
    let context = ServerCallContext::new();

    match rest
        .handler
        .on_list_push_notification_config(params, Some(&context))
        .await
    {
        Ok(configs) => Json(configs).into_response(),
        Err(e) => RestErrorResponse::from_error(&e).into_response(),
    }
}

/// DELETE /v1/tasks/{id}/push-config/{config_id} - Delete push notification config.
pub async fn delete_push_config<H: RequestHandler + 'static>(
    State(rest): State<RestHandler<H>>,
    Path((task_id, config_id)): Path<(String, String)>,
) -> Response {
    let params = DeleteTaskPushNotificationConfigParams::new(task_id, config_id);
    let context = ServerCallContext::new();

    match rest
        .handler
        .on_delete_push_notification_config(params, Some(&context))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => RestErrorResponse::from_error(&e).into_response(),
    }
}

/// Converts an event stream to SSE events.
fn event_stream_to_sse(
    stream: super::request_handler::EventStream,
) -> impl Stream<Item = std::result::Result<SseEvent, Infallible>> {
    use futures::StreamExt;

    stream.map(|result| {
        Ok(match result {
            Ok(event) => {
                let event_type = match &event {
                    Event::StatusUpdate(_) => "status",
                    Event::ArtifactUpdate(_) => "artifact",
                    Event::Task(_) => "task",
                    Event::Message(_) => "message",
                };
                let data = serde_json::to_string(&event).unwrap_or_default();
                SseEvent::default().event(event_type).data(data)
            }
            Err(e) => {
                let error = RestErrorResponse::from_error(&e);
                let data = serde_json::to_string(&error).unwrap_or_default();
                SseEvent::default().event("error").data(data)
            }
        })
    })
}

/// Creates REST API routes for the handler.
///
/// Returns an Axum router with all REST endpoints configured.
/// Creates REST API routes for the handler.
///
/// Note: This is a simplified version. For production use, consider
/// adding proper error handling and authentication middleware.
pub fn rest_routes<H: RequestHandler + 'static>(handler: RestHandler<H>) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/v1/message", post(send_message::<H>))
        .route("/v1/message:stream", post(send_message_stream::<H>))
        .route("/v1/tasks/:id", get(get_task::<H>))
        .route("/v1/tasks/:id/cancel", post(cancel_task::<H>))
        .route("/v1/tasks/:id/subscribe", get(subscribe_task::<H>))
        .route("/v1/agent-card", get(get_agent_card::<H>))
        .route("/v1/tasks/:id/push-config", post(set_push_config::<H>))
        .route("/v1/tasks/:id/push-config", get(get_push_config::<H>))
        .route("/v1/tasks/:id/push-configs", get(list_push_configs::<H>))
        .with_state(handler)
}

#[cfg(test)]
mod tests {
    use crate::error::JsonRpcError;

    use super::*;

    #[test]
    fn test_rest_error_response() {
        let error = RestErrorResponse::new(-32001, "Task not found");
        assert_eq!(error.code, -32001);
        assert_eq!(error.message, "Task not found");
    }

    #[test]
    fn test_rest_error_from_a2a_error() {
        let err = A2AError::JsonRpc(JsonRpcError::task_not_found("test-123"));
        let response = RestErrorResponse::from_error(&err);
        assert_eq!(response.code, -32001);
    }
}
