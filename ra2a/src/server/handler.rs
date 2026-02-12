//! Request handler trait and JSON-RPC dispatcher for the A2A server.
//!
//! Defines the [`RequestHandler`] trait (the server-side interface for all
//! A2A JSON-RPC methods) and the [`handle_request`] dispatcher that routes
//! incoming JSON bodies to the appropriate method.

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::error::{JsonRpcError, Result};
use crate::types::{
    DeleteTaskPushNotificationConfigParams, GetTaskPushNotificationConfigParams,
    JsonRpcErrorResponse, JsonRpcRequest, JsonRpcSuccessResponse,
    ListTaskPushNotificationConfigParams, Message, MessageSendParams, Task, TaskIdParams,
    TaskPushNotificationConfig, TaskQueryParams,
};

use super::ServerState;
use super::events::Event;

/// A boxed stream of events for streaming responses.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event>> + Send>>;

/// Trait defining the interface for handling A2A JSON-RPC requests.
///
/// Implement this trait to customize how your server handles incoming requests.
/// The `DefaultRequestHandler` provides a standard implementation that coordinates
/// between the `AgentExecutor`, `TaskStore`, and `QueueManager`.
#[async_trait]
pub trait RequestHandler: Send + Sync {
    /// Handles the `message/send` request (non-streaming).
    async fn on_message_send(&self, params: MessageSendParams) -> Result<SendMessageResponse>;

    /// Handles the `message/stream` request (streaming).
    async fn on_message_stream(&self, params: MessageSendParams) -> Result<EventStream>;

    /// Handles the `tasks/get` request.
    async fn on_get_task(&self, params: TaskQueryParams) -> Result<Task>;

    /// Handles the `tasks/cancel` request.
    async fn on_cancel_task(&self, params: TaskIdParams) -> Result<Task>;

    /// Handles the `tasks/resubscribe` request.
    async fn on_resubscribe(&self, params: TaskIdParams) -> Result<EventStream>;

    /// Handles the `tasks/pushNotificationConfig/set` request.
    async fn on_set_push_notification_config(
        &self,
        params: TaskPushNotificationConfig,
    ) -> Result<TaskPushNotificationConfig>;

    /// Handles the `tasks/pushNotificationConfig/get` request.
    async fn on_get_push_notification_config(
        &self,
        params: GetTaskPushNotificationConfigParams,
    ) -> Result<TaskPushNotificationConfig>;

    /// Handles the `tasks/pushNotificationConfig/list` request.
    async fn on_list_push_notification_config(
        &self,
        params: ListTaskPushNotificationConfigParams,
    ) -> Result<Vec<TaskPushNotificationConfig>>;

    /// Handles the `tasks/pushNotificationConfig/delete` request.
    async fn on_delete_push_notification_config(
        &self,
        params: DeleteTaskPushNotificationConfigParams,
    ) -> Result<()>;
}

/// Response type for `message/send` operations.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum SendMessageResponse {
    /// A task was created or updated.
    Task(Task),
    /// A direct message response (no task created).
    Message(Message),
}

impl From<Task> for SendMessageResponse {
    fn from(task: Task) -> Self {
        Self::Task(task)
    }
}

impl From<Message> for SendMessageResponse {
    fn from(message: Message) -> Self {
        Self::Message(message)
    }
}

/// Dispatches a raw JSON-RPC request body to the appropriate
/// [`RequestHandler`](super::RequestHandler) method and returns the serialized response.
pub async fn handle_request(state: &ServerState, request_body: &str) -> Result<String> {
    // Parse the request envelope
    let request: JsonRpcRequest<serde_json::Value> = match serde_json::from_str(request_body) {
        Ok(req) => req,
        Err(_) => {
            let error_response = JsonRpcErrorResponse::new(None, JsonRpcError::parse_error());
            return Ok(serde_json::to_string(&error_response)?);
        }
    };

    let id = request.id.clone();
    let handler = &state.handler;

    // Dispatch to the appropriate handler method
    let result: Result<String> = match request.method.as_str() {
        "message/send" => {
            let params = parse_params::<MessageSendParams>(&request)?;
            let resp = handler.on_message_send(params).await?;
            serialize_success(&id, &resp)
        }
        "tasks/get" => {
            let params = parse_params::<TaskQueryParams>(&request)?;
            let task = handler.on_get_task(params).await?;
            serialize_success(&id, &task)
        }
        "tasks/cancel" => {
            let params = parse_params::<TaskIdParams>(&request)?;
            let task = handler.on_cancel_task(params).await?;
            serialize_success(&id, &task)
        }
        "tasks/resubscribe" => {
            let params = parse_params::<TaskIdParams>(&request)?;
            // Resubscribe returns a stream, but for non-streaming endpoint
            // we just acknowledge. The actual streaming happens via SSE.
            let _stream = handler.on_resubscribe(params).await?;
            serialize_success(&id, &serde_json::Value::Null)
        }
        "tasks/pushNotificationConfig/set" => {
            let params = parse_params::<TaskPushNotificationConfig>(&request)?;
            let config = handler.on_set_push_notification_config(params).await?;
            serialize_success(&id, &config)
        }
        "tasks/pushNotificationConfig/get" => {
            let params = parse_params::<GetTaskPushNotificationConfigParams>(&request)?;
            let config = handler.on_get_push_notification_config(params).await?;
            serialize_success(&id, &config)
        }
        "tasks/pushNotificationConfig/list" => {
            let params = parse_params::<ListTaskPushNotificationConfigParams>(&request)?;
            let configs = handler.on_list_push_notification_config(params).await?;
            serialize_success(&id, &configs)
        }
        "tasks/pushNotificationConfig/delete" => {
            let params = parse_params::<DeleteTaskPushNotificationConfigParams>(&request)?;
            handler.on_delete_push_notification_config(params).await?;
            serialize_success(&id, &serde_json::Value::Null)
        }
        "agent/getAuthenticatedExtendedCard" => {
            serialize_success(&id, &*state.agent_card)
        }
        _ => Err(JsonRpcError::method_not_found(&request.method).into()),
    };

    match result {
        Ok(json) => Ok(json),
        Err(e) => {
            let rpc_err = e.to_jsonrpc_error();
            Ok(serde_json::to_string(&JsonRpcErrorResponse::new(
                Some(id),
                rpc_err,
            ))?)
        }
    }
}

/// Extracts and deserializes params from a JSON-RPC request.
fn parse_params<T: serde::de::DeserializeOwned>(
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<T> {
    match &request.params {
        Some(p) => Ok(serde_json::from_value(p.clone())?),
        None => Err(JsonRpcError::invalid_params("Missing params").into()),
    }
}

/// Serializes a success response.
fn serialize_success<T: serde::Serialize>(
    id: &crate::types::RequestId,
    result: &T,
) -> Result<String> {
    let response = JsonRpcSuccessResponse::new(Some(id.clone()), result);
    Ok(serde_json::to_string(&response)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::JsonRpcErrorCode;

    #[test]
    fn test_method_not_found() {
        let error = JsonRpcError::method_not_found("unknown/method");
        assert_eq!(error.code, JsonRpcErrorCode::MethodNotFound as i32);
    }
}
