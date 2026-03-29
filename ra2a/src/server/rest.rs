//! HTTP+JSON REST protocol binding for A2A agents.
//!
//! Implements the REST endpoints defined by the proto `google.api.http` annotations.
//! Error responses use google.rpc.Status format (AIP-193).
//!
//! - [`rest_router`] — Axum router with all REST endpoints
//! - Can be merged with the existing JSON-RPC router or used standalone

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router, body::Body};
use serde::Deserialize;

use super::ServerState;
use crate::error::A2AError;
use crate::types::{
    CancelTaskRequest, DeleteTaskPushNotificationConfigRequest, GetExtendedAgentCardRequest,
    GetTaskPushNotificationConfigRequest, GetTaskRequest, ListTaskPushNotificationConfigsRequest,
    ListTasksRequest, SendMessageRequest, SubscribeToTaskRequest, TaskId,
    TaskPushNotificationConfig, TaskState,
};

/// Returns an Axum [`Router`] implementing the HTTP+JSON REST protocol binding.
///
/// Routes follow the proto `google.api.http` annotations:
///
/// | Method | Path | Operation |
/// |--------|------|-----------|
/// | POST | `/message:send` | SendMessage |
/// | POST | `/message:stream` | SendStreamingMessage (SSE) |
/// | GET | `/tasks/{id}` | GetTask |
/// | GET | `/tasks` | ListTasks |
/// | POST | `/tasks/{id}:cancel` | CancelTask |
/// | GET | `/tasks/{id}:subscribe` | SubscribeToTask (SSE) |
/// | POST | `/tasks/{id}/pushNotificationConfigs` | CreateTaskPushNotificationConfig |
/// | GET | `/tasks/{id}/pushNotificationConfigs/{configId}` | GetTaskPushNotificationConfig |
/// | GET | `/tasks/{id}/pushNotificationConfigs` | ListTaskPushNotificationConfigs |
/// | DELETE | `/tasks/{id}/pushNotificationConfigs/{configId}` | DeleteTaskPushNotificationConfig |
/// | GET | `/extendedAgentCard` | GetExtendedAgentCard |
pub fn rest_router(state: ServerState) -> Router {
    rest_router_inner().with_state(state)
}

/// Returns the REST routes without state binding.
/// Used by [`super::http::a2a_full_router`] and [`super::http::a2a_tenant_router`]
/// to merge REST routes before a single `with_state` call.
pub(super) fn rest_router_inner() -> Router<ServerState> {
    Router::new()
        .route("/message:send", post(handle_send_message))
        .route("/message:stream", post(handle_stream_message))
        .route("/tasks", get(handle_list_tasks))
        .route("/tasks/{id}", get(handle_get_task))
        .route("/tasks/{id}:cancel", post(handle_cancel_task))
        .route("/tasks/{id}:subscribe", get(handle_subscribe_to_task))
        .route(
            "/tasks/{id}/pushNotificationConfigs",
            post(handle_create_push_config).get(handle_list_push_configs),
        )
        .route(
            "/tasks/{id}/pushNotificationConfigs/{configId}",
            get(handle_get_push_config).delete(handle_delete_push_config),
        )
        .route("/extendedAgentCard", get(handle_get_extended_agent_card))
}

#[derive(Deserialize)]
struct TaskIdPath {
    id: String,
}

