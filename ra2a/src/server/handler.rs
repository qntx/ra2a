//! Request handler trait and JSON-RPC dispatcher for the A2A server.
//!
//! Defines the [`RequestHandler`] trait (the server-side interface for all
//! A2A JSON-RPC methods) and the [`handle_request`] dispatcher that routes
//! incoming JSON bodies to the appropriate method.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use super::ServerState;
use super::events::Event;
use crate::error::{JsonRpcError, Result};
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, GetTaskPushConfigParams, JsonRpcErrorResponse,
    JsonRpcRequest, JsonRpcSuccessResponse, ListTaskPushConfigParams, ListTasksRequest,
    ListTasksResponse, Message, MessageSendParams, Task, TaskIdParams, TaskPushConfig,
    TaskQueryParams,
};

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
    async fn on_set_task_push_config(&self, params: TaskPushConfig) -> Result<TaskPushConfig>;

    /// Handles the `tasks/pushNotificationConfig/get` request.
    async fn on_get_task_push_config(
        &self,
        params: GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig>;

    /// Handles the `tasks/pushNotificationConfig/list` request.
    async fn on_list_task_push_config(
        &self,
        params: ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>>;

    /// Handles the `tasks/pushNotificationConfig/delete` request.
    async fn on_delete_task_push_config(&self, params: DeleteTaskPushConfigParams) -> Result<()>;

    /// Handles the `tasks/list` request.
    ///
    /// Default implementation returns an empty list. Override to provide
    /// filtering/pagination backed by your [`TaskStore`](super::TaskStore).
    async fn on_list_tasks(&self, _params: ListTasksRequest) -> Result<ListTasksResponse> {
        Ok(ListTasksResponse::default())
    }

    /// Returns the authenticated extended agent card.
    ///
    /// Aligned with Go's `OnGetExtendedAgentCard`. The default implementation
    /// returns [`A2AError::ExtendedCardNotConfigured`](crate::error::A2AError::ExtendedCardNotConfigured).
    async fn on_get_extended_agent_card(&self) -> Result<AgentCard> {
        Err(crate::error::A2AError::ExtendedCardNotConfigured)
    }
}

/// Response type for `message/send` operations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    let request: JsonRpcRequest<serde_json::Value> = if let Ok(req) = serde_json::from_str(request_body) { req } else {
        let error_response = JsonRpcErrorResponse::new(None, JsonRpcError::parse_error());
        return Ok(serde_json::to_string(&error_response)?);
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
        // Streaming methods (message/stream, tasks/resubscribe) are handled
        // exclusively by the SSE endpoint, aligned with Go's handleStreamingRequest.
        "message/stream" | "tasks/resubscribe" => Err(JsonRpcError::invalid_request(
            "Streaming methods must be called via the SSE endpoint",
        )
        .into()),
        "tasks/list" => {
            let params = parse_params::<ListTasksRequest>(&request)?;
            let resp = handler.on_list_tasks(params).await?;
            serialize_success(&id, &resp)
        }
        "tasks/pushNotificationConfig/set" => {
            let params = parse_params::<TaskPushConfig>(&request)?;
            let config = handler.on_set_task_push_config(params).await?;
            serialize_success(&id, &config)
        }
        "tasks/pushNotificationConfig/get" => {
            let params = parse_params::<GetTaskPushConfigParams>(&request)?;
            let config = handler.on_get_task_push_config(params).await?;
            serialize_success(&id, &config)
        }
        "tasks/pushNotificationConfig/list" => {
            let params = parse_params::<ListTaskPushConfigParams>(&request)?;
            let configs = handler.on_list_task_push_config(params).await?;
            serialize_success(&id, &configs)
        }
        "tasks/pushNotificationConfig/delete" => {
            let params = parse_params::<DeleteTaskPushConfigParams>(&request)?;
            handler.on_delete_task_push_config(params).await?;
            serialize_success(&id, &serde_json::Value::Null)
        }
        "agent/getAuthenticatedExtendedCard" => {
            let card = handler.on_get_extended_agent_card().await?;
            serialize_success(&id, &card)
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
