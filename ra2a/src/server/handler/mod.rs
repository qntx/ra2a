//! Request handler trait, implementations, and JSON-RPC dispatcher.
//!
//! - [`RequestHandler`] — trait for handling all A2A JSON-RPC methods
//! - [`DefaultRequestHandler`] — standard implementation coordinating executor, stores, queues
//! - [`InterceptedHandler`] — decorator applying [`CallInterceptor`](super::CallInterceptor)s

mod default;
mod intercepted;

use std::pin::Pin;

use async_trait::async_trait;
pub use default::DefaultRequestHandler;
use futures::Stream;
pub use intercepted::InterceptedHandler;

use super::ServerState;
use super::event::Event;
use crate::error::{JsonRpcError, Result};
use crate::jsonrpc::{self, JsonRpcErrorResponse, JsonRpcRequest, JsonRpcSuccessResponse};
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, GetTaskPushConfigParams, ListTaskPushConfigParams,
    ListTasksRequest, ListTasksResponse, MessageSendParams, SendMessageResult, Task, TaskIdParams,
    TaskPushConfig, TaskQueryParams,
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
    async fn on_message_send(&self, params: MessageSendParams) -> Result<SendMessageResult>;

    /// Handles the `message/stream` request (streaming).
    async fn on_message_stream(&self, params: MessageSendParams) -> Result<EventStream>;

    /// Handles the `tasks/get` request.
    async fn on_get_task(&self, params: TaskQueryParams) -> Result<Task>;

    /// Handles the `tasks/cancel` request.
    async fn on_cancel_task(&self, params: TaskIdParams) -> Result<Task>;

    /// Handles the `tasks/resubscribe` request.
    async fn on_resubscribe(&self, params: TaskIdParams) -> Result<EventStream>;

    /// Handles the `tasks/pushNotificationConfig/set` request.
    ///
    /// Default returns [`A2AError::PushNotificationNotSupported`](crate::error::A2AError::PushNotificationNotSupported).
    async fn on_set_task_push_config(&self, _params: TaskPushConfig) -> Result<TaskPushConfig> {
        Err(crate::error::A2AError::PushNotificationNotSupported)
    }

    /// Handles the `tasks/pushNotificationConfig/get` request.
    ///
    /// Default returns [`A2AError::PushNotificationNotSupported`](crate::error::A2AError::PushNotificationNotSupported).
    async fn on_get_task_push_config(
        &self,
        _params: GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig> {
        Err(crate::error::A2AError::PushNotificationNotSupported)
    }

    /// Handles the `tasks/pushNotificationConfig/list` request.
    ///
    /// Default returns [`A2AError::PushNotificationNotSupported`](crate::error::A2AError::PushNotificationNotSupported).
    async fn on_list_task_push_config(
        &self,
        _params: ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        Err(crate::error::A2AError::PushNotificationNotSupported)
    }

    /// Handles the `tasks/pushNotificationConfig/delete` request.
    ///
    /// Default returns [`A2AError::PushNotificationNotSupported`](crate::error::A2AError::PushNotificationNotSupported).
    async fn on_delete_task_push_config(&self, _params: DeleteTaskPushConfigParams) -> Result<()> {
        Err(crate::error::A2AError::PushNotificationNotSupported)
    }

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

/// Dispatches a raw JSON-RPC request body to the appropriate
/// [`RequestHandler`](super::RequestHandler) method and returns the serialized response.
pub async fn handle_request(state: &ServerState, request_body: &str) -> Result<String> {
    // Parse the request envelope
    let request: JsonRpcRequest<serde_json::Value> =
        if let Ok(req) = serde_json::from_str(request_body) {
            req
        } else {
            let error_response = JsonRpcErrorResponse::new(None, JsonRpcError::parse_error());
            return Ok(serde_json::to_string(&error_response)?);
        };

    let id = request.id.clone();
    let handler = &state.handler;

    // Dispatch to the appropriate handler method
    let result: Result<String> = match request.method.as_str() {
        jsonrpc::METHOD_MESSAGE_SEND => {
            let params = parse_params::<MessageSendParams>(&request)?;
            let resp = handler.on_message_send(params).await?;
            serialize_success(&id, &resp)
        }
        jsonrpc::METHOD_TASKS_GET => {
            let params = parse_params::<TaskQueryParams>(&request)?;
            let task = handler.on_get_task(params).await?;
            serialize_success(&id, &task)
        }
        jsonrpc::METHOD_TASKS_CANCEL => {
            let params = parse_params::<TaskIdParams>(&request)?;
            let task = handler.on_cancel_task(params).await?;
            serialize_success(&id, &task)
        }
        // Streaming methods are handled exclusively by the SSE endpoint.
        jsonrpc::METHOD_MESSAGE_STREAM | jsonrpc::METHOD_TASKS_RESUBSCRIBE => Err(
            JsonRpcError::invalid_request("Streaming methods must be called via the SSE endpoint")
                .into(),
        ),
        jsonrpc::METHOD_TASKS_LIST => {
            let params = parse_params::<ListTasksRequest>(&request)?;
            let resp = handler.on_list_tasks(params).await?;
            serialize_success(&id, &resp)
        }
        jsonrpc::METHOD_PUSH_CONFIG_SET => {
            let params = parse_params::<TaskPushConfig>(&request)?;
            let config = handler.on_set_task_push_config(params).await?;
            serialize_success(&id, &config)
        }
        jsonrpc::METHOD_PUSH_CONFIG_GET => {
            let params = parse_params::<GetTaskPushConfigParams>(&request)?;
            let config = handler.on_get_task_push_config(params).await?;
            serialize_success(&id, &config)
        }
        jsonrpc::METHOD_PUSH_CONFIG_LIST => {
            let params = parse_params::<ListTaskPushConfigParams>(&request)?;
            let configs = handler.on_list_task_push_config(params).await?;
            serialize_success(&id, &configs)
        }
        jsonrpc::METHOD_PUSH_CONFIG_DELETE => {
            let params = parse_params::<DeleteTaskPushConfigParams>(&request)?;
            handler.on_delete_task_push_config(params).await?;
            serialize_success(&id, &serde_json::Value::Null)
        }
        jsonrpc::METHOD_GET_EXTENDED_AGENT_CARD => {
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
pub(crate) fn parse_params<T: serde::de::DeserializeOwned>(
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<T> {
    match &request.params {
        Some(p) => Ok(serde_json::from_value(p.clone())?),
        None => Err(JsonRpcError::invalid_params("Missing params").into()),
    }
}

/// Serializes a success response.
fn serialize_success<T: serde::Serialize>(
    id: &crate::jsonrpc::RequestId,
    result: &T,
) -> Result<String> {
    let response = JsonRpcSuccessResponse::new(Some(id.clone()), result);
    Ok(serde_json::to_string(&response)?)
}