#[derive(Deserialize)]
struct PushConfigPath {
    id: String,
    #[serde(rename = "configId")]
    config_id: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GetTaskQuery {
    history_length: Option<i32>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ListTasksQuery {
    context_id: Option<String>,
    status: Option<String>,
    page_size: Option<i32>,
    page_token: Option<String>,
    history_length: Option<i32>,
    status_timestamp_after: Option<String>,
    include_artifacts: Option<bool>,
}

/// Extracts tenant from the `x-a2a-tenant` header injected by [`super::http::inject_tenant_header`].
fn tenant_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("x-a2a-tenant")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn rest_error_response(err: &A2AError, task_id: Option<&str>) -> Response {
    let rest_err = err.to_rest_error(task_id);
    let status =
        StatusCode::from_u16(rest_err.http_status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = serde_json::to_string(&rest_err).unwrap_or_default();
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

async fn handle_send_message(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<SendMessageRequest>,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    let result = super::REQUEST_META
        .scope(meta, async { state.handler.on_message_send(req).await })
        .await;
    match result {
        Ok(resp) => json_response(&resp),
        Err(e) => rest_error_response(&e, None),
    }
}

async fn handle_stream_message(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<SendMessageRequest>,
) -> Response {
    use axum::response::IntoResponse;
    use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
    use futures::StreamExt;

    let meta = super::RequestMeta::from_header_map(&headers);
    let stream_result = super::REQUEST_META
        .scope(meta, async { state.handler.on_message_stream(req).await })
        .await;

    let event_stream = match stream_result {
        Ok(s) => s,
        Err(e) => return rest_error_response(&e, None),
    };

    let sse_stream = event_stream.map(|item| {
        let data = match item {
            Ok(event) => serde_json::to_string(&event).unwrap_or_default(),
            Err(e) => {
                let rest_err = e.to_rest_error(None);
                serde_json::to_string(&rest_err).unwrap_or_default()
            }
        };
        Ok::<_, std::convert::Infallible>(SseEvent::default().data(data))
    });

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn handle_get_task(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Path(path): Path<TaskIdPath>,
    Query(query): Query<GetTaskQuery>,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    let task_id = path.id;
    let req = GetTaskRequest {
        tenant: tenant_from_headers(&headers),
        id: TaskId::from(task_id.as_str()),
        history_length: query.history_length,
    };
    let result = super::REQUEST_META
        .scope(meta, async { state.handler.on_get_task(req).await })
        .await;
    match result {
        Ok(task) => json_response(&task),
        Err(e) => rest_error_response(&e, Some(&task_id)),
    }
}

async fn handle_list_tasks(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Query(query): Query<ListTasksQuery>,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    let req = ListTasksRequest {
        tenant: tenant_from_headers(&headers),
        context_id: query.context_id,
        status: query.status.map(|s| TaskState::from_state_str(&s)),
        page_size: query.page_size,
        page_token: query.page_token,
        history_length: query.history_length,
        status_timestamp_after: query.status_timestamp_after,
        include_artifacts: query.include_artifacts,
    };
    let result = super::REQUEST_META
        .scope(meta, async { state.handler.on_list_tasks(req).await })
        .await;
    match result {
        Ok(resp) => json_response(&resp),
        Err(e) => rest_error_response(&e, None),
    }
}

async fn handle_cancel_task(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Path(path): Path<TaskIdPath>,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    let req = CancelTaskRequest {
        tenant: tenant_from_headers(&headers),
        id: TaskId::from(path.id.as_str()),
        metadata: None,
    };
    let result = super::REQUEST_META
        .scope(meta, async { state.handler.on_cancel_task(req).await })
        .await;
    match result {
        Ok(task) => json_response(&task),
        Err(e) => rest_error_response(&e, Some(&path.id)),
    }
}

async fn handle_subscribe_to_task(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Path(path): Path<TaskIdPath>,
) -> Response {
    use axum::response::IntoResponse;
    use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
    use futures::StreamExt;

    let meta = super::RequestMeta::from_header_map(&headers);
    let req = SubscribeToTaskRequest {
        tenant: tenant_from_headers(&headers),
        id: TaskId::from(path.id.as_str()),
    };
    let stream_result = super::REQUEST_META
        .scope(meta, async {
            state.handler.on_subscribe_to_task(req).await
        })
        .await;

    let event_stream = match stream_result {
        Ok(s) => s,
        Err(e) => return rest_error_response(&e, Some(&path.id)),
    };

    let sse_stream = event_stream.map(|item| {
        let data = match item {
            Ok(event) => serde_json::to_string(&event).unwrap_or_default(),
            Err(e) => {
                let rest_err = e.to_rest_error(None);
                serde_json::to_string(&rest_err).unwrap_or_default()
            }
        };
        Ok::<_, std::convert::Infallible>(SseEvent::default().data(data))
    });

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn handle_create_push_config(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Path(path): Path<TaskIdPath>,
    Json(mut config): Json<TaskPushNotificationConfig>,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    config.task_id = Some(TaskId::from(path.id.as_str()));
    let result = super::REQUEST_META
        .scope(meta, async {
            state.handler.on_create_task_push_config(config).await
        })
        .await;
    match result {
        Ok(cfg) => json_response(&cfg),
        Err(e) => rest_error_response(&e, Some(&path.id)),
    }
}

async fn handle_get_push_config(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Path(path): Path<PushConfigPath>,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    let req = GetTaskPushNotificationConfigRequest {
        tenant: tenant_from_headers(&headers),
        task_id: TaskId::from(path.id.as_str()),
        id: path.config_id,
    };
    let result = super::REQUEST_META
        .scope(meta, async {
            state.handler.on_get_task_push_config(req).await
        })
        .await;
    match result {
        Ok(cfg) => json_response(&cfg),
        Err(e) => rest_error_response(&e, Some(&path.id)),
    }
}

async fn handle_list_push_configs(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Path(path): Path<TaskIdPath>,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    let req = ListTaskPushNotificationConfigsRequest {
        tenant: tenant_from_headers(&headers),
        task_id: TaskId::from(path.id.as_str()),
        page_size: None,
        page_token: None,
    };
    let result = super::REQUEST_META
        .scope(meta, async {
            state.handler.on_list_task_push_configs(req).await
        })
        .await;
    match result {
        Ok(resp) => json_response(&resp),
        Err(e) => rest_error_response(&e, Some(&path.id)),
    }
}

async fn handle_delete_push_config(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Path(path): Path<PushConfigPath>,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    let req = DeleteTaskPushNotificationConfigRequest {
        tenant: tenant_from_headers(&headers),
        task_id: TaskId::from(path.id.as_str()),
        id: path.config_id,
    };
    let result = super::REQUEST_META
        .scope(meta, async {
            state.handler.on_delete_task_push_config(req).await
        })
        .await;
    match result {
        Ok(()) => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap(),
        Err(e) => rest_error_response(&e, Some(&path.id)),
    }
}

async fn handle_get_extended_agent_card(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
) -> Response {
    let meta = super::RequestMeta::from_header_map(&headers);
    let req = GetExtendedAgentCardRequest {
        tenant: tenant_from_headers(&headers),
    };
    let result = super::REQUEST_META
        .scope(meta, async {
            state.handler.on_get_extended_agent_card(req).await
        })
        .await;
    match result {
        Ok(card) => json_response(&card),
        Err(e) => rest_error_response(&e, None),
    }
}

fn json_response<T: serde::Serialize>(value: &T) -> Response {
    let body = serde_json::to_string(value).unwrap_or_default();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}
